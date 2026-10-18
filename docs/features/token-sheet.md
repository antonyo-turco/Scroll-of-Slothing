# Token Sheet

Collects tokens, packs them onto Letter or A4 pages, previews the pages, and exports a PDF.

## Size labels
1 in = 72 pt. These are shared by the whole app.

| Label | Python label | pt | inches |
|---|---|---|---|
| Tiny | Tiny (0.5 inch) | 36 | 0.5 |
| Small | Small (0.8 inch) | 57.6 | 0.8 |
| Medium | Medium (1 inch) | 72 | 1 |
| Large | Large (2 inch) | 144 | 2 |
| Huge | Huge (3 inch) | 216 | 3 |
| Gargantuan | Gargantuan (4 inch) | 288 | 4 |

The legacy importer maps the Python labels to these.

## Items
Each item has:
- a source: an asset id, or a transient image (for example straight from the cutter or an upload);
- a label;
- a size (default Medium);
- copies (1–99, default 1).

Items can be added in these ways:
- uploaded on this page (png/jpg/jpeg/bmp/webp);
- with "Add to sheet" from the Token Cutter;
- from the Library, one asset or in bulk.

Item actions: change size, change copies, remove, Clear all.

**Clear all now asks for confirmation.** Python did not ask.

The sheet is autosaved to the `workspaces` table under `token_sheet`. Python lost the sheet when the app closed.

## Page options
- Format: Letter or A4. Default comes from the `token_page_format` setting.
- Cut border: on or off. Default comes from the `token_cut_border` setting.
  - When on, each token gets a circle with radius `min(w,h)/2`, drawn in the cut-line style.
  - When off, the preview shows a light grey `#cccccc` cell outline instead. The outline is not printed.

## Packing algorithm (core::pack::token_grid)
Reference: Python `pdf_generator.py`, `utils.py`.

**Grid**
- Margin is 10 pt. The packing cell is a fixed size (see the size change below).
- `cols = floor((W - 20) / cell)`, and rows the same way.
- The grid is centred on the page: `offset = (W - cols*cell)/2`.
- Python, with a 72 pt cell: Letter 612×792 gives 8×10 cells, A4 595.28×841.89 gives 8×11.

**Placement**
1. Expand each item by its copy count.
2. Sort by size, largest first.
3. Each token needs `cells = max(1, round(size_pt/cell))`.
4. Place each token at the first free spot, scanning row by row from the top-left.
5. If nothing fits, start a new page. If it still doesn't fit on an empty page, **report the item as skipped** (Python only printed a warning).

**Position on the page:** `x = off_x + col*cell`, `y = H - off_y - (row+cells)*cell`, in PDF bottom-left coordinates.

**Justification (app path, after packing; `token_grid::justify`, 2026-10-18).** `pack` places tokens edge to edge, which makes them hard to cut apart, and leaves the page's spare width and height as a strip around the block. Justify shares that space out evenly between token columns and between token rows, like justified text:
- gap = `(usable − occupied) / (columns − 1)`, capped at **18 pt (¼ in)** so a half-empty page doesn't scatter; the same vertically; the block is then centred inside the 10 pt margins;
- each token moves by `gap × rank` of its grid column (and row), where ranks count the distinct columns tokens start on, so neighbours always end up at least one gap apart and nothing overlaps (property test);
- sizes, pages and which tokens fit are unchanged: it only redistributes space that was empty.
- Typical gaps: A4, Medium tokens ≈ 12 pt between columns and rows; Letter, Medium ≈ 2 pt between columns (Letter's grid is nearly full width) and ≈ 6 pt between rows.
- The Python-parity `Legacy` mode and its golden fixture don't use it.

**Size change from Python**
- Python used a 72 pt cell, so Tiny and Small tokens printed at 1 inch.
- The packer takes `cell_pt` as a parameter, and the app uses **36 pt (0.5 in)**. Each token takes `ceil(size_pt/36)` cells per side and is drawn at its **true size**, centred in its cell block.
- The golden parity test runs the packer with `cell_pt = 72` and must reproduce Python's positions exactly.
- With 36 pt, the vertical centring offset can differ from Python's. For example, on Letter there are 21 half-inch rows instead of 10 inch rows, so the top offset is 18 pt instead of 36 pt. The app path is checked with property tests: no overlap, inside the margins, every copy either placed or reported, and page count no higher than with 72 pt cells.

## PDF
- Images are embedded directly as PNG, with alpha flattened on white. **Python wrote a temporary JPEG for each token.**
- The cut circle uses the cut-line style.
- Export runs as a Job with progress, so the UI never blocks.

## Preview
- Paged (◀ Page N of M ▶). The page is scaled to fit the pane.
- Pages are rendered client-side from the layout JSON, using thumbnail-sized images from `sleuth://blob/<hash>?w=`.
- The layout is recomputed in Rust, debounced 120 ms (Python used 180 ms).

## MCP
- `token_sheet_list`
- `token_sheet_add {items:[{asset_id|path, size?, copies?, label?}]}`
- `token_sheet_update {index, size?, copies?}`
- `token_sheet_remove {index}`
- `token_sheet_clear`
- `token_sheet_layout` returns pages and placements, plus any skipped items.
- `token_sheet_export_pdf {path, format?, cut_border?}`

## Parity checklist (Python `ui/token_sheet_panel.py`, `pdf_generator.py`, `image_manager.py`)
- [x] Upload, list, thumbnails, size, copies, remove, clear (clear asks for confirmation)
- [x] Letter/A4 and cut border defaults from settings
- [x] First-fit packing, largest first, centred grid (golden fixture `token_pack.json`, exact match in Legacy mode)
- [x] New page on overflow; items that can't fit are reported
- [x] Paged preview with Prev/Next
- [x] PDF export (background job); the cut circle in the cut-line style
- [x] Add from Cutter and Library, single and batch (`app.py:518-534`)

## Intentional changes
- UI consistency pass (2026-10-11, branch `ui/consistency`): the page preview is in the centre and the list of tokens on the right, like the other views; Add images and Export PDF are in the top bar, paper options and the pager in the toolbar band, Clear all in the panel footer.
- Half-inch packing grid: Tiny and Small print at their true size (see "Size change from Python" above).
- Justified pages: spare space is spread between tokens so they are easy to cut (see "Justification" above). Python packed them edge to edge.
- An item that can never fit is reported once and creates no pages (Python opened an empty page per copy).
- Images are embedded as PNG with transparency (Python wrote a temporary JPEG per token, flattened on white).
- Non-square images are fitted inside their cell, centred, in both the preview and the PDF. Python stretched them to a square.
- The sheet is autosaved and survives restarts.
- Clear all asks for confirmation.
