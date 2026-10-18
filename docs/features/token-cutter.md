# Token Cutter

Frames any image inside a token border (round, square, hexagonal, octagonal or decagonal) and outputs a token image. The output size equals the border's pixel size.

## Inputs
- Drop a file onto the view, browse for one, paste with Ctrl+V, or "Open in editor" from the Library.
- Accepted formats: png, jpg, jpeg, bmp, gif, webp.
- Paste accepts any of these, tried in order:
  1. an image on the clipboard;
  2. a file list on the clipboard;
  3. clipboard text that is a path to an image.

## Controls (inspector)
| Section | Control | Default | Range |
|---|---|---|---|
| Shape | radio (icons): Circle / Square / Hexagon / Octagon / Decagon, above the border strip | the current border's shape | picking a shape selects that shape's border of the same kind (plain, thin or no border) |
| Border | thumbnail strip of the current shape's borders | first border | circle: bundled `roll20.png` (442²), `roll20_greyscale.png` (256²), their thin versions `roll20_thin.png` / `roll20_greyscale_thin.png`; square, hexagon, octagon, decagon: `roll20_<shape>.png` and `roll20_<shape>_thin.png` (gold); a **No border** per shape (`none.png`, blank); plus user-added borders |
| Background | none (transparent) / colour picker / eyedropper | none | the eyedropper samples the source image |
| Ring | ring thickness (content inset), slider with − / + | 0 % | 0–50 % of the radius |
| Filters | the map filter panel limited to token filters: preset dropdown, toggles, sliders with − / +, reorder | none | see "Filters" |
| Info | name, size, tags: asked in the Save to library dialog | file stem (or the asset's), Medium (or the asset's), the asset's tags | the size labels |
| Framing | pan (drag), zoom (wheel ×1.1 per notch), reset | centred, ×1 | zoom 0.1–10 |

**Actions**
- Save PNG…
- Save to Library: a dialog asks for the name, size and tags (prefilled from the file name or, for a library asset, from the asset), and, if the token was opened from the Library, offers Update existing / Save as new / Cancel. Save is disabled while the name is empty.
- Add to Token Sheet

## Algorithm (core::token)
Reference: Python `ui/token_compositing.py`.

1. **Fit with transform.**
   - Cover scale: `base = max(tw/iw, th/ih)`, multiplied by the zoom.
   - Resize to `iw*base` × `ih*base`, centred, then offset by the pan.
   - Pan is stored in border-pixel units: `pan -= drag / (display/max(bw,bh))`.
   - Crop and paste onto a transparent RGBA canvas the size of the border.
   - **Filters.** The enabled token filters run on this border-sized window, colour channels only (alpha is kept). The border is composited afterwards, so it is never filtered. Running at border size makes preview and export identical.
2. **Content mask.** The border's shape (see "Shapes"), inset by `ring_inset_pct/100 * radius`, where radius = `min(w,h)/2`. Circle: an ellipse in the inset box (unchanged from Python). Multiply it into the base image's alpha.
3. **Background.** If a background colour is set, fill it inside the same mask, underneath the image.
4. **Composite.** Alpha-composite the base, then the border on top.
5. **Outer clip.** Clip to the border's outer edge, in its shape. The centre comes from the alpha bounding box; the radius is the largest that fits the box (circle: `min(w,h)/2`). A blank border has no alpha, so there's no outer clip: the content mask is the token's edge.

## Filters
- `sleuth_core::token::TOKEN_FILTERS`: the colour and tone map filters (docs/features/map-filters.md): autocontrast, contrast, lightness, clarity, sharpen, sepia, grayscale, posterize, edge_boost. The ink-saving B&W filters (adaptive, threshold, dither, line_art, canny) are for printing maps; other kinds in a token pipeline are ignored.
- Presets: the map presets that use only token filters (8 of 18, including "None (clear all)", Most legible, Posterized 3/4, Soft and Clean grayscale, Sepia print, Faded parchment). `token_filter_catalog` (service, Tauri command, MCP tool) lists the defs and these presets.

## Shapes
`sleuth_core::token::TokenShape`, each scaled so it touches the square canvas:
- `circle`;
- `square`;
- `hex`: a pointy-top regular hexagon (vertices at top and bottom, √3/2 as wide as tall; it fits Roll20's vertical hex grid);
- `octagon`: flat sides at the top, bottom, left and right;
- `decagon`: flat top and bottom, vertices left and right (cos 18° ≈ 0.951 as tall as wide).

The polygons are regular (side count, first side's normal, circumradius); `app/src/lib/token-shapes.ts` mirrors them for the UI.
- A border's shape is part of the border list (`Border.shape`). Built-ins declare it; an added border's is detected from its alpha (`detect_shape`): the opaque silhouette (each row filled between its first and last opaque pixel) is compared with each shape fitted to the same box, and the closest wins (ties go to the circle).
- The polygon gold borders are made from the round ones by `tools/token_borders/shape_borders.py`: each pixel is measured with the shape's polygon norm and mapped back onto the ring at the same angle, so the frame keeps the ring's width, bevel and upper-left lighting along every side.
- **No border** (one per shape) is a blank 442² image: the image is just cut to the shape. Ring thickness still insets it.
- The token sheet detects each item's shape from its image when it is added and cuts along it (see token-sheet.md).

## Thin borders
`roll20_thin.png` and `roll20_greyscale_thin.png` are the bundled rings squeezed radially to 45 % of their width; the outer rim stays put, so the image circle grows from 77 % to 90 % of the radius (about 35 % more image area). They were generated once from the originals; the texture is resampled, not redrawn.

The transparency checkerboard for preview uses an 8 px square in colours (204,204,204) and white.

## Performance design
- Pan and zoom are shown live as a client-side canvas transform of the preview proxy, with the border overlaid.
- The real compose runs in Rust on the **preview proxy**, debounced (80 ms), and tagged with a generation number so stale results are dropped.
- Export, save and add-to-sheet compose once from the full-resolution original, scaled down to the border size.

## Persistence
- `params_json = {border, bg, ring_inset_pct, pan, zoom, filters}` is stored (`filters` defaults to empty for older assets) on the asset, so re-editing restores the exact framing.
- The unsaved editor state is autosaved to the `workspaces` table.

## MCP
- `token_compose {source: path|asset_id, border?, bg?, ring_inset_pct?, pan?, zoom?, filter_preset? | filters?, name?, size?, save_to_library?, add_to_sheet?, out_png?}`
  - Returns `{asset_id?, png_path?}` and a preview image.
- `token_filter_catalog`
- `token_borders_list`
- `token_borders_add {path}`

## Intentional changes
- UI consistency pass (2026-10-11, branch `ui/consistency`): the source image (with the colour picker) is the inspector's first section instead of a separate left column; Open image, Paste, Save PNG, Add to sheet and Save to library are in the top bar; zoom has −/+ buttons next to Reset view.
- Live preview: the compose runs in Rust off the UI thread, resampling only the visible window of the source (Python resized the whole image on every motion event). At most one render is in flight; the latest drag or zoom state wins, so the UI never queues up or freezes.
- Circle edges (content mask and outer clip) are anti-aliased. PIL drew hard edges. The golden test allows for this: mean error ≤ 3/255, alpha mismatch ≤ 1 % of pixels. Measured: 0.12–0.77 and ≤ 0.31 %.
- Edit params are stored, so re-editing is non-destructive.
- Users can add their own border PNGs (they need transparency).
- Thin versions of both bundled borders, to show more of the image (user request, 2026-10-15). Python had only the two thick rings.
- Square, hexagonal, octagonal and decagonal tokens (gold, normal and thin) and a No border option for each shape, with a shape picker above the borders (user request, 2026-10-18). Python tokens were round only.
- Token filters: the colour/tone map filters on the image only, never the ring (user request, 2026-10-15). Python tokens had no filters.
- The cutter draft (source, framing, name, size) is autosaved and restored after a restart.
- Name and size are not inspector fields: "Save to library" asks for name, size and tags in one dialog (user request, 2026-10-11). Python had name and size fields in the panel and no tags at save time. For an asset opened from the library it offers Update existing / Save as new / Cancel instead of Python's Yes/No/Cancel. Add to sheet uses the last name and size (the sheet can change both).

## Parity checklist (Python `ui/token_generator_panel.py`, `ui/token_compositing.py`)
- [x] Border loading and selection (`_load_borders` :80)
- [x] Background: colour, eyedropper, clear
- [x] Ring inset 0–50 %
- [x] Pan by drag, zoom ×1.1 clamped to 0.1–10, reset
- [x] Name and size
- [x] Drop, browse, paste (image, or a copied file path) (`_paste_image` :410)
- [x] Save PNG; Save to library with Update/New/Cancel (`_save_to_library` :615); Add to sheet
- [x] Compose output matches the Python reference (golden test `crates/core/tests/parity_token_compose.rs`)
