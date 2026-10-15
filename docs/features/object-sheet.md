# Object Sheet

Lays cut-outs out on pages, either flat or as fold-over standees, and exports a PDF.

## Items
Each item has:
- an image or asset;
- a label (default "Cut-Out");
- a size label (default Medium);
- a scale: 10–400 % in steps of 10, default 100;
- copies: 1–99, default 1.

Items can be added from the Object Cutter or from the Library, one at a time or in bulk.

You can change size, scale and copies, remove an item, or Clear all (with confirmation).

The sheet is autosaved in `workspaces` under the key `object_sheet`.

## Options
- **Format:** Letter or A4. The default comes from the `object_page_format` setting.
- **Mode:** Flat (default) or Standee.

## Sizing (core::pack::shelf)
- An item's longest side, in points, is `SIZE_PT[size] * scale/100`. Aspect ratio is preserved.
- In standee mode, the block is the image flipped vertically on top and the upright image below, with no gap. A dashed fold line sits at the seam: 6 px dashes, colour (120,120,120,220), 1 px. This doubles the height.

## Shelf packer
- Margin 18 pt, gap 8 pt.
- Rectangles are sorted tallest first, then placed left to right on shelves.
- When an item doesn't fit horizontally, it starts a new shelf at `top += shelf_h + gap`.
- When a shelf overflows the page, packing continues on a new page.
- Items larger than the usable area are **reported as skipped**. Python skipped them silently.

## PDF
- Images are embedded with alpha preserved. Export runs as a background Job.

## MCP
- `object_sheet_list`, `object_sheet_add`, `object_sheet_update`, `object_sheet_remove`, `object_sheet_clear`
- `object_sheet_layout`
- `object_sheet_export_pdf {path, format?, mode?}`

## Intentional changes
- UI consistency pass (2026-10-11, branch `ui/consistency`): the page preview is in the centre and the list of cut-outs on the right, like the other views; Add images and Export PDF are in the top bar, paper options and the pager in the toolbar band, Clear all in the panel footer.
- The page format default now comes from a setting. Python hard-coded Letter.
- Items that are too large are reported instead of silently dropped.
- Clear all now asks for confirmation.
- Python stacked a new trace callback on every rebuild, which leaked; there are no trace leaks now.
- The preview caches scaled images.

## Parity checklist (Python `ui/object_sheet_panel.py`, `cutout_sheet_generator.py`)
- [x] Item list with size, scale, copies, remove, clear
- [x] Flat and standee modes (`make_standee_block`, cutout_compositing.py:64). The standee block matches Python exactly (golden test).
- [x] Shelf packer constants and order (`_layout` :79-140): exact match on all 20 cases of the golden fixture `shelf_pack.json`
- [x] Letter/A4 output, paged preview (the standee preview shows the mirrored half and the fold line), PDF export (background job)
