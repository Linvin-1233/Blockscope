# Capability Map: Memory Work

**Status:** Draft for review

This map splits the two requested experiments into independently verifiable
modules. The second depends on the first for measurement and regression
protection.

| Module id          | Responsibility                                                                                                                | Depends on        |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| `memory-baseline`  | Allocation-peak harness, sparse/dense fixtures, budgets, and a recorded baseline for litematic parse and scene construction   | —                 |
| `sparse-litematic` | Compact/sparse litematic states plus a two-pass streaming reader that avoids the dense `BlockStates` array for sparse regions | `memory-baseline` |

Build order: `memory-baseline` -> `sparse-litematic`

## Why split

- `memory-baseline` changes tests and documentation only. It can land without
  touching production behavior and gives the second module a repeatable oracle.
- `sparse-litematic` changes the scene representation and the parser. It needs
  a baseline to prove the win and to catch dense-file regressions.
- The baseline also answers whether the remaining work is worth doing before
  the parser is rewritten. If the measured sparse gap is small, stop.

## Scope boundary

The 18.9 GB peak reported for the 9.4M-block exact render is dominated by the
mesh and projected-quad tables, not by parsing. `sparse-litematic` targets
large bounding boxes with low occupancy and oversized decompressed NBT. It does
not promise a reduction of the dense exact-render peak; that remains the job of
`--stream-mesh`, compact attributes, and resolution controls.

A 2026-10-28 baseline on the current release binary measured parse+scene at
108-132 MB for the 9.4M-67M-block fixtures and streamed render at 407 MB for
`000001`. A synthetic `1024^3` sparse region with one block used 495 MB, but
`512^3` is already rejected by the default 100M volume limit. The sparse parser
is therefore conditional on the S0 spike showing a win for a target the product
actually needs; it is not the first memory problem to solve.

## Approval gates

1. Review this map: module boundaries and build order.
2. Review `tasks/memory-baseline-spec.md`.
3. Review `tasks/sparse-litematic-spec.md`.
4. After approval, write `tasks/plan.md` and the task list in `tasks/todo.md`.
