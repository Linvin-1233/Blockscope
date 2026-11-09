# crates.io publishing

Status: implementation complete on `chore/crates-io-publish`; waiting for merge,
the first tag, and the crates.io token.

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

## Checklist

- [x] Add license, description, repository, homepage, and README metadata to the
  workspace crates.
- [x] Add versions to internal path dependencies.
- [x] Add `.github/workflows/publish-crates.yml`.
- [x] Add `.github/scripts/publish-crates.sh`.
- [x] Update `just bump` to keep path dependency versions in lockstep.
- [x] Document the bootstrap, rate limit, and trusted publishing migration.
- [ ] Add `CARGO_REGISTRY_TOKEN` to the `crates-io` GitHub environment.
- [ ] Merge this branch to `main`.
- [ ] Cut a new patch tag; `v0.5.0` predates the publish metadata.
- [ ] Confirm all eight crates appear on crates.io.
- [ ] Configure trusted publishing for each crate and remove the long-lived
  token.
