#!/usr/bin/env bash
# Publish the Blockscope workspace crates to crates.io from a clean release tag.
#
# The workflow supplies TAG, DRY_RUN, and CARGO_REGISTRY_TOKEN. The same guards
# run here so a local invocation cannot upload from a dirty tree, from a tag
# that is not on origin/main, or from a tag that does not match the workspace
# version.

set -euo pipefail

readonly CRATES=(
  blockscope-nbt
  blockscope-scene
  blockscope-core
  blockscope-mesh
  blockscope-material
  blockscope-render
  blockscope-video
  blockscope-cli
)

readonly API_ROOT="https://crates.io/api/v1"
readonly USER_AGENT="Blockscope-crates-io-publisher/1.0 (https://github.com/Linvin-1233/Blockscope)"
readonly NEW_CRATE_RATE_LIMIT_SLEEP=630
readonly UPDATE_RATE_LIMIT_SLEEP=65
readonly INDEX_WAIT_SECONDS=600

TAG="${TAG:-}"
DRY_RUN="${DRY_RUN:-false}"
MAX_RATE_LIMIT_SLEEP="${MAX_RATE_LIMIT_SLEEP:-900}"

metadata_file="$(mktemp)"
summary_file="$(mktemp)"
cleanup() {
  rm -f "$metadata_file" "$summary_file"
}
trap cleanup EXIT

log() {
  printf '==> %s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

record() {
  printf '| %s | %s | %s |\n' "$1" "$2" "$3" >>"$summary_file"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

package_version() {
  local crate="$1"
  jq -r --arg name "$crate" '
    .packages[] | select(.name == $name) | .version
  ' "$metadata_file"
}

# --- Preflight ---------------------------------------------------------------

[ -n "$TAG" ] || die "TAG is required (for example v0.6.0)"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+ ]] || die "TAG must look like v1.2.3, got '$TAG'"
[[ "$MAX_RATE_LIMIT_SLEEP" =~ ^[0-9]+$ ]] || die "MAX_RATE_LIMIT_SLEEP must be a number"
case "$DRY_RUN" in
  true | false) ;;
  *) die "DRY_RUN must be true or false, got '$DRY_RUN'" ;;
esac

require_command cargo
require_command curl
require_command git
require_command jq

[ -n "$(git status --porcelain)" ] && {
  git status --short >&2
  die "working tree is dirty"
}

git fetch --force --no-tags origin main:refs/remotes/origin/main >/dev/null
git merge-base --is-ancestor HEAD refs/remotes/origin/main ||
  die "HEAD is not on origin/main"
cargo xtask verify-tag "$TAG"

remote_tag="$(git ls-remote --tags origin "refs/tags/${TAG}^{}" | awk '{print $1}')"
if [ -z "$remote_tag" ]; then
  remote_tag="$(git ls-remote --tags origin "refs/tags/${TAG}" | awk '{print $1}')"
fi
[ -n "$remote_tag" ] || die "tag $TAG does not exist on origin"
[ "$remote_tag" = "$(git rev-parse HEAD)" ] ||
  die "origin tag $TAG points to $remote_tag, not $(git rev-parse HEAD)"

cargo metadata --locked --no-deps --format-version 1 >"$metadata_file"

publishable="$(jq -r '.packages[] | select(.publish != []) | .name' "$metadata_file" | sort)"
expected="$(printf '%s\n' "${CRATES[@]}" | sort)"
[ "$publishable" = "$expected" ] || {
  printf 'publishable workspace members:\n%s\n' "$publishable" >&2
  die "the publish list in this script is out of date; add or remove crates deliberately"
}

missing_metadata="$(
  jq -r '
    .packages[]
    | select(.publish != [])
    | select((.description // "") == "" or (.license // "") == "")
    | .name
  ' "$metadata_file"
)"
[ -z "$missing_metadata" ] || die "missing description or license: $missing_metadata"

# The publish order must follow the internal dependency graph, and every
# internal dependency must require the same version that is being published.
for index in "${!CRATES[@]}"; do
  crate="${CRATES[$index]}"
  version="$(package_version "$crate")"
  [ -n "$version" ] || die "cannot read the version of $crate"

  while IFS=$'\t' read -r dep req; do
    [ -n "$dep" ] || continue
    dep_index=-1
    for candidate_index in "${!CRATES[@]}"; do
      if [ "${CRATES[$candidate_index]}" = "$dep" ]; then
        dep_index="$candidate_index"
        break
      fi
    done
    [ "$dep_index" -ge 0 ] || die "$crate depends on unpublished workspace crate $dep"
    [ "$dep_index" -lt "$index" ] || die "$crate must be published after $dep"
    case "$req" in
      "^$version" | "=$version") ;;
      *) die "$crate requires $dep $req, expected ^$version; update the path dependency version" ;;
    esac
  done < <(
    jq -r --arg name "$crate" '
      .packages[]
      | select(.name == $name)
      | .dependencies[]
      | select(.path != null)
      | [.name, .req]
      | @tsv
    ' "$metadata_file"
  )
done

# --- crates.io helpers -------------------------------------------------------

API_STATUS=""

api_status() {
  local path="$1"
  local attempt code
  for attempt in 1 2 3 4 5; do
    code="$(curl -sS -o /dev/null -w '%{http_code}' \
      -H "User-Agent: $USER_AGENT" \
      "$API_ROOT/$path" || true)"
    case "$code" in
      200 | 404) API_STATUS="$code"; return 0 ;;
      000 | 429 | 5??) sleep $((attempt * 5)) ;;
      *) die "crates.io API returned HTTP $code for $path" ;;
    esac
  done
  die "crates.io API did not answer for $path"
}

crate_version_exists() {
  local crate="$1"
  local version="$2"
  api_status "crates/${crate}/${version}"
  [ "$API_STATUS" = "200" ]
}

crate_name_exists() {
  local crate="$1"
  api_status "crates/${crate}"
  [ "$API_STATUS" = "200" ]
}

wait_for_index() {
  local crate="$1"
  local version="$2"
  local deadline=$((SECONDS + INDEX_WAIT_SECONDS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if cargo info "${crate}@${version}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 5
  done
  die "timed out after ${INDEX_WAIT_SECONDS}s waiting for ${crate} ${version} in the registry index"
}

publish_one() {
  local crate="$1"
  local version
  version="$(package_version "$crate")"
  [ -n "$version" ] || die "cannot read the version of $crate"

  if [ "$DRY_RUN" = "true" ]; then
    log "validate $crate $version (dry run)"
    files="$(cargo package -p "$crate" --locked --no-verify --list)"
    count="$(printf '%s\n' "$files" | wc -l | tr -d ' ')"
    [ "$count" -gt 0 ] || die "package $crate has no files"
    record "$crate" "$version" "validated ${count} files (dry run)"
    return 0
  fi

  if crate_version_exists "$crate" "$version"; then
    log "skip $crate $version (already on crates.io)"
    record "$crate" "$version" "already published"
    return 0
  fi

  # crates.io allows a burst of five new crate names, then one every ten
  # minutes. New versions of existing crates have a much larger burst.
  local rate_limit_sleep="$UPDATE_RATE_LIMIT_SLEEP"
  if ! crate_name_exists "$crate"; then
    rate_limit_sleep="$NEW_CRATE_RATE_LIMIT_SLEEP"
  fi

  local attempt=1 output status
  while true; do
    log "publish $crate $version (attempt $attempt)"
    set +e
    output="$(cargo publish -p "$crate" --locked 2>&1)"
    status=$?
    set -e

    if [ "$status" -eq 0 ]; then
      break
    fi

    if grep -qiE 'already (been )?uploaded|already exists' <<<"$output"; then
      log "skip $crate $version (cargo reports it is already uploaded)"
      record "$crate" "$version" "already published"
      return 0
    fi

    if grep -qiE '429|too many|rate.?limit|try again after' <<<"$output"; then
      if [ "$MAX_RATE_LIMIT_SLEEP" -eq 0 ] ||
        [ "$rate_limit_sleep" -gt "$MAX_RATE_LIMIT_SLEEP" ]; then
        printf '%s\n' "$output" >&2
        die "crates.io rate limit hit; re-run the workflow after the time printed above"
      fi
      log "crates.io rate limit hit; sleeping ${rate_limit_sleep}s before retrying $crate"
      sleep "$rate_limit_sleep"
      attempt=$((attempt + 1))
      continue
    fi

    printf '%s\n' "$output" >&2
    die "failed to publish $crate $version"
  done

  wait_for_index "$crate" "$version"
  log "published $crate $version"
  record "$crate" "$version" "published"
}

# --- Publish -----------------------------------------------------------------

workspace_version="$(package_version blockscope-nbt)"
log "publishing ${#CRATES[@]} crates at version ${workspace_version}"

for crate in "${CRATES[@]}"; do
  publish_one "$crate"
done

{
  echo "## crates.io publish"
  echo
  echo "| Crate | Version | Result |"
  echo "| --- | --- | --- |"
  cat "$summary_file"
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
