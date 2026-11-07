# Issue: Server-side preview for `000002`-class structures

**Suggested labels:** enhancement, performance, service
**Status:** Deferred for future implementation
**Design record:** `tasks/server-preview-spec.md`,
`tasks/server-preview-contract.md`, `docs/ideas/3d-preview.md`
**Measurements:** `docs/performance-preview.md`

## Summary

Add a low-latency server-side preview that lets an upper-layer service offer
camera and turntable-keyframe tuning without rebuilding a full textured render
for every drag. The 2026-11-07 spike showed that a **color-only blockout
preview** meets the 300 ms gate for `000002`-class structures on the CPU.
Textured stateless preview and textured resident sessions do not fit the 4 GB
reference card.

## Why the original direction is blocked

The first design reused the client-jar material set and the full mesh cache.
At 384x384, warm cache, with `000002` (39.82M triangles):

| Path | Warm p95 | Peak working set |
|---|---:|---:|
| textured stateless, CPU | 1.77-1.82 s | 5.96 GB |
| textured stateless, GPU | 2.52-3.16 s | not recorded |
| textured streamed, CPU | 3.55 s | 216 MB |
| textured resident session | `BACKEND_UNAVAILABLE` | needs 3.66 GB; 1.29 GB budget |

Lowering resolution does not help: 384x384 to 128x128 changes CPU render time
by about 5% because the path is geometry-bound. `000001` (128.63M textured
triangles) is 9.4-10.5 s warm and is not interactive in any measured mode.

## Why color-only blockout works

The built-in color catalog removes client-jar model detail:

| Case | Warm p50 | Warm max | Peak working set |
|---|---:|---:|---:|
| `000002` color, 384x384, CPU | 208-213 ms | 265 ms | 333 MB |
| `000001` color, 384x384, CPU | 4.82-4.99 s | 4.99 s | not recorded |

`000002` color-only has 2.30M triangles instead of 39.82M and meets the 300 ms
gate with no GPU and no client jar. `000001` still has 91.57M triangles and
must not be offered as interactive preview. A color-only resident session at
256x256 measured about 27 ms/frame with 201.9 MB resident; it is an optional
optimization, not a requirement.

## Proposed scope

- Add a color-only **blockout preview** path that uses the built-in color
  catalog and never loads the client jar.
- Keep the existing `render` / `animate` output unchanged. Preview is a
  separate request type or a preview-only command; it must not be confused
  with the existing terminal `preview` alias.
- Enforce a hard color-mesh geometry budget. `000002`-class must pass;
  `000001`-class must fail with a stable resource-limit error before meshing.
- Default to a stateless preview at 384x384, supersample 1, CPU. This meets
  the gate without a session. Use the existing scene/mesh cache and the
  cache/memory policy from the design record.
- Optional follow-up: a 256x256 resident color-only session for lower latency
  on `000002`-class structures. It adds session lifecycle, idle timeout, and
  routing work and is not required for the first slice.
- Textured preview is opt-in only on hardware with enough VRAM (roughly 8 GB
  for `000002`-class); it is not the default and not required for this issue.
- `000001`-class structures use the existing batch `--draft` path, with no
  interactive claim.
- The upper service must label the result as a blockout preview. It shows
  geometry, block colors, framing, and rotation speed, not final textures.
  Texture detail may differ in the final render.
- The upper service owns debounce/latest-only handling; the engine does not
  preempt or cancel jobs.

## Acceptance criteria

- A color-only preview of `000002` at 384x384, warm cache, p95 under 300 ms on
  the reference host, with no client jar and no GPU.
- Peak working set stays near the measured 333 MB for the color-only
  `000002` path; no preview builds a multi-gigabyte full mesh.
- `000001`-class structures fail the preview geometry budget with an
  actionable, stable error before mesh construction.
- Final `render` / `animate` output and PNG goldens are unchanged.
- The service protocol documents the capability, limits, error codes, and the
  blockout-preview caveat.
- Tests cover: budget rejection, cache hit/miss behavior, framing parity
  against a color-only `render` at the same camera/options, and no frame bytes
  over stdout.
- Performance results are recorded in `docs/performance-preview.md`.

## Open questions

1. What exact color-mesh triangle/quad budget separates `000002`-class from
   `000001`-class? The measured points are 2.30M triangles (pass) and 91.57M
   (fail); a measured crossover is still needed.
2. Should the default preview size be 384x384 or 256x256? 384x384 passes with
   a 12-20% margin; 256x256 gives more headroom.
3. Should the preview be a new `preview` request type, a `mode` field on
   `render`, or a separate `--draft` CLI path? The blockout variant changes the
   material contract, so it needs a fresh decision.
4. Should `RenderPreset` still be shared with the textured final render? The
   camera/keyframes can be shared, but the appearance cannot.
5. Is a resident color-only session worth the lifecycle complexity, or is the
   stateless 384x384 path enough?
6. Which GPU VRAM threshold makes textured preview an opt-in capability, and
   how should `hello` advertise it?

## References

- `docs/performance-preview.md` — full spike tables and verdict.
- `tasks/server-preview-spec.md` — deferred original spec and gate outcome.
- `tasks/server-preview-contract.md` — deferred wire/CLI/cache contract.
- `docs/ideas/3d-preview.md` — problem framing and spike outcome.
- `out/preview-spike/` — raw results and reproduction scripts (local,
  gitignored).
