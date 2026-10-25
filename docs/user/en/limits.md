# Limits and Security

## Default Limits

| Item | Default |
|---|---|
| Input file | 64 MiB, regular files only |
| Decompressed NBT | 256 MiB |
| NBT depth / tags | 64 / 4,000,000 |
| NBT string / list / array | 1 MiB / 4,000,000 / 64,000,000 |
| Total NBT allocation budget | 512 MiB |
| Regions | 1,024 |
| Volume per region | 100,000,000 blocks |
| Total region volume | 200,000,000 blocks |
| Palette / block entities / entities / ticks | 1,000,000 per region |
| Region coordinate range | ±30,000,000 |
| Mesh | 5,000,000 quads / 20,000,000 vertices / 65,536 pages |
| Render pixels | 16,777,216; 33,554,432 after supersampling |
| Threads | At most 256 |
| Textures | 2048x2048, 256 MiB decoded, 8 MiB per asset |

Mesh limits are further reduced by the available-memory budget.

## `--force-limits`

`--force-limits` raises:

- input size and total NBT allocation budget
- scene/region volume
- mesh quads, vertices, and pages
- render pixel limits

It does not disable the thread, texture, NBT-depth, or NBT-tag limits. The
option can exhaust memory; use it only with trusted input.

## Security Boundary

- `serve`/`supervise` communicate over stdio only; there is no network listener
- Inputs must be regular files; directories, FIFOs, devices, and oversized files
  are rejected
- Input paths and output directories are controlled by the caller; run the
  process as a dedicated low-privilege user
- Do not expose the service to untrusted clients without validating paths and
  resource packs
- Resource packs and client jars are supplied by the deployer; the repository
  does not distribute Minecraft assets

## Error Codes

See [Engine protocol v1](../../engine-protocol.md) for the full error
table and retryability. Common errors:

- `INPUT_TOO_LARGE`: input or resource limit exceeded
- `INVALID_REQUEST`: malformed request, relative path, or non-regular file
- `INVALID_RENDER_OPTIONS`: bad size, lighting, background, or thread count
- `MESH_LIMIT_EXCEEDED` / `PAGE_LIMIT_EXCEEDED`: mesh limits
- `INVALID_NBT` / `INVALID_LITEMATIC`: corrupt or unsupported file
- `ASSET_SOURCE_REQUIRED`: `materials` needs a resource pack or client jar

## See Also

- [CLI Reference](./cli.md)
- [Service Mode](./service.md)
- [Troubleshooting](./troubleshooting.md)
- [中文](../limits.md)
