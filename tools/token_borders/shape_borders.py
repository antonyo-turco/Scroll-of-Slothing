"""Builds the square, hexagonal, octagonal and decagonal Roll20 gold borders from the round ones.

Each output pixel is mapped back onto the ring: its distance from the centre is
measured with the shape's own polygon norm (the same shapes and orientation as
`sleuth_core::token::TokenShape`), so the frame keeps the ring's width and bevel along
every side, and its angle is kept, so the lighting (bright upper left) matches
the round border. 3x3 supersampling per pixel. Run once from the repo root:

    python tools/token_borders/shape_borders.py
"""
import numpy as np
from PIL import Image

DIR = "assets/token_borders"
SS = 3
SQRT2 = 2 ** 0.5


def polygon_norm(sides, normal_deg, circumradius):
    """Norm of a regular polygon in radius units (sleuth_core::token::TokenShape):
    1 on the outline, constant along every side."""
    apothem = circumradius * np.cos(np.pi / sides)
    angles = np.radians(normal_deg + 360.0 * np.arange(sides) / sides)

    def norm(x, y):
        return np.max([x * np.cos(t) + y * np.sin(t) for t in angles], axis=0) / apothem

    return norm


SHAPES = {
    "square": polygon_norm(4, 0, SQRT2),
    "hex": polygon_norm(6, 0, 1.0),  # pointy-top
    "octagon": polygon_norm(8, 0, 1 / np.cos(np.pi / 8)),
    "decagon": polygon_norm(10, 18, 1.0),  # flat top and bottom
}


def bilinear(img, x, y):
    h, w, _ = img.shape
    x = np.clip(x, 0, w - 1.001)
    y = np.clip(y, 0, h - 1.001)
    x0, y0 = np.floor(x).astype(int), np.floor(y).astype(int)
    fx, fy = (x - x0)[..., None], (y - y0)[..., None]
    a, b = img[y0, x0], img[y0, x0 + 1]
    c, d = img[y0 + 1, x0], img[y0 + 1, x0 + 1]
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def reshape(src_name, norm, out_name):
    src = np.asarray(Image.open(f"{DIR}/{src_name}").convert("RGBA")).astype(np.float64)
    h, w, _ = src.shape
    ys, xs = np.nonzero(src[..., 3])
    cx, cy = (xs.min() + xs.max() + 1) / 2, (ys.min() + ys.max() + 1) / 2
    r = min(xs.max() + 1 - xs.min(), ys.max() + 1 - ys.min()) / 2
    # Premultiply so transparent texels don't bleed colour into the edges.
    pre = src.copy()
    pre[..., :3] *= pre[..., 3:4] / 255
    acc = np.zeros((h, w, 4))
    for sy in range(SS):
        for sx in range(SS):
            gy, gx = np.mgrid[0:h, 0:w].astype(np.float64)
            px = (gx + (sx + 0.5) / SS - w / 2) / (w / 2)
            py = (gy + (sy + 0.5) / SS - h / 2) / (h / 2)
            n = norm(px, py)
            ang = np.arctan2(py, px)
            sample = bilinear(pre, cx - 0.5 + n * r * np.cos(ang), cy - 0.5 + n * r * np.sin(ang))
            sample[n > 1] = 0
            acc += sample
    acc /= SS * SS
    a = acc[..., 3:4]
    rgb = np.where(a > 0, acc[..., :3] * 255 / np.maximum(a, 1e-9), 0)
    out = np.concatenate([rgb, a], axis=-1).round().clip(0, 255).astype(np.uint8)
    Image.fromarray(out, "RGBA").save(f"{DIR}/{out_name}", optimize=True)
    print(out_name, out.shape)


for base in ("roll20", "roll20_thin"):
    suffix = "_thin" if base.endswith("_thin") else ""
    for name, norm in SHAPES.items():
        reshape(f"{base}.png", norm, f"roll20_{name}{suffix}.png")
