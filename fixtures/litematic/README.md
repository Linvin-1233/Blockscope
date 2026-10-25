# Local Litematic Fixtures

Place real `.litematic` files in this directory for local validation. The
`.litematic` files are ignored by git and must not be committed without
permission.

Useful coverage:

- litematic versions 1 through 7
- multiple regions and negative sizes
- all-air or empty regions
- chests, signs, and other block entities
- palette sizes around bit-width boundaries (1, 2, 16, 17, 32, 33)
- a large structure near the deployment limit

Run the opt-in validation:

```powershell
$env:LITEMATIC_FIXTURES_DIR = (Resolve-Path .\fixtures\litematic)
cargo test --workspace -- --ignored
```

Or inspect a single file:

```powershell
cargo run -p blockscope-cli -- inspect .\fixtures\litematic\example.litematic --json
```
