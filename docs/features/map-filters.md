# Map filters

A filter pipeline that makes battle maps easier to read and cheaper to print. It is used by the Map Paginator.

## Model
- A pipeline is an ordered list of `{type, enabled, params}`.
- The enabled steps are applied in order to an RGB image.
- Parameters are in percent unless stated otherwise.
- `W` is the image width. Radii that depend on `W` keep the preview proxy and the full-resolution export looking the same.

## Filters
| key | label | param (min–max, default, step) | algorithm |
|---|---|---|---|
| autocontrast | Auto-contrast | clip 0–20, 1, 1 | PIL `ImageOps.autocontrast(cutoff=clip)`: per-channel histogram stretch that clips `clip`% from each end |
| contrast | Contrast | amount 0–300, 150, 5 (falls back to 100 if missing) | `ImageEnhance.Contrast(a/100)`: blends with the mean grey |
| lightness | Lightness | amount 0–200, 100, 5 | `ImageEnhance.Brightness(a/100)` |
| clarity | Local contrast | amount 0–300, 100, 10 | UnsharpMask(radius=max(5, W/40), percent=a, threshold=0) |
| sharpen | Sharpen | amount 0–500, 150, 10 | UnsharpMask(radius=2, percent=a, threshold=2) |
| sepia | Sepia | — | grayscale, then colourize: black (34,20,8), mid (150,110,60), white (255,240,200) |
| grayscale | Grayscale | — | luminance L = 0.299R + 0.587G + 0.114B |
| adaptive | Adaptive B&W | sensitivity 0–100, 50 | blur = Gaussian(max(2, W/120)); diff = blur − gray; off = round(2 + (100−s)/100×28); pixel is black if diff > off |
| threshold | Ink-saving B&W | cutoff 0–100, 50 | global threshold at round(c/100×255) |
| posterize | Posterize | levels 2–8, 4 | grey lookup table with step 255/(L−1) |
| dither | Dither B&W | — | Floyd–Steinberg to 1-bit (PIL `convert("1")`) |
| edge_boost | Edge boost | strength 0–100, 60 | hp = abs(gray − Gaussian(max(1, W/700))); contrast ×2; cutoff = round(110−s); paint black where hp > cutoff |
| line_art | Line art only | detail 0–100, 55 | high-pass (blur max(1, W/600)) combined with inverted CONTOUR of a Gaussian(0.6); contrast ×1.8; clear a 2 px border; cutoff = round((0.55 − 0.45×d/100)×255) |
| canny | Canny edges (*new*) | low 0–255 50, high 0–255 150, dilate 0–5 0 | port of the unused Python `line_art_extractor.py`: gray → Gaussian 5×5 → Canny → invert → dilate 3×3 × n |

**Default order:**
autocontrast, contrast, lightness, clarity, sharpen, sepia, grayscale, adaptive, threshold, posterize, dither, edge_boost, line_art, canny

## Presets (18, verbatim)
Applying a preset enables exactly the listed filters, in the listed order, with the given parameter overrides. Every other filter is disabled.

| Preset | Steps |
|---|---|
| None (clear all) | — |
| Most legible (gray + edges) | autocontrast; sharpen 150; grayscale; edge_boost 70 |
| Ink-saver (adaptive) | autocontrast; sharpen 150; adaptive 50 |
| Detailed line maps | autocontrast; sharpen 200; adaptive 60 |
| Max detail B&W | autocontrast; clarity 120; sharpen 150; adaptive 65; edge_boost 40 |
| Atmospheric (dither) | autocontrast; dither; edge_boost 50 |
| Newsprint | autocontrast; contrast 140; dither |
| Newsprint (print-friendly) | autocontrast clip 2; contrast 120; sharpen 150; dither; edge_boost 55 |
| High-contrast B&W | autocontrast; contrast 180; threshold 55 |
| Posterized (3 levels) | autocontrast; clarity 100; posterize 3; edge_boost 50 |
| Posterized (4 levels) | autocontrast; posterize 4 |
| Soft grayscale | autocontrast; clarity 80; grayscale |
| Clean grayscale | autocontrast; grayscale |
| Pure line art | sharpen 150; line_art 55 |
| Pencil sketch | clarity 120; line_art 68 |
| Bold ink (thick lines) | autocontrast; sharpen 280; adaptive 40 |
| Sepia print | autocontrast; sepia |
| Faded parchment | autocontrast; clarity 60; sepia |

## UI
- An inspector section, "Filters". It replaces Python's separate popup window and contains:
  - a preset dropdown;
  - a toggle and a slider for each filter;
  - drag handles to reorder filters (Python used ▲/▼ buttons);
  - "Clear all".
- The preview recomputes on the work proxy, debounced 120 ms, in Rust off the UI thread.

## Parity
The filters are ports of PIL's `libImaging` maths (3-pass box Gaussian with 24-bit fixed point, UnsharpMask integer rounding, float32 `blend`, autocontrast/colorize LUTs, Floyd–Steinberg `tobilevel`, CONTOUR 3×3) and of OpenCV for canny. The golden test (`crates/core/tests/parity_map_images.rs`) checks every filter at two parameter values, all 18 presets and canny against the Python/PIL/OpenCV output: **all are bit-exact**. The test allows ≥ 99.9 % identical pixels and MAE ≤ 0.05.

**Checklist**
- [x] All 13 Python filters, plus canny
- [x] Defaults and ranges as listed
- [x] All 18 presets, verbatim
- [ ] Order is user-controllable
- [x] Golden image tests within tolerance (bit-exact)
