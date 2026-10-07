# Object Cutter

Cuts an object out of an image with a lasso, optionally adds a sticker-style contour outline, and outputs a cut-out (an RGBA PNG cropped to the object).

## Inputs
- Drop, browse, paste (image or file list), or open from the Library.
- Clicking an empty canvas opens the file browser.

## Canvas interaction
**Selection modes:** Polygon (default) or Freehand. Switching modes resets the selection.
- Polygon: click to add points. Close with a double-click, a right-click or Enter.
- Freehand: drag to draw. Releasing closes the shape if it has at least 3 points.

**Edit selection**
- Undo point (Ctrl+Z). If the shape is closed, this reopens it.
- Reset (Esc).

**View**
- Mouse wheel zooms ×1.1 around the cursor. Buttons −/+ zoom ×1.25. "Fit" fits the image. Zoom is clamped to 0.25–10, and the percentage is shown.
- Pan with the middle mouse button, or with Space + drag (Space + drag is new).

**Lasso drawing:** line `#ff3b3b`, 2 px wide, vertex dots 3 px.

Points are stored in **image coordinates**, not screen coordinates.

## Inspector
| Control | Default | Range |
|---|---|---|
| Outline colour | white (255,255,255) | colour picker |
| Outline thickness | 6 px | 0–40 |
| Name / Size | "", Medium | |

**Actions**
- Export PNG…
- Save to Library (Update / New / Cancel, as in the Token Cutter)
- Add to Object Sheet

## Algorithms (core::cutout)
Python reference: `ui/cutout_compositing.py`.

**`apply_lasso_mask(img, points)`**
1. Fill the polygon into an L mask. This is skipped if there are fewer than 3 points.
2. Multiply the mask into the alpha channel.
3. Crop to the bounding box of the alpha.

**`add_contour_outline(img, px, rgb)`**
1. Pad the image by `px + 2`.
2. Binarize the alpha at a threshold of 16.
3. Dilate by `px`. Python ran `MaxFilter(3)` px times, which grows the shape as a square/Chebyshev distance. The Rust version uses **a distance transform with the Chebyshev (L∞) metric**, so the result is identical in O(n) time for any thickness.
4. Fill the outline colour through the dilated mask.
5. Put the original on top.
6. Crop to the bounding box.

## Performance
- While points are being drawn, the lasso preview is a client-side overlay.
- The mask and outline are computed in Rust on the preview proxy, debounced. Moving the slider no longer blocks the UI.
- Export uses full resolution.

## Persistence
`params_json = {mode, points, outline:{rgb, px}}`. Re-editing restores the trace. **Python only reloaded the original image**, so the user had to trace again.

## MCP
`cutout_create {source: path|asset_id, points:[[x,y],...], outline_px?, outline_rgb?, name?, size?, save_to_library?, add_to_sheet?, out_png?}`

## Parity checklist (Python `ui/object_cutter_panel.py`, `ui/cutout_compositing.py`)
- [ ] Polygon and freehand modes; close via double-click, right-click or Enter; undo; reset
- [ ] Zoom at cursor (`_zoom_at` :416-455), buttons, Fit, clamped 0.25–10
- [ ] Middle-drag pan
- [ ] Outline colour and thickness 0–40 (default 6, white)
- [ ] Lasso mask and crop are pixel-identical to Python (golden test)
- [ ] Outline matches Python's MaxFilter dilation (golden test)
- [ ] Export PNG, Save to Library (with Update/New/Cancel), Add to Object Sheet
