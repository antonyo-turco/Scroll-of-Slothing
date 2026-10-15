# Performance budgets

The map paginator does the heaviest pixel work in the app, so it carries the budgets. The benchmark drives the service exactly as the UI does:

```
cargo bench -p sleuth-service --bench perf              # report
cargo bench -p sleuth-service --bench perf -- --strict  # exit 1 if a step is over budget
cargo bench -p sleuth-service --bench perf -- --size 3000x2000
```

Source: `crates/service/benches/perf.rs`. The input is a synthetic 6000×4500 px (27 MP) parchment map with a grid every 100 px, about a 300 dpi battle map. The page size is A4 at 1" squares.

## Budgets and the 2026-10-10 baseline
Release (`bench`) profile, the user's Windows 11 development machine.

| Step | Budget | Baseline | Notes |
|---|---|---|---|
| Open file (import + 2048 px proxy) | 4 s | 163 ms | The proxy is cached per blob. |
| Preview, no filters | 250 ms | 57 ms | The proxy re-encoded as PNG: the floor of every preview. |
| Preview, filter presets (median of 17) | 400 ms | 297 ms | Debounced 120 ms in the UI; stale results are dropped. |
| Preview, slowest preset ("Max detail B&W") | 800 ms | 550 ms | |
| Preview, FFT grid removal (cold) | 2.5 s | 603 ms | Runs once per grid size, then cached. |
| Preview, FFT grid removal (cached) | 250 ms | 107 ms | |
| Page preview (0.5 px/pt) | 250 ms | 47 ms | |
| Export PDF, 32 pages | 20 s | 9.2 s | A cancellable background job with progress. |
| Export PDF with full-resolution grid removal | 45 s | 16.2 s | Ditto. |

Every preview runs off the UI thread (`spawn_blocking`), and the UI keeps panning and zooming client-side while it computes. The budgets are for that background latency, not for UI blocking.

## When to re-run
Re-run after touching `crates/core/src/maps`, `crates/pdf/src/map_tiles.rs`, `crates/service/src/maps.rs` or the image helpers. If a step goes over budget, investigate before merging. If you change a budget, update this table.
