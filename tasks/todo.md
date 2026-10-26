# Outstanding Tasks

**Status:** Active

Only unfinished work is listed here. Completed task details live in
`CHANGELOG.md` and git history.

## Pending verification

- [ ] Run the real-file smoke tests with
      `LITEMATIC_FIXTURES_DIR=... cargo test --workspace -- --ignored`.
- [ ] Review `docs/litematic-format.md` with a human.
- [ ] Measure `--greedy-mesh` on a real flat or repetitive build and decide
      whether it should become the default. It stays opt-in while random-palette
      scenes pay extra mesh time for little reduction.
