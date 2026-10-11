# Token Cutter

Frames any image inside a circular token border and outputs a token image. The output size equals the border's pixel size.

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
| Border | thumbnail strip of the border set | first border | bundled `roll20.png` (442²), `roll20_greyscale.png` (256²), plus user-added borders |
| Background | none (transparent) / colour picker / eyedropper | none | the eyedropper samples the source image |
| Ring | ring thickness (content inset) | 0 % | 0–50 % of the radius |
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
2. **Content mask.** An ellipse inset by `ring_inset_pct/100 * radius`. Multiply it into the base image's alpha.
3. **Background.** If a background colour is set, fill it inside the same mask, underneath the image.
4. **Composite.** Alpha-composite the base, then the border on top.
5. **Outer clip.** Clip to the border's outer circle. The centre comes from the alpha bounding box, and the radius is `min(w,h)/2`.

The transparency checkerboard for preview uses an 8 px square in colours (204,204,204) and white.

## Performance design
- Pan and zoom are shown live as a client-side canvas transform of the preview proxy, with the border overlaid.
- The real compose runs in Rust on the **preview proxy**, debounced (80 ms), and tagged with a generation number so stale results are dropped.
- Export, save and add-to-sheet compose once from the full-resolution original, scaled down to the border size.

## Persistence
- `params_json = {border, bg, ring_inset_pct, pan, zoom}` is stored on the asset, so re-editing restores the exact framing.
- The unsaved editor state is autosaved to the `workspaces` table.

## MCP
- `token_compose {source: path|asset_id, border?, bg?, ring_inset_pct?, pan?, zoom?, name?, size?, save_to_library?, add_to_sheet?, out_png?}`
  - Returns `{asset_id?, png_path?}` and a preview image.
- `token_borders_list`
- `token_borders_add {path}`

## Intentional changes
- UI consistency pass (2026-10-11, branch `ui/consistency`): the source image (with the colour picker) is the inspector's first section instead of a separate left column; Open image, Paste, Save PNG, Add to sheet and Save to library are in the top bar; zoom has −/+ buttons next to Reset view.
- Live preview: the compose runs in Rust off the UI thread, resampling only the visible window of the source (Python resized the whole image on every motion event). At most one render is in flight; the latest drag or zoom state wins, so the UI never queues up or freezes.
- Circle edges (content mask and outer clip) are anti-aliased. PIL drew hard edges. The golden test allows for this: mean error ≤ 3/255, alpha mismatch ≤ 1 % of pixels. Measured: 0.12–0.77 and ≤ 0.31 %.
- Edit params are stored, so re-editing is non-destructive.
- Users can add their own border PNGs (they need transparency).
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
