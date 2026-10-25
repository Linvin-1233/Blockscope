# Golden Manifests

These files are generated from synthetic fixtures by
`crates/cli/tests/golden.rs`. Regenerate them after an intentional manifest
change:

```powershell
$env:UPDATE_GOLDEN = "1"
cargo test -p blockscope-cli --test golden
Remove-Item Env:UPDATE_GOLDEN
```

Review the diff before committing. Golden files must not contain private
`.litematic` data or Minecraft assets.
