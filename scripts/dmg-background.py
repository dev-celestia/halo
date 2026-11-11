#!/usr/bin/env python3
"""Finder background for the Halo drag-and-drop disk image.

The window is a dark page with a quiet grid. One title group sits above two
drop wells, and a dotted arrow between the wells points at Applications.
Finder draws the app icon, the Applications alias, and both labels on top of
the wells, so this image leaves those spots empty.

Geometry is in points. The origin is the top-left of the Finder content area,
and icon positions are centers. scripts/create-dmg.py imports these constants
and passes them to dmgbuild unchanged.

Produces:
  - dmg-background.png    (720x420 @1x)
  - dmg-background@2x.png (1440x840 @2x)
  - dmg-background.tiff   (multi-resolution HiDPI TIFF via tiffutil)

Usage:
  python3 scripts/dmg-background.py [--app-name Halo] [--out-dir crates/ui/assets/dmg]
  python3 scripts/dmg-background.py --preview /tmp/halo-dmg-preview.png
"""

import argparse
import os
import subprocess

# Points. Finder opens the volume at this content size.
WINDOW_W = 720
WINDOW_H = 420
ICON_SIZE = 112
LABEL_PT = 12

# Drop-well padding. The bottom pad is the Finder label, including the
# selected-name capsule. Side and top pads frame the icon. Padding is
# asymmetric, so the well radius is its own value, not the icon's.
WELL_PAD_X = 22
WELL_PAD_TOP = 18
WELL_PAD_BOTTOM = 46
WELL_RADIUS = 28
WELL_W = ICON_SIZE + WELL_PAD_X * 2
WELL_H = WELL_PAD_TOP + ICON_SIZE + WELL_PAD_BOTTOM

# Icon centers. The app leads; Applications trails.
APP_CENTER = (196, 214)
APPS_CENTER = (524, 214)

# Page and type colors from the dark theme: page #07060c, foreground
# #e0def4, muted #908caa. The arrow and the word "Applications" share one
# blue-to-pink gradient, the destination cue.
GROUND = (7, 6, 12)  # #07060c
INK = (224, 222, 244)  # #e0def4
MUTED = (144, 140, 170)  # #908caa
GRAD_BLUE = (126, 168, 255)  # #7ea8ff
GRAD_PINK = (236, 160, 230)  # #eca0e6

# Title group: 8pt between the kicker ink and the headline. The gaps from
# the headline to the wells and from the wells to the footer are larger.
TITLE_GAP = 8
GROUP_GAP = 24
# 48 puts the title inside one grid cell, so a grid line never sits
# between the kicker and the headline.
TOP_MARGIN = 48
BOTTOM_MARGIN = 36

GRID_STEP = 48
KICKER_PT = 11
KICKER_TRACKING = 2.0
HEADLINE_PT = 30
FOOTER_PT = 13


def _pt(value, scale):
    return int(round(value * scale))


def _well_box(center):
    """Point box. The icon sits WELL_PAD_TOP below the top edge."""
    left = center[0] - WELL_W / 2.0
    top = center[1] - ICON_SIZE / 2.0 - WELL_PAD_TOP
    return (left, top, left + WELL_W, top + WELL_H)


def _load_font(pixel_size, weight, point_size):
    """SF Pro when Pillow can read it, otherwise Helvetica Neue."""
    from PIL import ImageFont

    sf = "/System/Library/Fonts/SFNS.ttf"
    if os.path.exists(sf):
        font = ImageFont.truetype(sf, pixel_size)
        try:
            # Width, optical size, grade, weight. Optical size follows the
            # point size, so the @2x plate is the same cut of the face.
            opsz = max(17, min(96, int(round(point_size))))
            font.set_variation_by_axes([100, opsz, 400, weight])
        except Exception:
            pass
        return font

    neue = "/System/Library/Fonts/HelveticaNeue.ttc"
    if os.path.exists(neue):
        index = 10 if weight >= 560 else 0
        return ImageFont.truetype(neue, pixel_size, index=index)

    helvetica = "/System/Library/Fonts/Helvetica.ttc"
    if os.path.exists(helvetica):
        index = 1 if weight >= 560 else 0
        return ImageFont.truetype(helvetica, pixel_size, index=index)

    return ImageFont.load_default()


def _srgb_to_linear(channel):
    c = channel / 255.0
    if c <= 0.04045:
        return c / 12.92
    return ((c + 0.055) / 1.055) ** 2.4


def _linear_to_srgb(channel):
    c = max(0.0, min(1.0, channel))
    if c <= 0.0031308:
        v = 12.92 * c
    else:
        v = 1.055 * (c ** (1.0 / 2.4)) - 0.055
    return int(round(max(0.0, min(1.0, v)) * 255))


def _rgb_to_oklab(rgb):
    r, g, b = (_srgb_to_linear(c) for c in rgb)
    l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
    m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
    s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b
    l_, m_, s_ = l ** (1 / 3), m ** (1 / 3), s ** (1 / 3)
    L = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_
    a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_
    b_ = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_
    return (L, a, b_)


def _oklab_to_rgb(lab):
    L, a, b_ = lab
    l_ = L + 0.3963377774 * a + 0.2158037573 * b_
    m_ = L - 0.1055613458 * a - 0.0638541728 * b_
    s_ = L - 0.0894841775 * a - 1.2914855480 * b_
    l, m, s = l_ ** 3, m_ ** 3, s_ ** 3
    r = +4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
    g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
    b = -0.0041960362 * l - 0.7034186147 * m + 1.7076147010 * s
    return (_linear_to_srgb(r), _linear_to_srgb(g), _linear_to_srgb(b))


def _oklab_lerp(a, b, t):
    la, aa, ba = _rgb_to_oklab(a)
    lb, ab, bb = _rgb_to_oklab(b)
    return _oklab_to_rgb((la + (lb - la) * t, aa + (ab - aa) * t, ba + (bb - ba) * t))


def _contrast(foreground, background):
    """WCAG 2 contrast ratio of two opaque sRGB triples."""
    def lum(rgb):
        r, g, b = (_srgb_to_linear(c) for c in rgb)
        return 0.2126 * r + 0.7152 * g + 0.0722 * b

    lighter = max(lum(foreground), lum(background))
    darker = min(lum(foreground), lum(background))
    return (lighter + 0.05) / (darker + 0.05)


def _rounded_mask(size, box, radius, supersample=4):
    from PIL import Image, ImageDraw

    ss = supersample
    mask = Image.new("L", (size[0] * ss, size[1] * ss), 0)
    draw = ImageDraw.Draw(mask)
    scaled = [int(round(v * ss)) for v in box]
    draw.rounded_rectangle(scaled, radius=int(round(radius * ss)), fill=255)
    return mask.resize(size, Image.Resampling.LANCZOS)


def _grid(img, scale):
    from PIL import Image, ImageDraw

    step = _pt(GRID_STEP, scale)
    overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    color = (255, 255, 255, 22)
    width, height = img.size
    x = 0
    while x < width:
        draw.line([(x, 0), (x, height)], fill=color)
        x += step
    y = 0
    while y < height:
        draw.line([(0, y), (width, y)], fill=color)
        y += step
    return Image.alpha_composite(img, overlay)


def _wells(img, scale):
    """Neutral frames. The stroke is structure; the fill only lifts the target."""
    from PIL import Image, ImageChops

    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    radius = _pt(WELL_RADIUS, scale)
    inset = max(1, scale)
    for center in (APP_CENTER, APPS_CENTER):
        box = tuple(_pt(v, scale) for v in _well_box(center))
        fill_mask = _rounded_mask(img.size, box, radius)
        inner = (
            box[0] + inset,
            box[1] + inset,
            box[2] - inset,
            box[3] - inset,
        )
        inner_mask = _rounded_mask(img.size, inner, max(0, radius - inset))
        ring = ImageChops.subtract(fill_mask, inner_mask)

        fill = Image.new("RGBA", img.size, (255, 255, 255, 14))
        plate = Image.new("RGBA", img.size, (0, 0, 0, 0))
        plate.paste(fill, (0, 0), fill_mask)
        stroke = Image.new("RGBA", img.size, (255, 255, 255, 48))
        plate.paste(stroke, (0, 0), ring)
        layer = Image.alpha_composite(layer, plate)
    return Image.alpha_composite(img, layer)


def _arrow(img, scale):
    """Dots, then a chevron, in the same gradient as the destination word.

    The row is centered on the icon, not the well, so it lines up with the
    glyphs Finder will draw. The chevron is heavier than a dot, so the row
    sits 2pt toward the app.
    """
    from PIL import Image, ImageDraw

    left_well = _well_box(APP_CENTER)
    right_well = _well_box(APPS_CENTER)
    gap_left = left_well[2] + 22
    gap_right = right_well[0] - 22
    span = gap_right - gap_left
    if span < 72:
        raise RuntimeError(f"arrow gap is {span:.1f}pt, need at least 72")

    chevron_w = 9
    chevron_h = 13
    dot_d = 3.5
    # Leave the chevron and a small gap at the trailing end, then fit dots.
    usable = span - chevron_w - 8
    pitch = 9.0
    count = max(6, int(usable // pitch))
    pitch = usable / count
    row_w = count * pitch + 8 + chevron_w
    # 2pt toward the leading icon: the head outweighs the tail.
    origin = gap_left + (span - row_w) / 2.0 - 2
    cy = APP_CENTER[1]

    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    for i in range(count):
        t = (i * pitch) / max(1.0, row_w - chevron_w)
        color = _oklab_lerp(GRAD_BLUE, GRAD_PINK, t) + (255,)
        cx = origin + dot_d / 2.0 + i * pitch
        r = dot_d / 2.0
        box = [
            _pt(cx - r, scale),
            _pt(cy - r, scale),
            _pt(cx + r, scale),
            _pt(cy + r, scale),
        ]
        draw.ellipse(box, fill=color)

    # Chevron at 4× so the tip stays smooth, then colored with the end stop.
    ss = 4
    cw = _pt(chevron_w, scale) * ss
    ch = _pt(chevron_h, scale) * ss
    stroke = max(ss * 2, _pt(2.5, scale) * ss)
    mask = Image.new("L", (cw, ch), 0)
    mask_draw = ImageDraw.Draw(mask)
    pad = stroke
    mask_draw.line(
        [(pad, pad), (cw - pad, ch // 2), (pad, ch - pad)],
        fill=255,
        width=stroke,
        joint="curve",
    )
    mask = mask.resize((max(1, cw // ss), max(1, ch // ss)), Image.Resampling.LANCZOS)
    chevron = Image.new("RGBA", mask.size, GRAD_PINK + (255,))
    chevron.putalpha(mask)
    tip_x = _pt(origin + count * pitch + 8, scale)
    tip_y = _pt(cy, scale) - chevron.size[1] // 2
    layer.paste(chevron, (tip_x, tip_y), chevron)
    return Image.alpha_composite(img, layer)


def _text_width(font, text, tracking=0):
    if tracking <= 0:
        return font.getlength(text)
    width = 0.0
    for ch in text:
        width += font.getlength(ch) + tracking
    return max(0.0, width - tracking)


def _draw_tracked(draw, xy, text, font, fill, tracking):
    x, y = xy
    for ch in text:
        draw.text((x, y), ch, font=font, fill=fill)
        x += font.getlength(ch) + tracking
    return x


def _fit_font(text, point, weight, scale, max_width, tracking=0, floor=11):
    while point >= floor:
        font = _load_font(_pt(point, scale), weight, point)
        if _text_width(font, text, _pt(tracking, scale)) <= max_width:
            return font, point
        point -= 1
    font = _load_font(_pt(floor, scale), weight, floor)
    return font, floor


def _title(img, scale, app_name):
    from PIL import Image, ImageDraw

    draw = ImageDraw.Draw(img)
    max_width = _pt(WINDOW_W - 96, scale)
    kicker = f"INSTALL {app_name.upper()}"
    kicker_font, _ = _fit_font(
        kicker, KICKER_PT, 560, scale, max_width, KICKER_TRACKING, floor=9
    )
    tracking = _pt(KICKER_TRACKING, scale)
    kicker_w = _text_width(kicker_font, kicker, tracking)
    kicker_x = (img.size[0] - kicker_w) / 2.0
    kicker_y = _pt(TOP_MARGIN, scale)
    _draw_tracked(draw, (kicker_x, kicker_y), kicker, kicker_font, MUTED + (255,), tracking)
    # Ink bottom, not the em box, so the gap matches the pixels.
    kicker_box = kicker_font.getbbox(kicker)
    kicker_bottom = kicker_y + kicker_box[3]

    lead = "Drag it into "
    dest = "Applications"
    headline_font, _ = _fit_font(
        lead + dest, HEADLINE_PT, 650, scale, max_width, floor=20
    )
    lead_w = headline_font.getlength(lead)
    lead_box = headline_font.getbbox(lead)
    dest_box = headline_font.getbbox(dest)
    dest_w = dest_box[2] - dest_box[0]
    dest_h = dest_box[3] - dest_box[1]
    line_w = lead_w + headline_font.getlength(dest)
    line_x = (img.size[0] - line_w) / 2.0
    # bbox[1] is the ink top inside the em box. Subtract it so TITLE_GAP is
    # the gap between the kicker ink and the headline ink.
    line_y = kicker_bottom + _pt(TITLE_GAP, scale) - lead_box[1]
    draw.text((line_x, line_y), lead, font=headline_font, fill=INK + (255,))

    mask = Image.new("L", (max(1, int(round(dest_w)) + 2), max(1, dest_h + 2)), 0)
    ImageDraw.Draw(mask).text(
        (-dest_box[0], -dest_box[1]), dest, font=headline_font, fill=255
    )
    gradient = Image.new("RGBA", mask.size)
    row = []
    for x in range(mask.size[0]):
        t = x / max(1, mask.size[0] - 1)
        row.append(_oklab_lerp(GRAD_BLUE, GRAD_PINK, t) + (255,))
    gradient.putdata(row * mask.size[1])
    gradient.putalpha(mask)
    dest_x = int(round(line_x + lead_w + dest_box[0]))
    dest_y = int(round(line_y + dest_box[1]))
    img.paste(gradient, (dest_x, dest_y), gradient)

    line_box = headline_font.getbbox(lead + dest)
    return line_y + line_box[3]


def _footer(img, scale, app_name, well_bottom):
    from PIL import ImageDraw

    text = f"Then open {app_name} from Applications. Setup starts when you launch it."
    max_width = _pt(WINDOW_W - 96, scale)
    font, point = _fit_font(text, FOOTER_PT, 400, scale, max_width, floor=11)
    if _text_width(font, text) > max_width:
        raise RuntimeError("footer does not fit the window at 11pt")
    bbox = font.getbbox(text)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    # Sit on the bottom margin. Never closer to the wells than GROUP_GAP.
    y = _pt(WINDOW_H - BOTTOM_MARGIN, scale) - bbox[3]
    min_y = well_bottom + _pt(GROUP_GAP, scale)
    if y < min_y:
        y = min_y
    x = (img.size[0] - text_w) / 2.0 - bbox[0]
    ImageDraw.Draw(img).text((x, y), text, font=font, fill=MUTED + (255,))
    bottom = y + bbox[3]
    margin = (_pt(WINDOW_H, scale) - bottom) / scale
    if margin < 24:
        raise RuntimeError(f"footer is {margin:.1f}pt from the window edge, need 24")
    return bottom, point, text_h


def _check_layout(headline_bottom, scale):
    well = _well_box(APP_CENTER)
    gap_above = well[1] - (headline_bottom / scale)
    if gap_above < GROUP_GAP:
        raise RuntimeError(
            f"headline is {gap_above:.1f}pt from the wells, need {GROUP_GAP}"
        )
    if well[0] < 48 or WINDOW_W - _well_box(APPS_CENTER)[2] < 48:
        raise RuntimeError("wells are closer than 48pt to the window edge")
    if _well_box(APPS_CENTER)[0] - well[2] < 72:
        raise RuntimeError("wells overlap or leave no room for the arrow")

    # The selected "Applications" capsule has to live inside the well.
    font = _load_font(LABEL_PT, 400, LABEL_PT)
    label_w = font.getlength("Applications") + 16
    if label_w > WELL_W - 16:
        raise RuntimeError(
            f"Applications label is {label_w:.0f}pt, well only leaves {WELL_W - 16:.0f}pt"
        )


def _check_contrast():
    """Opaque pairs on the page. Large headline stops included."""
    pairs = {
        "headline": (INK, 4.5),
        "kicker and footer": (MUTED, 4.5),
        "Applications, blue stop": (GRAD_BLUE, 4.5),
        "Applications, pink stop": (GRAD_PINK, 4.5),
        "Applications, midpoint": (_oklab_lerp(GRAD_BLUE, GRAD_PINK, 0.5), 4.5),
    }
    report = []
    for name, (color, minimum) in pairs.items():
        ratio = _contrast(color, GROUND)
        report.append(f"  {name}: {ratio:.2f}:1 on #07060c")
        if ratio < minimum:
            raise RuntimeError(
                f"{name} is {ratio:.2f}:1 on #07060c, need {minimum:.1f}:1"
            )
    return report


def render_background(scale, app_name):
    from PIL import Image

    size = (_pt(WINDOW_W, scale), _pt(WINDOW_H, scale))
    img = Image.new("RGBA", size, GROUND + (255,))
    img = _grid(img, scale)
    img = _wells(img, scale)
    img = _arrow(img, scale)
    headline_bottom = _title(img, scale, app_name)
    _check_layout(headline_bottom, scale)
    well_bottom = _pt(_well_box(APP_CENTER)[3], scale)
    _footer(img, scale, app_name, well_bottom)
    return img.convert("RGB")


def render_preview(scale, app_name, icon_path):
    """Background plus stand-in icons and labels, for checking the wells."""
    from PIL import Image, ImageDraw

    img = render_background(scale, app_name).convert("RGBA")
    icon_px = _pt(ICON_SIZE, scale)
    if icon_path and os.path.exists(icon_path):
        icon = Image.open(icon_path).convert("RGBA")
        icon = icon.resize((icon_px, icon_px), Image.Resampling.LANCZOS)
        left = _pt(APP_CENTER[0], scale) - icon_px // 2
        top = _pt(APP_CENTER[1], scale) - icon_px // 2
        img.paste(icon, (left, top), icon)

    folder = _system_applications_icon(icon_px)
    if folder is not None:
        left = _pt(APPS_CENTER[0], scale) - icon_px // 2
        top = _pt(APPS_CENTER[1], scale) - icon_px // 2
        img.paste(folder, (left, top), folder)

    draw = ImageDraw.Draw(img)
    font = _load_font(_pt(LABEL_PT, scale), 400, LABEL_PT)
    _paint_label(draw, scale, APP_CENTER, app_name, font, selected=True)
    _paint_label(draw, scale, APPS_CENTER, "Applications", font, selected=False)
    return img.convert("RGB")


def _paint_label(draw, scale, center, text, font, selected):
    """Approximate Finder's icon-view label, 4pt under the icon."""
    bbox = font.getbbox(text)
    text_w = bbox[2] - bbox[0]
    cx = _pt(center[0], scale)
    top = _pt(center[1] + ICON_SIZE / 2.0 + 4, scale)
    x = cx - text_w / 2.0 - bbox[0]
    if selected:
        pad_x = _pt(7, scale)
        pad_y = _pt(2, scale)
        pill = [
            x + bbox[0] - pad_x,
            top + bbox[1] - pad_y,
            x + bbox[2] + pad_x,
            top + bbox[3] + pad_y,
        ]
        draw.rounded_rectangle(pill, radius=_pt(4, scale), fill=(10, 132, 255, 255))
        fill = (255, 255, 255, 255)
    else:
        fill = (224, 222, 244, 255)
    draw.text((x, top), text, font=font, fill=fill)


def _system_applications_icon(pixel_size):
    from PIL import Image

    icns = (
        "/System/Library/CoreServices/CoreTypes.bundle/"
        "Contents/Resources/ApplicationsFolderIcon.icns"
    )
    if not os.path.exists(icns):
        return None
    import tempfile

    fd, path = tempfile.mkstemp(suffix=".png")
    os.close(fd)
    try:
        result = subprocess.run(
            ["sips", "-s", "format", "png", icns, "--out", path],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0 or not os.path.exists(path):
            return None
        icon = Image.open(path).convert("RGBA")
        return icon.resize((pixel_size, pixel_size), Image.Resampling.LANCZOS)
    finally:
        if os.path.exists(path):
            os.remove(path)


def main():
    parser = argparse.ArgumentParser(
        description="Render the macOS drag-and-drop installer background"
    )
    parser.add_argument("--app-name", default="Halo", help="Application name")
    parser.add_argument("--out-dir", default="", help="Output directory")
    parser.add_argument(
        "--preview",
        default="",
        help="Also write a composite with icons and labels at this path",
    )
    args = parser.parse_args()

    for line in _check_contrast():
        print(line)

    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = args.out_dir or os.path.join(root, "crates/ui/assets/dmg")
    os.makedirs(out_dir, exist_ok=True)

    p1x = os.path.join(out_dir, "dmg-background.png")
    p2x = os.path.join(out_dir, "dmg-background@2x.png")
    ptiff = os.path.join(out_dir, "dmg-background.tiff")

    print(f"==> rendering {args.app_name} DMG background...")
    render_background(1, args.app_name).save(p1x, "PNG")
    print(f"  created: {p1x} ({WINDOW_W}x{WINDOW_H})")

    render_background(2, args.app_name).save(p2x, "PNG")
    print(f"  created: {p2x} ({WINDOW_W * 2}x{WINDOW_H * 2} @2x)")

    if os.path.exists("/usr/bin/tiffutil"):
        result = subprocess.run(
            ["/usr/bin/tiffutil", "-cathidpicheck", p1x, p2x, "-out", ptiff],
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            print(f"  created: {ptiff} (HiDPI Retina TIFF)")
        else:
            print(f"  warning: tiffutil failed: {result.stderr.strip()}")

    if args.preview:
        icon = os.path.join(root, "crates/ui/assets/icons/icon.png")
        render_preview(2, args.app_name, icon).save(args.preview, "PNG")
        print(f"  preview: {args.preview}")


if __name__ == "__main__":
    main()
