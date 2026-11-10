# crates.io publishing

Status: `0.5.1` is published under the old `litematic-*` library names.
`0.6.0` renames those crates to `blockscope-*` and is waiting for merge, a tag,
and the first publish under the new namespace.

## Design

- Publish only from a `v*` tag whose commit is on `origin/main`, with a clean
  checkout and a passing `just check`.
- Publish the eight current crates in dependency order with
  `cargo publish --locked`.
- Keep the publish list explicit so a new workspace member cannot be uploaded
  accidentally.
- Skip versions that already exist, retry after crates.io rate limits, and
  re-run safely after a partial failure.
- Keep every internal path dependency pinned to the workspace version, and make
  `just bump` update those pins.
- Publish the `blockscope-*` names from `0.6.0`; yank the old `litematic-*`
  `0.5.1` versions after the new publish succeeds.

## Checklist

- [x] Add license, description, repository, homepage, and README metadata to the
  workspace crates.
- [x] Add versions to internal path dependencies.
- [x] Add `.github/workflows/publish-crates.yml`.
- [x] Add `.github/scripts/publish-crates.sh`.
- [x] Update `just bump` to keep path dependency versions in lockstep.
- [x] Document the bootstrap, rate limit, and trusted publishing migration.
- [x] Publish `0.5.1` to crates.io under the `litematic-*` names.
- [x] Add `CARGO_REGISTRY_TOKEN` to the repository secrets.
- [x] Rename the library crates to `blockscope-*` for `0.6.0`.
- [ ] Merge the rename branch to `main`.
- [ ] Cut and push the `v0.6.0` tag.
- [ ] Confirm all eight `blockscope-*` crates appear on crates.io.
- [ ] Yank the `litematic-*` `0.5.1` versions.
- [ ] Configure trusted publishing for each crate and remove the long-lived
  token.
