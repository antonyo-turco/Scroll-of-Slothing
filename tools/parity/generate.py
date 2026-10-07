"""Generate golden parity fixtures from the reference Python app.

Usage (from the repo root, with the *reference* repo's venv):

    <orb>/.venv/Scripts/python.exe -I -B tools/parity/generate.py <path-to-orb-of-many-functions> [out_dir]

Writes JSON files to ``fixtures/parity/`` (default). Rust tests in the crates
load these and assert the new implementation reproduces the Python results.
Regenerate only deliberately and explain diffs in the commit.

The reference repo is only *read*: modules are imported, nothing is written there.
"""

import json
import math
import os
import sys


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    ref = os.path.abspath(sys.argv[1])
    out = os.path.abspath(sys.argv[2] if len(sys.argv) > 2 else "fixtures/parity")
    os.makedirs(out, exist_ok=True)
    sys.path.insert(0, ref)
    os.chdir(ref)  # some modules resolve files relative to themselves; harmless

    gens = [
        ("settings_coerce.json", gen_settings),
        ("token_pack.json", gen_token_pack),
        ("shelf_pack.json", gen_shelf_pack),
        ("cards_parse.json", gen_cards_parse),
        ("cards_layout.json", gen_cards_layout),
        ("grid_modes.json", gen_grid_modes),
        ("homography.json", gen_homography),
        ("tiling.json", gen_tiling),
        ("aoe_geometry.json", gen_aoe),
        ("maxrects.json", gen_maxrects),
        ("token_compose.json", lambda: gen_token_compose(ref, out)),
    ]
    for name, fn in gens:
        data = {"source": "orb-of-many-functions (dev)", "generator": "tools/parity/generate.py",
                "cases": fn()}
        with open(os.path.join(out, name), "w", encoding="utf-8", newline="\n") as f:
            json.dump(data, f, indent=1, ensure_ascii=False, sort_keys=True)
            f.write("\n")
        print(f"wrote {name}: {len(data['cases'])} cases")


class Var:
    """Stand-in for a tk Variable (``.get()``)."""

    def __init__(self, v):
        self.v = v

    def get(self):
        return self.v


SIZES = ["Tiny (0.5 inch)", "Small (0.8 inch)", "Medium (1 inch)", "Large (2 inch)",
         "Huge (3 inch)", "Gargantuan (4 inch)"]


def short_size(label):
    return label.split(" (")[0]


# ── settings ───────────────────────────────────────────────────────────────

def gen_settings():
    import app_settings as s
    inputs = [
        ("cut_line_width", "1.5"), ("cut_line_width", True), ("cut_line_width", "abc"),
        ("cut_line_width", None), ("cut_line_width", 2), ("map_square_in", False),
        ("token_cut_border", 1), ("token_cut_border", True), ("token_cut_border", "true"),
        ("map_margin_in", "0.25"), ("map_overlap_in", [1]),
    ]
    return [{"key": k, "input": v, "output": s._coerce(k, v)} for k, v in inputs]


# ── token sheet packing ────────────────────────────────────────────────────

def gen_token_pack():
    from pdf_generator import PDFGenerator
    scenarios = [
        [("Medium (1 inch)", 1)],
        [("Medium (1 inch)", 90)],
        [("Large (2 inch)", 3), ("Medium (1 inch)", 10), ("Tiny (0.5 inch)", 4)],
        [("Gargantuan (4 inch)", 3), ("Huge (3 inch)", 2), ("Small (0.8 inch)", 7)],
        [("Huge (3 inch)", 5), ("Large (2 inch)", 9), ("Medium (1 inch)", 41)],
        [("Small (0.8 inch)", 2), ("Gargantuan (4 inch)", 1), ("Medium (1 inch)", 1)],
    ]
    cases = []
    for fmt in ("Letter", "A4"):
        for items in scenarios:
            gen = PDFGenerator(page_format=fmt)
            data = [{"path": f"item{i}", "image": None, "size": Var(s), "copies": Var(c)}
                    for i, (s, c) in enumerate(items)]
            pages = gen.generate_preview_pages(data)
            cases.append({
                "format": fmt,
                "items": [{"size": short_size(s), "copies": c} for s, c in items],
                "pages": [[{"item": int(t["path"][4:]), "x": t["x"], "y": t["y"],
                            "w": t["width"], "h": t["height"]} for t in p["tokens"]]
                          for p in pages],
            })
    return cases


# ── object sheet shelf packing ─────────────────────────────────────────────

def gen_shelf_pack():
    from PIL import Image
    from cutout_sheet_generator import CutoutSheetGenerator
    scenarios = [
        [((100, 200), "Medium (1 inch)", 100, 3)],
        [((300, 100), "Large (2 inch)", 150, 4), ((50, 50), "Tiny (0.5 inch)", 100, 12)],
        [((120, 400), "Gargantuan (4 inch)", 200, 5), ((200, 200), "Huge (3 inch)", 100, 3)],
        [((80, 60), "Small (0.8 inch)", 10, 2), ((640, 480), "Gargantuan (4 inch)", 400, 1)],
        [((90, 160), "Huge (3 inch)", 100, 9)],
    ]
    cases = []
    for fmt in ("Letter", "A4"):
        for mode in ("flat", "standee"):
            for items in scenarios:
                gen = CutoutSheetGenerator(page_format=fmt, mode=mode)
                data = []
                for i, ((w, h), size, scale, copies) in enumerate(items):
                    img = Image.new("RGBA", (w, h), (i * 40 % 255, 0, 0, 255))
                    img.info["item"] = i
                    data.append({"image": img, "size": Var(size), "scale": Var(scale),
                                 "copies": Var(copies)})
                pages = gen._layout(data)
                ids = {}
                for i, d in enumerate(data):
                    ids[id(d["image"])] = i
                out_pages = []
                for p in pages:
                    toks = []
                    for t in p["tokens"]:
                        # standee renders are new images; map back by size ratio
                        toks.append({"x": t["x"], "y": t["y"], "w": t["width"], "h": t["height"]})
                    out_pages.append(toks)
                cases.append({
                    "format": fmt, "mode": mode,
                    "items": [{"w_px": w, "h_px": h, "size": short_size(s), "scale": sc,
                               "copies": c} for (w, h), s, sc, c in items],
                    "pages": out_pages,
                })
    return cases


# ── cards ──────────────────────────────────────────────────────────────────

CARD_DECKS = [
    "# Fireball\n**Type:** Spell\n**Level:** 3rd-level\n**Range:** 150 feet\n**Copies:** 2\n\n"
    "A bright streak flashes from your finger to a point you choose,\nthen blossoms into a "
    "**20-foot-radius sphere** of flame.\n",
    "# Lucky\n**Type:** Feat\n\nYou have *inexplicable luck* that seems to kick in at just the right moment.",
    "# Boss Goblin\n**Type:** Boss #8B0000\n**HP:** 42\n**AC:** 15\n\nA cunning goblin chieftain "
    "with __uncanny reflexes__.",
    "# No stats here\nJust body text\n\nSecond paragraph _italic_ and **bold *nested* text**.",
    "**Type:** item\n\nUntitled card body",
    "# Copies junk\n**Type:** Ability\n**Copies:** many\n\nBody",
    "# Zero copies\n**Copies:** 0\n**Type:** weird type words\n\nBody",
    "",
    "# A\n---\n# B\n**Type:** Feat\n\nbody b\n\n-----\n\n# C",
    "# Stray * markers _ here\n**Type:** Spell\n\nUnclosed *star and lone _ underscore, plus a**b.",
    "# Long\n**Type:** Spell\n**Casting Time:** 1 action, which is a long description of the casting "
    "time that should wrap onto multiple lines\n\n" + ("Lorem ipsum dolor sit amet, consectetur "
                                                      "adipiscing elit. " * 30),
]


def card_to_json(c):
    return {"title": c["title"], "type": c["type"], "stats": [list(s) for s in c["stats"]],
            "body": c["body"], "color": list(c["color"]), "copies": c["copies"]}


def gen_cards_parse():
    import card_generator as cg
    cases = [{"markdown": md, "cards": [card_to_json(c) for c in cg.parse_markdown(md)]}
             for md in CARD_DECKS]
    cases.append({"inline": True, "texts": [
        {"text": t, "tokens": [list(x) for x in cg._inline_tokens(t)]}
        for t in ["plain words here", "**bold** and *it* and __u__ and _i_",
                  "a**b", "*unclosed", "x_y_z", "**multi word bold** tail"]]})
    return cases


def gen_cards_layout():
    """Text-measurement-sensitive layout facts: body start y, wrapped line
    counts per font size, and the overflow set."""
    from io import BytesIO

    import card_generator as cg
    from reportlab.pdfgen import canvas as rl_canvas
    c = rl_canvas.Canvas(BytesIO())
    line_w = cg.CARD_W - 12
    cases = []
    for md in CARD_DECKS:
        cards = cg.parse_markdown(md)
        per_card = []
        for card in cards:
            per_card.append({
                "title": card["title"],
                "sim_body_start_y": cg._sim_body_start_y(c, card),
                "body_heights": {str(fs): cg._body_height(c, card["body"], fs, line_w)
                                 for fs in cg.BODY_FONT_SIZES} if card["body"] else {},
                "lines_at": {str(fs): [[w for (w, *_rest) in ln] for ln in
                                       cg._token_lines(c, cg._inline_tokens(p), fs, line_w)]
                             for fs in (13.0, 9.0) for p in card["body"].split("\n")[:1] if p},
            })
        cases.append({"markdown": md, "overflow": cg.find_overflow_cards(cards), "cards": per_card})
    for fmt in ("A4", "Letter"):
        cases.append({"page_layout": fmt, **cg.get_page_layout(fmt)})
    return cases


# ── maps ───────────────────────────────────────────────────────────────────

def gen_grid_modes():
    import map_grid as g
    cases = []
    for size, rows, cols in [((1000, 700), 10, 14), ((1234, 777), 35, 55), ((50, 50), 0, -3)]:
        cases.append({"mode": "rows_cols", "size": size, "rows": rows, "cols": cols,
                      "out": g.grid_from_rows_cols(size, rows, cols)})
    for rect, n in [((10, 20, 220, 230), 3), ((300, 400, 100, 150), 2), ((5.5, 7.25, 75.5, 77.25), 1),
                    ((-30, -10, 60, 80), 3)]:
        cases.append({"mode": "reference", "rect": rect, "n": n,
                      "out": g.grid_from_reference(rect, n)})
    for ppi, sq in [(70, 1.0), (150, 0.5), (-5, 1.0)]:
        cases.append({"mode": "ppi", "ppi": ppi, "square_in": sq, "out": g.grid_from_ppi(ppi, sq)})
    for side in [70, 12.5, -1]:
        cases.append({"mode": "cell_side", "side": side, "out": g.grid_from_cell_side(side)})
    for off, step, ext in [(0, 70, 700), (35, 70, 700), (-15, 50, 510), (123.4, 33.3, 400),
                           (0, 0.4, 10), (0, 0, 10), (7, 10, 6.9995)]:
        cases.append({"mode": "lines", "offset": off, "step": step, "extent": ext,
                      "out": g._line_positions(off, step, ext)})
    for style in ("solid", "dashed", "dotted", "broken"):
        for cell, th in [(70, 1), (12, 3), (200, 10)]:
            cases.append({"mode": "style", "style": style, "cell": cell, "thickness": th,
                          "out": g._style_pattern(style, cell, th)})
    return cases


def gen_homography():
    import map_grid as g
    cases = []
    quads = [
        [(100, 100), (300, 110), (310, 300), (90, 290)],
        [(310, 300), (100, 100), (90, 290), (300, 110)],  # shuffled
        [(0, 0), (10, 0), (10, 10), (0, 10)],
        [(50, 60), (250, 40), (270, 260), (30, 240)],
    ]
    for q in quads:
        ordered = g.order_quad_corners(q)
        side = 200.0
        square = [(0, 0), (side, 0), (side, side), (0, side)]
        cases.append({
            "quad": q,
            "ordered": ordered,
            "perspective_coeffs": g.perspective_coeffs(square, ordered),
            "homography": g.homography_matrix(ordered, square),
            "apply": [list(g.apply_homography(g.homography_matrix(ordered, square), x, y))
                      for x, y in [(0, 0), (500, 0), (123, 456)]],
        })
    try:
        g.homography_matrix([(0, 0), (1, 1), (2, 2), (3, 3)], [(0, 0), (1, 0), (1, 1), (0, 1)])
        degenerate = "ok"
    except ValueError as e:
        degenerate = str(e)
    cases.append({"degenerate": degenerate})
    return cases


def gen_tiling():
    from map_pdf_generator import MapPDFGenerator
    cases = []
    params = [
        dict(image_size=(2000, 1400), region=None, cell=(70, 70), sq=1.0, off=(0, 0), ov=0.0, align=False, goff=(0, 0)),
        dict(image_size=(2000, 1400), region=None, cell=(70, 70), sq=1.0, off=(0, 0), ov=0.5, align=False, goff=(0, 0)),
        dict(image_size=(2000, 1400), region=(140, 70, 980, 700), cell=(70, 70), sq=1.0, off=(35, -20), ov=0.0, align=True, goff=(10, 5)),
        dict(image_size=(3000, 2000), region=None, cell=(100, 80), sq=1.25, off=(0, 0), ov=1.0, align=True, goff=(0, 0)),
        dict(image_size=(800, 600), region=(0, 0, 800, 600), cell=(40, 40), sq=0.5, off=(-100, -100), ov=0.25, align=False, goff=(0, 0)),
        dict(image_size=(500, 500), region=(400, 400, 500, 500), cell=(50, 50), sq=1.0, off=(0, 0), ov=0.0, align=False, goff=(0, 0)),
        dict(image_size=(500, 500), region=None, cell=(0, 50), sq=1.0, off=(0, 0), ov=0.0, align=False, goff=(0, 0)),
    ]
    for fmt in ("Letter", "A4"):
        for margin in (0.0, 18.0):
            for p in params:
                gen = MapPDFGenerator(page_format=fmt)
                gen.margin = margin
                sx, sy, tiles = gen.plan(p["image_size"], p["region"], p["cell"][0], p["cell"][1],
                                         p["sq"], fmt, p["off"], p["ov"], p["align"], p["goff"])
                cases.append({"format": fmt, "margin_pt": margin, **{k: v for k, v in p.items()},
                              "scale": [sx, sy], "tiles": tiles})
    return cases


# ── AoE ────────────────────────────────────────────────────────────────────

def gen_aoe():
    import aoe_templates as a
    cases = []
    for shape in ("circle", "cone", "line", "square"):
        for a_ft, b_ft in [(20, 5), (15, 5), (30, 5), (0, 0), (100, 10), (7.5, 2.5)]:
            cases.append({
                "shape": shape, "a_ft": a_ft, "b_ft": b_ft,
                "bbox_ft": a.bbox_ft(shape, a_ft, b_ft),
                "points": a._shape_points(shape, a_ft, b_ft, 30.0, 8, 8),
                "label": a.default_label(shape, a_ft, b_ft),
            })
    cases.append({"presets": [list(p) for p in a.PRESETS]})
    return cases


def gen_maxrects():
    """Replays compose_sheets' packing loop using the real MaxRects helpers, but
    records positions instead of pasting images."""
    import aoe_templates as a
    cases = []
    scenarios = [
        [(1, 1), (2, 2), (3, 1), (1, 3), (4, 4)],
        [(6, 6), (6, 6), (2, 1)],
        [(12, 2), (1, 1), (1, 1), (1, 1), (5, 5), (3, 3)],
        [(30, 30), (1, 1)],
    ]
    for fmt in ("A4", "Letter"):
        for sizes_in in scenarios:
            dpi, gap_in, margin_in = 150, 0.2, 0.2
            pw_in, ph_in = a.PAGE_INCHES[fmt]
            page_w, page_h = int(round(pw_in * dpi)), int(round(ph_in * dpi))
            margin = int(round(margin_in * dpi))
            gap = int(round(gap_in * dpi))
            usable_w, usable_h = page_w - 2 * margin, page_h - 2 * margin
            boxes, skipped = [], []
            for i, (wi, hi) in enumerate(sizes_in):
                w, h = int(round(wi * dpi)), int(round(hi * dpi))
                if w > usable_w or h > usable_h:
                    skipped.append(i)
                    continue
                boxes.append({"i": i, "w": w, "h": h, "pad_w": min(w + gap, usable_w),
                              "pad_h": min(h + gap, usable_h)})
            boxes.sort(key=lambda b: b["w"] * b["h"], reverse=True)
            pages, placed = [], []
            for b in boxes:
                placement = None
                for pi, free in enumerate(pages):
                    res = a._maxrects_place(free, b["pad_w"], b["pad_h"])
                    if res:
                        placement = (pi, res)
                        break
                if placement is None:
                    pages.append([(0, 0, usable_w, usable_h)])
                    placement = (len(pages) - 1, a._maxrects_place(pages[-1], b["pad_w"], b["pad_h"]))
                pi, (pos, new_free) = placement
                pages[pi] = new_free
                placed.append({"item": b["i"], "page": pi, "x": margin + pos[0], "y": margin + pos[1],
                               "w": b["w"], "h": b["h"]})
            cases.append({"format": fmt, "dpi": dpi, "gap_in": gap_in, "margin_in": margin_in,
                          "items_in": sizes_in, "page_px": [page_w, page_h], "placed": placed,
                          "skipped": skipped, "pages": len(pages)})
    return cases


# ── token compositing (images) ─────────────────────────────────────────────

def gen_token_compose(ref, out):
    """Renders tokens with the real Python compositor; writes PNGs next to the JSON."""
    from PIL import Image

    from ui.token_compositing import compose_token
    img_dir = os.path.join(out, "token")
    os.makedirs(img_dir, exist_ok=True)
    # Deterministic, detailed source: gradients + stripes, 640x400, opaque.
    w, h = 640, 400
    src = Image.new("RGBA", (w, h))
    px = src.load()
    for y in range(h):
        for x in range(w):
            px[x, y] = (x * 255 // w, y * 255 // h, ((x // 20 + y // 20) % 2) * 200 + 30, 255)
    src.save(os.path.join(img_dir, "source.png"))
    cases = []
    params = [
        dict(pan_x=0, pan_y=0, zoom=1.0, ring_inset_pct=0, bg_color=None),
        dict(pan_x=60, pan_y=-25, zoom=1.0, ring_inset_pct=0, bg_color=None),
        dict(pan_x=0, pan_y=0, zoom=2.5, ring_inset_pct=10, bg_color=None),
        dict(pan_x=-30, pan_y=40, zoom=0.6, ring_inset_pct=25, bg_color=(30, 90, 160)),
    ]
    for border in ("roll20.png", "roll20_greyscale.png"):
        b = Image.open(os.path.join(ref, "images", "token_borders", border)).convert("RGBA")
        for i, p in enumerate(params):
            name = f"{border[:-4]}_{i}.png"
            compose_token(src, b, **p).save(os.path.join(img_dir, name))
            cases.append({"border": border, "output": f"token/{name}", **{
                k: (list(v) if isinstance(v, tuple) else v) for k, v in p.items()}})
    return cases


if __name__ == "__main__":
    main()
