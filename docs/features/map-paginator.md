# Map Paginator

Takes a battle-map image, works out its grid, cleans it up, and prints it at true scale (for example 1 square = 1 inch) across several pages, with optional overlap and cut borders.

## Loading
- From a file (png, jpg, jpeg, bmp, webp), by paste, by drag and drop, or with "Open in editor" from the Library.
- Clicking the empty canvas opens the file browser.
- A **working proxy** of at most 2000 px (`WORK_MAX`) drives all interaction. Export always uses full resolution.
- DPI detection: if the image's `dpi` metadata is greater than 1 it is used; otherwise 70 is assumed (`DEFAULT_DPI`).

## View
- Mouse wheel zooms ×1.1 around the cursor. The −/+ buttons zoom ×1.25. Zoom is clamped to 0.1–12. "Fit" fits the map to the view.
- Pan with a middle-button drag. Left-drag pans too when no other tool is active.
- **Zoom and pan are client-side transforms of the proxy, with the grid and overlays drawn as vector graphics.** Python re-scaled the whole composite with LANCZOS on every mouse move.

## Grid model
- `GridModel{cell_w, cell_h, offset_x, offset_y, color, thickness, opacity, visible, style}`
- Defaults: 70, 70, 0, 0, (40,40,40), 1, 160, true, solid.
- **Sizing modes.** Each produces `(cell_w, cell_h, off_x, off_y)` in image pixels:
  - **Rows × Cols**: `w/cols`, `h/rows`. Defaults are rows 35 (range 1–999) and cols 55. "Square cells" (on by default) sets both sides to `min(cw, ch)`.
  - **Reference block**: drag a box over a known N×N block (N defaults to 3, range 1–99). The box snaps to a square unless Shift is held. `cell = rect side / N`, and the offset is `left % cell`.
  - **PPI**: `cell = ppi × square_in`. Default 70, range 10–1200.
  - **Cell side**: `cell = px`. Default 70, range 2–2000.
- A manual X/Y offset is added on top of every mode.
- **Nudge-align**: drag over one real cell, and `offset = (left % cw) − base_offset`.
- **Line positions**: `start = off − floor(off/step)·step`, then step up to `extent + 0.001`. If `step < 0.5`, no lines are drawn.
- **Styles**: dash patterns as (on, off) pairs, where c is the cell size and t the thickness:

  | Style | Pattern |
  |---|---|
  | solid | continuous |
  | dashed | (max(2, .18c), max(2, .12c)) |
  | dotted | (t, max(2, .10c)) |
  | broken | (max(4, .45c), max(2, .12c)) |

- Grid appearance controls:
  - colour picker;
  - thickness 1–10;
  - opacity 0–255;
  - Show grid.

## Embedded grid removal (FFT notch)
Used when the map already has grid lines drawn in. Options: "Keep" (default) or "Remove embedded grid".
1. FFT each RGB channel (with fftshift). Detection uses the mean magnitude across channels.
2. Peak ratio = `2 + 2·strength`, with strength 0.8.
3. For each axis, the expected fundamental frequency is `f = span/cell`. Skip the axis if `f < 6` or `f > span/2 − 4`.
4. Search bins from 0.8f to 1.2f. Pick the bin whose first 6 harmonics have the highest mean prominence. Prominence = 3×3 peak divided by the median of neighbours 3–12 bins away along the axis.
5. Require at least 2 harmonics above the ratio. Otherwise return the image unchanged.
6. Zero a 5×5 square at every harmonic up to Nyquist, at ± positions.
7. Restore the central 13×13 low-frequency block. Inverse FFT, then clip to 0–255.
8. Skip entirely if `cell ≤ 3 px`.

**Performance**: the preview uses the proxy, in Rust, off the UI thread. Export runs at full resolution inside the cancellable export job.

## Perspective de-warp
1. Use the reference-block rectangle. Drag its 4 corner handles onto the real block (hit tolerance 10 px). The new app also has a handle in the middle of each side: dragging it moves that side parallel to itself, so an axis-aligned rectangle stays one.
2. Corners are ordered TL, TR, BR, BL by sorting on x+y and x−y.
3. Cell size = max(average horizontal edge, average vertical edge) / N.
4. Map the quad to an axis-aligned square. Project the image corners to size the output. Cap the output at 8000 px, with 2 attempts that shrink `cell_px`.
5. Perspective transform with bicubic sampling and a white fill. The homography comes from an 8×8 linear solve with partial pivoting; a pivot below 1e-12 raises "degenerate corner selection".
6. Afterwards, set the cell side and offset from the result.
7. "Undo de-warp" restores the previous image.

Before the warp is applied, the preview grid comes from the quad (average edge length / N).

## Filters
See `map-filters.md`.

**Export composite order**: remove grid (if chosen) → filters → grid overlay.

## Print settings
Defaults come from Settings.

| Control | Default |
|---|---|
| Page | A4 / Letter |
| 1 square = N inch | 1.0 |
| Calibration: a printed 1 in measures _ mm | 25.4 (factor = 25.4/mm) |
| Margin (in) | 0 |
| Overlap (in) | 0 |
| Align pages to grid | off |
| Cut border | off; drawn as a rectangle in the cut-line style |

**Fit check**: target area in feet, default 3×2. `squares = round(region/cell)`, and the printed size in inches is compared with the target in either orientation, showing ✓ or ✗.

## Region and pages
**Print region**
- Drag a rectangle. It snaps to grid intersections unless Shift is held.
- 8 resize handles, and the rectangle can be moved.
- "Whole map" clears the region.

**Page tiles**
- "Show page tiles" overlays the tiles on the map.
- Click a tile to include or exclude it. Excluded tiles are shaded and marked ✕. "Include all" resets them.

**Move layout**
- Drag the whole page grid.
- It snaps to 0, ¼, ½, ¾ or 1 page within 10 screen px. Shift moves it freely.
- "Reset layout" returns it to the start.

**Preview pane**
- Prev / Next, "Page N of M", and an "Include this page" toggle.
- A summary line: "X/Y selected · N pages · c×r grid".

## Tiling math (core::maps::tiling)
**Scale**
- `sq_pt = square_in × 72 × (25.4/calibration_mm)`
- `scale_x = sq_pt/cell_w`, `scale_y = sq_pt/cell_h`. The two axes are independent, so rectangular cells still print square.
- `usable = page − 2·margin`
- `overlap_pt = min(overlap_in·72, 0.9·usable)`

**Free layout**
- `tile_px = usable/scale`
- `step_px = (usable − overlap)/scale`
- `origin = region top-left + page offset`

**Grid-aligned layout**
- `cells = max(1, floor(usable/sq_pt))`
- `ov_cells = min(cells−1, round(overlap/sq_pt))`
- `tile = cells·cell_px`, `step = (cells−ov_cells)·cell_px`
- `origin` is snapped onto a grid line: `origin + round((v−origin)/period)·period`

**Page selection (`_axis_pages`)**: start at `floor((lo−origin)/step) − 1` and keep every page whose span overlaps the region. Capped at 20000 iterations.

**Each tile** has `{col, row, crop rect (clipped to region), full page rect, page_x/y pt, w/h pt}`.

**Rendering**
- The crop is resampled with LANCZOS on a float source box, so tiles meet with sub-pixel accuracy and no seams.
- `y = H − margin − page_y − h`.
- Optional cut-border rectangle.
- Only included tiles are exported.

## Saving
- **Export Map PDF**: a background job with progress and cancel.
- **Save as Asset**: kind `map`. Stores:
  - the original;
  - the gridded composite;
  - the PDF;
  - `params_json`, holding the whole paginator state.

  If the map was opened from the Library, you choose Update / New / Cancel.
- Re-opening an asset restores the full state. Python restored only the image.

## MCP
| Tool | Input |
|---|---|
| `map_open` | `{source}` → `{session_id}` |
| `map_set_grid` | `{mode, ...}` |
| `map_set_style` | |
| `map_set_filters` | `{preset? \| pipeline?}` |
| `map_remove_grid` | `{on}` |
| `map_dewarp` | `{quad, n}` |
| `map_set_print` | |
| `map_set_region` | |
| `map_toggle_page` | |
| `map_layout` | returns the tiles |
| `map_preview_page` | `{index}` → PNG |
| `map_export_pdf` | `{path}` |
| `map_save_asset` | `{name, asset_id?}` |

## Intentional changes
- Canvas tools are labelled by what they do: Pan, Print area (Python: region), Reference square, Align grid (Python: nudge) and Move pages. The active tool's instructions are shown under the toolbar.
- While the Reference square tool is active, the grid overlay and the page tiles are hidden, and a previous square is hidden while a new one is drawn, so the block underneath stays visible (alpha-test feedback).
- Reference square: side handles in addition to corner handles (alpha-test feedback).
- Zoom/pan are client-side transforms; the grid, region, tiles and reference block are vector overlays (Python re-rendered the composite with LANCZOS on every move).
- The whole paginator state (`MapDoc`) is autosaved in the `map_paginator` workspace and stored as the asset's `params_json`, so reopening restores grid, filters, region, page selection and print settings. Python restored only the image.
- Embedded-grid removal uses an f32 FFT and detects on the spectrum of R+G+B (the same as Python's mean of channel spectra), processing one channel at a time to bound memory. Output matches Python on 99.99 % of pixels.
- Export and Save as Asset are cancellable background jobs; PDF images are embedded from raw pixels (no temp files).
- DPI detection reads PNG `pHYs`, JPEG JFIF and BMP headers directly.

## Parity checklist (Python `ui/map_paginator_panel.py`, `map_grid.py`, `map_pdf_generator.py`)
- [ ] Load via file, paste, drop or library; 2000 px proxy; DPI detection
- [ ] View zoom and pan
- [x] 4 sizing modes, square cells, manual offset, nudge-align (golden `grid_modes.json`)
- [ ] Grid styles and appearance
- [x] FFT removal: synthetic test plus golden comparison (99.99 % identical pixels; f32 FFT)
- [x] De-warp with an 8000 px cap, undo (golden `homography.json`; bicubic warp bit-exact)
- [ ] Print settings including calibration; fit check
- [ ] Region snapping and handles; tile include/exclude; layout drag with ¼ snapping
- [x] Tiling math: free and grid-aligned (golden `tiling.json`)
- [ ] Sub-pixel tile resampling, cut border, include set
- [ ] Export PDF; Save as Asset with Update/New
