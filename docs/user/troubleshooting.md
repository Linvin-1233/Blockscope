---
title: "Troubleshooting"
---

## Common Problems

| Symptom | Action |
|---|---|
| `ASSET_SOURCE_REQUIRED` | `materials` needs `--client-jar`, `--assets`, or a self-contained pack |
| `INPUT_TOO_LARGE` | File exceeds the input cap; check integrity or adjust LOD/limits |
| `MESH_LIMIT_EXCEEDED` / `PAGE_LIMIT_EXCEEDED` | Use an explicit `--lod 2`/`4`, or `--force-limits` with trusted input |
| Everything renders as flat colors | No resource pack, or missing models/textures; run `materials --json` |
| Many missing textures | Client jar version mismatch, or an incomplete overlay pack |
| `INVALID_RENDER_OPTIONS: threads` | `--threads` or the request exceeds 256 |
| `INVALID_NBT` / `INVALID_LITEMATIC` | Corrupt, unsupported, or malformed file; run `validate` first |
| `serve` never prints `hello` | Check worker stderr and resource-pack paths |
| Cannot write output | Ensure the container/service user can write to `outputDir` |
| Slow render or out of memory | Use `capacity`, reduce resolution/supersampling, lower concurrency, or enable LOD |

## Diagnostic Commands

```bash
blockscope validate model.litematic
blockscope inspect model.litematic --json
blockscope stats model.litematic --json
blockscope materials model.litematic --client-jar client.jar --json
blockscope capacity --json
```

The manifest diagnostics from `inspect` report metadata mismatches, overlapping
regions, ignored entities/ticks, and similar issues. `validate` is the quickest
first check.

## Reporting

Include:

- OS, Rust version, and `blockscope --version`
- The full command or protocol request, with sensitive paths removed
- Actual and expected behavior
- Error code and relevant stderr
- A minimal, shareable synthetic fixture when possible

For security issues, contact the maintainer through the GitHub profile instead
of opening a public issue with sensitive details.

## See Also

- [Limits and Security](./limits.md)
- [Service Mode](./service.md)
- [中文](/zh/user/troubleshooting)
