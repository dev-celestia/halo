//! Device-pixel rasters for colored marks.
//!
//! Text, rounded panels, and monochrome icons are already antialiased by the
//! renderer: glyphs use grayscale coverage, panels use a distance field, and
//! Phosphor icons are drawn at twice the device-pixel size of their box.
//! Colored SVGs and small PNGs are the exception. GPUI uploads them once and
//! the sprite atlas samples with a linear filter and no mipmaps, so a 16px
//! SVG or a 128px logo drawn at 16px is rescaled on the GPU and the edges
//! stair-step.
//!
//! These helpers rasterize to the exact device-pixel box the mark is drawn
//! in, so the atlas samples 1:1. SVGs are drawn at twice that size and
//! box-filtered back down, which is the antialiasing for their edges.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use gpui::{Image, ImageFormat};

/// Backing scale of the window currently building its frame.
///
/// `0` means no window has rendered yet; [`scale`] then reports 1.
static SCALE_BITS: AtomicU32 = AtomicU32::new(0);

/// Record the scale of the window whose frame is being built.
pub(crate) fn note_scale(scale: f32) {
    if scale.is_finite() && scale > 0. {
        SCALE_BITS.store(scale.to_bits(), Ordering::Relaxed);
    }
}

fn scale() -> f32 {
    let bits = SCALE_BITS.load(Ordering::Relaxed);
    if bits == 0 {
        1.
    } else {
        f32::from_bits(bits)
    }
}

/// Device pixels for a logical size at the current window scale.
pub(crate) fn device_px(logical: f32) -> u32 {
    let px = (logical * scale()).round();
    if px.is_finite() && px > 0. {
        px as u32
    } else {
        1
    }
}

type RasterCache = HashMap<String, HashMap<u32, Arc<Image>>>;

fn cache() -> &'static Mutex<RasterCache> {
    static CACHE: LazyLock<Mutex<RasterCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));
    &CACHE
}

fn lock() -> MutexGuard<'static, RasterCache> {
    cache().lock().unwrap_or_else(|poison| poison.into_inner())
}

fn lookup(path: &str, device: u32) -> Option<Arc<Image>> {
    lock().get(path)?.get(&device).cloned()
}

fn store(path: &str, device: u32, image: Arc<Image>) -> Arc<Image> {
    lock()
        .entry(path.to_string())
        .or_default()
        .insert(device, image.clone());
    image
}

/// Colored SVG mark, rasterized for the current display and cached.
pub(crate) fn svg_mark(path: &str, logical: f32) -> Option<Arc<Image>> {
    svg_mark_device(path, device_px(logical))
}

fn svg_mark_device(path: &str, device: u32) -> Option<Arc<Image>> {
    if let Some(hit) = lookup(path, device) {
        return Some(hit);
    }
    let bytes = crate::assets::load_bytes(path)?;
    let image = rasterize_svg(bytes.as_ref(), device)?;
    Some(store(path, device, image))
}

/// PNG mark, resampled to a square device-pixel box and cached.
pub(crate) fn cached_png(path: &str, logical: f32) -> Option<Arc<Image>> {
    let device = device_px(logical);
    if let Some(hit) = lookup(path, device) {
        return Some(hit);
    }
    let bytes = crate::assets::load_bytes(path)?;
    let image = png_square(bytes.as_ref(), device)?;
    Some(store(path, device, image))
}

/// Resize a PNG to `device_px` square.
///
/// Alpha is premultiplied before the filter and restored after. Filtering
/// straight RGBA bleeds the fill color into a transparent edge.
pub(crate) fn png_square(bytes: &[u8], device_px: u32) -> Option<Arc<Image>> {
    use image::imageops::FilterType;

    if device_px == 0 {
        return None;
    }

    let src = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    let mut rgba = src.to_rgba8();

    for px in rgba.pixels_mut() {
        let alpha = px.0[3] as u32;
        px.0[0] = ((px.0[0] as u32 * alpha + 127) / 255) as u8;
        px.0[1] = ((px.0[1] as u32 * alpha + 127) / 255) as u8;
        px.0[2] = ((px.0[2] as u32 * alpha + 127) / 255) as u8;
    }

    let mut small = image::imageops::resize(&rgba, device_px, device_px, FilterType::Lanczos3);

    for px in small.pixels_mut() {
        let alpha = px.0[3] as u32;
        if alpha == 0 {
            px.0[0] = 0;
            px.0[1] = 0;
            px.0[2] = 0;
            continue;
        }
        px.0[0] = ((px.0[0] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
        px.0[1] = ((px.0[1] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
        px.0[2] = ((px.0[2] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
    }

    encode_png(small)
}

/// How many coverage samples per device pixel, on each axis.
const SUPERSAMPLE: u32 = 2;

fn rasterize_svg(bytes: &[u8], device: u32) -> Option<Arc<Image>> {
    let options = resvg::usvg::Options {
        shape_rendering: resvg::usvg::ShapeRendering::GeometricPrecision,
        ..resvg::usvg::Options::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let sample = device.saturating_mul(SUPERSAMPLE).max(SUPERSAMPLE);
    let pixmap = render_fit(&tree, sample)?;
    let down = box_downsample(pixmap.data(), pixmap.width(), pixmap.height(), SUPERSAMPLE)?;
    let mut rgba = down.pixels;
    unpremultiply(&mut rgba);
    let image = image::RgbaImage::from_raw(down.width, down.height, rgba)?;
    encode_png(image)
}

fn render_fit(tree: &resvg::usvg::Tree, side: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let size = tree.size();
    let width = size.width();
    let height = size.height();
    let fit = (side as f32) / width.max(height);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(side, side)?;
    let tx = (side as f32 - width * fit) * 0.5;
    let ty = (side as f32 - height * fit) * 0.5;
    let transform = resvg::tiny_skia::Transform::from_scale(fit, fit).post_translate(tx, ty);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

struct Downsample {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Average each `factor`×`factor` block. The source is premultiplied, so the
/// average is coverage antialiasing rather than a darkened fringe.
fn box_downsample(src: &[u8], width: u32, height: u32, factor: u32) -> Option<Downsample> {
    if factor == 0 || !width.is_multiple_of(factor) || !height.is_multiple_of(factor) {
        return None;
    }
    let dst_w = width / factor;
    let dst_h = height / factor;
    if dst_w == 0 || dst_h == 0 {
        return None;
    }
    let area = factor * factor;
    let mut pixels = vec![0u8; (dst_w * dst_h * 4) as usize];
    for y in 0..dst_h {
        for x in 0..dst_w {
            let mut acc = [0u32; 4];
            for oy in 0..factor {
                for ox in 0..factor {
                    let sx = x * factor + ox;
                    let sy = y * factor + oy;
                    let i = ((sy * width + sx) * 4) as usize;
                    for channel in 0..4 {
                        acc[channel] += src[i + channel] as u32;
                    }
                }
            }
            let o = ((y * dst_w + x) * 4) as usize;
            for channel in 0..4 {
                pixels[o + channel] = ((acc[channel] + area / 2) / area) as u8;
            }
        }
    }
    Some(Downsample {
        width: dst_w,
        height: dst_h,
        pixels,
    })
}

fn unpremultiply(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let alpha = px[3] as u32;
        if alpha == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            continue;
        }
        px[0] = ((px[0] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
        px[1] = ((px[1] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
        px[2] = ((px[2] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
    }
}

fn encode_png(image: image::RgbaImage) -> Option<Arc<Image>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        out.into_inner(),
    )))
}
