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
Used when the map already has grid lines drawn in. Options: "Keep" (default) or "Remove" (the Look tab's "Grid printed in the map").
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

## Automatic detection (new; `core::maps::detect`)
Both run on the working proxy (≤ 2048 px, 30–300 ms) and are buttons, never automatic.

**⌗ Detect grid** (Grid section, `map_detect_grid`) switches to Cell side with the detected square size and line offset.
- Each axis becomes an edge profile: the summed brightness steps per column or row, high-passed so only sharp peaks remain.
- That profile is probed at every candidate period with one Fourier term. A comb of lines scores high at its period P and its harmonics P/2, P/3, but near zero at 2P, so the largest period scoring within 80 % of the best is taken.
- Each line is then located, and a straight-line fit gives a sub-pixel period and offset.
- Rejected as not a grid:
  - periods under 12 px or under 1/80 of the image (JPEG's 8 px blocks, fine texture);
  - a best score under 4× the median;
  - edge energy on the lines under 2.5× the energy between them;
  - lines missing at more than 40 % of the predicted positions, or unevenly spaced.
- Texture fixes:
  - Strong texture can outscore a grid a few times its size, so the multiples 2P–4P compete on line contrast.
  - When a reading implies more than 60 squares across, the stronger of k interleaved line sets is taken as the grid. This handles grid lines drawn on stone blocks a third of a square wide.
  - When the two axes differ by 2× or 3× (running-bond floor tiles), the larger period is used for both.
- Squares are square, so the axes are averaged.
- Checked against the user's 20 library maps on 2026-10-11. The grid maps read 18.5–80 px, and the 150 px floor tiles of a gridless map were also picked up. "dwarven mine entrance 32x48" reads exactly 32 × 48 squares, and the hand-measured laboratory map (40.31 × 40.09) reads 40.2. Maps with no visible grid report "no grid".

**Fit to map** (Pages section, `map_fit_region`) sets the print region to the content.
- Each side is trimmed while its outermost line is one colour (≥ 98 % of pixels within 24 levels of the line's median): scanner bars, blank paper, an edge grid line.
- Each edge then snaps to the **nearest** grid line, so the region holds whole squares. Artwork overhanging a line by less than half a square is cut, as a hand-drawn region would be. On the laboratory map this reproduces the user's own 24 × 22-square region.
- A region covering the whole image becomes "whole map". Page exclusions are cleared.

## Print settings
Defaults come from Settings.

| Control | Default |
|---|---|
| Page | A4 / Letter / A3 (297×420 mm, maps only) |
| Orientation | Portrait / Landscape (default portrait) |
| 1 square = N inch | 1.0 |
| Calibration: a printed 1 in measures _ mm | 25.4 (factor = 25.4/mm) |
| Margin (in) | 0; shown in Paper, next to the cut lines |
| Overlap (in) | 0 |
| Align pages to grid | off |
| Cut lines (trim the white margin) | off (the `map_cut_border` setting); four lines in the cut-line style along every edge of each page's image, running on to the paper edge. Shown in the page preview too. Warns when the margin is under 0.2 in, because most printers clip that band; the warning's "Set margin to 0.2 in" is undone when cut lines are turned off again. |

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
- Prev / Next, "Page N of M", and a "Print this page" toggle (Python: "Include this page").
- A summary line: "X/Y selected · N pages · c×r grid".

## Tiling math (core::maps::tiling)
**Scale**
- `sq_pt = square_in × 72 × (25.4/calibration_mm)`
- `scale_x = sq_pt/cell_w`, `scale_y = sq_pt/cell_h`. The two axes are independent, so rectangular cells still print square.
- `page = format size, swapped when landscape` (`tiling::page_size_pt`)
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

**Page selection (`_axis_pages`)**: start at `floor((lo−origin)/step) − 1` and keep every page whose span overlaps the region by at least 1 px (or half the region, if smaller). Capped at 20000 iterations.

**Fewest pages (`tiling::fewest_pages`)**: fewer pages means fewer cuts. Columns and rows are independent, so each axis is minimised on its own, for both orientations:
- When aligning to the grid, the candidate page-grid shifts are whole grid squares within one page; otherwise they are 1/64-page steps.
- The result is the orientation and shift with the fewest pages. Ties keep portrait and the smallest shift, so a layout that is already minimal stays as it is.
- `map_layout` returns it as `fewest` only when it beats the current layout.
- The Pages section then shows "Landscape prints this on N pages instead of M" with **Use fewest pages**. That calls `map_fewest_pages`, which applies it and clears the page selections, because they refer to the old pages.

**Each tile** has `{col, row, crop rect (clipped to region), full page rect, page_x/y pt, w/h pt}`.

**Rendering**
- The crop is resampled with LANCZOS on a float source box, so tiles meet with sub-pixel accuracy and no seams.
- `y = H − margin − page_y − h`.
- Optional cut lines: the image's left and right edges as lines from the top to the bottom of the sheet, its top and bottom edges across the whole sheet (`map_tiles::trim_lines`).
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
- Save to library asks for the name and tags in one dialog; the tags are applied when the save job finishes.
- UI consistency pass (2026-10-11, branch `ui/consistency`): the settings panel was too crowded (about 1,600 px of controls in one scroll). It is now three tabs that follow the workflow: **Scale** (Detect grid, sizing mode, grid position, "Align on the map…"), **Pages** (the fewest-pages suggestion, paper, print area with Fit to map / Whole map / Draw… / Move pages…, the page pager and preview, and a collapsed **Advanced** with margin, overlap, calibration, align-to-grid and the fit check) and **Look** (printed grid lines, embedded-grid removal, filters). Picking a canvas tool switches to the tab it edits, and the tab is remembered while the app runs. File actions moved to the top bar, zoom to a floating cluster on the canvas, and Save to library is a dialog. When cut lines are on with a margin under 0.2 in, the warning has a one-click "Set margin to 0.2 in".
- Canvas tools are labelled by what they do: Pan, Print area (Python: region), Reference square, Align grid (Python: nudge) and Move pages. The active tool's instructions float over the top-left of the canvas.
- While the Reference square tool is active, the grid overlay and the page tiles are hidden, and a previous square is hidden while a new one is drawn, so the block underneath stays visible (alpha-test feedback).
- Reference square: side handles in addition to corner handles (alpha-test feedback).
- Zoom/pan are client-side transforms; the grid, region, tiles and reference block are vector overlays (Python re-rendered the composite with LANCZOS on every move).
- The whole paginator state (`MapDoc`) is autosaved in the `map_paginator` workspace and stored as the asset's `params_json`, so reopening restores grid, filters, region, page selection and print settings. Python restored only the image.
- Embedded-grid removal uses an f32 FFT and detects on the spectrum of R+G+B (the same as Python's mean of channel spectra), processing one channel at a time to bound memory. Output matches Python on 99.99 % of pixels.
- Export and Save as Asset are cancellable background jobs; PDF images are embedded from raw pixels (no temp files).
- DPI detection reads PNG `pHYs`, JPEG JFIF and BMP headers directly.
- A page must hold at least 1 px of the print region. Python counted pages that merely touched the region's edge, so a region of exactly 24 squares on 8-square pages, starting on a page boundary, could come out as 5 columns instead of 3 through float rounding. The PDF already skipped those blank pages, but the count, the numbering and the fewest-pages suggestion did not. The golden tiling fixture is unaffected.
- Grid detection and fit-to-map are new (Python had neither).
- Landscape pages and the fewest-pages suggestion are new (Python printed portrait only, from the region's corner).
- Cut lines extend through the margin to the paper edge instead of only outlining the image (Python drew a rectangle), so the white margin of a printer without borderless printing can be trimmed with a ruler or trimmer. The page preview shows them, and a hint asks for a margin of at least 0.2 in.
- A3 paper (user request, 2026-10-15). Python printed on A4 and Letter only. A3 uses the same formula as ReportLab's A4 (29.7 × 42 cm), and is offered for maps and AoE templates only; token, object and card sheets refuse it.
- The margin moved from Advanced into Paper, next to the cut lines (user request, 2026-10-15): the hint's "Set margin to 0.2 in" changed a field hidden in a collapsed section, so turning cut lines off left the white margin in place with no visible way back. Turning cut lines off now also restores the margin the hint replaced.

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
