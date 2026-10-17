# AoE templates

Prints area-of-effect templates at map scale, for example a 20 ft Fireball at 1 in per 5 ft square. Templates can be printed one at a time or packed several to a sheet.

## Shapes (D&D 5e)
Units: `px_per_ft = px_per_square / feet_per_square`. Coordinates start at the top-left plus padding.

| Shape | Dimensions | Footprint (ft) | Geometry |
|---|---|---|---|
| circle | radius r | 2r × 2r | ellipse inscribed in the footprint |
| cone | length L (end width = L) | L × L | triangle (0, L/2), (L, 0), (L, L); apex on the left edge |
| line | length L × width W | L × max(W, 0.01) | rectangle |
| square | side S | S × S | rectangle |

## Render (core::aoe::render_template)
**Defaults**
- 5 ft per square
- 150 px per square
- fill (220,70,40) at alpha 80
- outline (150,25,0), 4 px wide
- grid (60,60,60) at alpha 110
- supersample ×2

**Sizing**
- Padding is `line_width + 4`.
- If either side would exceed 6000 px, `px_per_square` is reduced and the render is retried.
- The view sets `px_per_square = clamp(150 × inch, 60, 300)`.

**Drawing**
- Grid lines pass through the shape's origin.
- Layers, bottom to top: white background, grid, fill, outline, label.
- Label:
  - font size `max(12, 0.26 × px_per_square)`;
  - drawn with a 2 px white halo;
  - uses the bundled Liberation Sans (Python used Arial, with DejaVuSans as fallback).
- The final image is downscaled with LANCZOS by the supersample factor.

**Default labels:** "20 ft radius", "15 ft cone", "30 x 5 ft line", "15 ft cube".

## Presets
**Built-in (11)**
| Preset | Shape | Length (ft) | Width (ft) |
|---|---|---|---|
| Sphere — 10 ft radius | circle | 10 | 5 |
| Sphere — 20 ft (Fireball) | circle | 20 | 5 |
| Sphere — 30 ft radius | circle | 30 | 5 |
| Cone — 15 ft (Burning Hands) | cone | 15 | 5 |
| Cone — 30 ft | cone | 30 | 5 |
| Cone — 60 ft (Cone of Cold) | cone | 60 | 5 |
| Line — 30 × 5 ft | line | 30 | 5 |
| Line — 60 × 5 ft | line | 60 | 5 |
| Line — 100 × 5 ft (Lightning) | line | 100 | 5 |
| Cube — 15 ft | square | 15 | 5 |
| Cube — 20 ft | square | 20 | 5 |

**User presets**
- Shown with a ★ prefix and stored in the `aoe_presets` table.
- **Save preset…** asks before overwriting an existing name. **Delete** works on ★ presets only.
- Config keys: `shape`, `a_ft`, `b_ft`, `feet_per_square`, `fill_rgb`, `fill_alpha`, `line_rgb`, `line_width`, `grid_on`, `grid_rgb`, `grid_alpha`, `label_on`, `label`, `inch`, `page_format`, `name`.
- The legacy importer reads the old `aoe_presets.json`.

## Controls (defaults)
**Shape and size**
- shape
- size 1–2000, step 5 (default 20)
- width 1–200 (default 5), shown for line only
- feet per square 1–100 (default 5)
- inches per square (default 1.0, minimum 0.05)
- page A4 / Letter / A3

**Style**
- fill colour and opacity
- outline colour, width 1–12
- grid on/off, colour, opacity
- label on/off and text. The text auto-fills until you edit it.

**Info line:** footprint in feet, size in inches on paper, page count.

## Sheet packing (core::pack::maxrects)
- Pages render at 150 DPI, with a 0.2 in gap and a 0.2 in margin.
- Each item's size in px is `img_px / cell_px × inch × dpi`.
- Items larger than a page are reported as skipped.
- Footprints are padded by the gap, clamped to the usable area, and sorted largest area first.
- **Algorithm:** MaxRects, best-short-side-fit.
  - When a rectangle is placed, each free rectangle it overlaps is split into up to 4 pieces.
  - Any free rectangle contained in another is pruned.
  - Existing pages are tried first; otherwise a new page is opened.

## Preview and export
**Preview modes**
- "Current template": the template with its page rectangles outlined.
- "Full sheet": all pages side by side.

**Sheet list**
- Add current, multi-select, Remove selected, Clear sheet, set the gap.
- The sheet is autosaved in `workspaces` under the key `aoe.sheet`.

**Export**
- Export the sheet as a PDF (each page is a full-page image).
- Export the current template as a PDF, through the map tiler with margin 0.
- Save the current template as a PNG.

## MCP
| Tool | Input / output |
|---|---|
| `aoe_render` | `{shape, a_ft, b_ft?, feet_per_square?, inch?, style...}` → PNG preview + `{footprint_ft, inches, pages}` |
| `aoe_presets_list` | |
| `aoe_presets_save` | |
| `aoe_presets_delete` | |
| `aoe_sheet_add` | |
| `aoe_sheet_list` | |
| `aoe_sheet_remove` | |
| `aoe_sheet_clear` | |
| `aoe_sheet_export_pdf` | `{path}` |
| `aoe_export_template_pdf` | |
| `aoe_export_png` | |

## Intentional changes
- UI consistency pass (2026-10-11, branch `ui/consistency`): exports in the top bar; the inspector uses the shared sections (Preset, Shape & size, Scale & paper, Style, Sheet).
- Labels use the bundled Liberation Sans (metric-compatible with Arial) instead of the system Arial/DejaVu fallback.
- Built-in presets cannot be deleted; saving over an existing user preset needs explicit `overwrite` (the UI asks, as Python did).
- The sheet is autosaved in the `aoe.sheet` workspace instead of living only in memory.
- Exports run as cancellable jobs and report skipped (too large) templates in the job result.
- A3 paper (11.6929 × 16.5354 in) is new (user request, 2026-10-15); Python had A4 and Letter only.

## Parity checklist (Python `aoe_templates.py`, `ui/aoe_template_panel.py`)
- [x] 4 shapes; geometry matches the golden fixture `aoe_geometry.json`
- [x] Render defaults, supersampling, the 6000 px cap (golden renders within MAE 0.45)
- [ ] Labels, halo, auto-fill
- [ ] 11 built-in presets; user presets with save, overwrite confirm, delete
- [x] MaxRects packing (golden `maxrects.json`); skipped items reported
- [ ] Sheet preview and current-template preview
- [ ] Sheet PDF, template PDF, PNG
