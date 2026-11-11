//! Raster app marks copied from `references/hexbuffer/src/assets/app-icon`.
//!
//! Embedded so [`celestia_ui::components::ImageIcon`] can paint them without
//! the Phosphor asset source, which only serves SVGs.
//!
//! The marks are authored at 234×234 and the shell draws them at 13–64px.
//! GPUI's sprite atlas samples with linear filtering and builds no mipmaps, so
//! minifying 234px into a 20px mark drops ~99% of the texels per output
//! pixel and the edges alias — worst on 1x displays, where the ratio is
//! 234 → 20 device pixels instead of 234 → 40. [`icon_at`] re-rasterizes a
//! mark for an exact device-pixel size with a proper filter so the atlas
//! samples it 1:1.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use gpui::{Image, ImageFormat};

use crate::icon::ICON_PNG;
use crate::nav::Page;

/// An embedded mark: its raw PNG bytes, a stable id for the raster cache, and
/// the full-size image (built once per process).
pub(crate) struct Icon {
    id: &'static str,
    bytes: &'static [u8],
    full: Arc<Image>,
}

fn png(bytes: &'static [u8]) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec()))
}

macro_rules! app_png {
    ($name:ident, $file:literal) => {
        fn $name() -> Icon {
            static FULL: LazyLock<Arc<Image>> = LazyLock::new(|| png(include_bytes!($file)));
            Icon {
                id: stringify!($name),
                bytes: include_bytes!($file),
                full: FULL.clone(),
            }
        }
    };
}

app_png!(http_icon, "../assets/app-icon/http.png");
app_png!(intercept_icon, "../assets/app-icon/intercept.png");
app_png!(repeater_icon, "../assets/app-icon/repeater.png");
app_png!(intruder_icon, "../assets/app-icon/intruder.png");
app_png!(crawler_icon, "../assets/app-icon/crawler.png");
app_png!(coder_icon, "../assets/app-icon/coder.png");
app_png!(source_control_icon, "../assets/app-icon/source-control.png");
app_png!(agent_icon, "../assets/app-icon/agent.png");
app_png!(notes_icon, "../assets/app-icon/notes.png");
app_png!(settings_icon, "../assets/app-icon/settings.png");
app_png!(api_mock_icon, "../assets/app-icon/api-mock.png");
app_png!(api_override_icon, "../assets/app-icon/api-override.png");
app_png!(jwt_icon, "../assets/app-icon/jwt.png");
app_png!(port_scanner_icon, "../assets/app-icon/port-scanner.png");
app_png!(comparer_icon, "../assets/app-icon/comparer.png");
app_png!(file_explorer_icon, "../assets/app-icon/file-explorer.png");
app_png!(r2_storage_icon, "../assets/app-icon/r2-storage.png");
app_png!(wordlists_icon, "../assets/app-icon/wordlists.png");
app_png!(split_view_icon, "../assets/app-icon/split-view.png");
app_png!(doctor_icon, "../assets/app-icon/doctor.png");
app_png!(black_wallpaper, "../assets/black-wallpaper.png");
app_png!(white_wallpaper, "../assets/white-wallpaper.png");

/// The Halo brand mark (512px), used for the Start button and work tab.
fn brand_icon() -> Icon {
    static FULL: LazyLock<Arc<Image>> = LazyLock::new(|| png(ICON_PNG));
    Icon {
        id: "brand",
        bytes: ICON_PNG,
        full: FULL.clone(),
    }
}

/// The mark that represents `page`.
fn page_icon(page: Page) -> Icon {
    match page {
        Page::Desktop => brand_icon(),
        Page::HttpHistory => http_icon(),
        Page::Intercept => intercept_icon(),
        Page::Repeater => repeater_icon(),
        Page::Intruder => intruder_icon(),
        Page::Crawler => crawler_icon(),
        Page::Coder => coder_icon(),
        Page::SourceControl => source_control_icon(),
        Page::Agent => agent_icon(),
        Page::Notes => notes_icon(),
        Page::Settings => settings_icon(),
        Page::ApiMock => api_mock_icon(),
        Page::ApiOverride => api_override_icon(),
        Page::Jwt => jwt_icon(),
        Page::PortScanner => port_scanner_icon(),
        Page::Comparer => comparer_icon(),
        Page::FileExplorer => file_explorer_icon(),
        Page::R2Storage => r2_storage_icon(),
        Page::Wordlists => wordlists_icon(),
        Page::SplitView => split_view_icon(),
        Page::Doctor => doctor_icon(),
        Page::Diagram => source_control_icon(),
    }
}

/// Theme-matched desktop wallpaper from the reference Halo assets.
pub(crate) fn wallpaper(dark: bool) -> Arc<Image> {
    if dark {
        black_wallpaper().full
    } else {
        white_wallpaper().full
    }
}

/// Device-pixel size of a logical size on a display with the given scale.
///
/// A 20px mark on a 1x display is 20 device pixels; the same mark on a 2x
/// display is 40.
pub(crate) fn device_px(logical: f32, scale: f32) -> u32 {
    ((logical * scale).round() as u32).max(1)
}

/// Mark sizes the shell draws, in logical pixels.
const SHELL_SIZES: [f32; 5] = [13., 20., 32., 48., 64.];

/// Pre-rasterize the shell's marks off the UI thread.
///
/// The first paint of the dock and the desktop tiles would otherwise resize
/// 234px marks on the UI thread — a visible hitch in a debug build. The
/// cache is filled either way; this only moves the work off the frame.
pub(crate) fn warm_up(scale: f32) {
    std::thread::spawn(move || {
        for size in SHELL_SIZES {
            let px = device_px(size, scale);
            for page in Page::ALL {
                let _ = icon_at(page, px);
            }
            let _ = brand_at(px);
        }
    });
}

/// Cache key: mark id plus the device-pixel size it was rasterized for.
type RasterKey = (&'static str, u32);

/// Raster cache: (mark id, device pixels) → image.
fn raster_cache() -> &'static Mutex<HashMap<RasterKey, Arc<Image>>> {
    static CACHE: LazyLock<Mutex<HashMap<RasterKey, Arc<Image>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &CACHE
}

/// The mark for `page`, rasterized for an exact `device_px` box.
///
/// Falls back to the full-size image when the mark cannot be decoded, so a
/// broken asset renders as it does today rather than disappearing.
pub(crate) fn icon_at(page: Page, device_px: u32) -> Arc<Image> {
    scaled(page_icon(page), device_px)
}

/// The brand mark, rasterized for an exact `device_px` box.
pub(crate) fn brand_at(device_px: u32) -> Arc<Image> {
    scaled(brand_icon(), device_px)
}

fn scaled(icon: Icon, device_px: u32) -> Arc<Image> {
    if device_px == 0 {
        return icon.full;
    }

    let key = (icon.id, device_px);
    if let Some(hit) = raster_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
    {
        return hit.clone();
    }

    let raster = rasterize(icon.bytes, device_px).unwrap_or_else(|| icon.full.clone());
    raster_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, raster.clone());
    raster
}

/// Resize a PNG to `device_px` square and re-encode it.
///
/// Alpha is premultiplied before the filter runs and un-premultiplied after:
/// the marks have transparent rounded corners, and filtering straight RGBA
/// bleeds the fill colour into the alpha edge.
fn rasterize(bytes: &'static [u8], device_px: u32) -> Option<Arc<Image>> {
    use image::imageops::FilterType;

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

    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(small)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;

    Some(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        out.into_inner(),
    )))
}
