//! Screenshots of an apply window, cropped to the page and scaled down for the
//! model. Cross-platform through `xcap`; macOS asks for Screen Recording.
use image::{imageops::FilterType, DynamicImage};
use tauri::WebviewWindow;

use super::vision::png_base64;

/// Wider screenshots are scaled down to this.
const MAX_WIDTH: u32 = 1280;

/// `(x, y, width, height)` in physical pixels.
pub type Rect = (i32, i32, u32, u32);

/// The page area inside a window capture. `outer` and `inner` are the
/// window's frame and content in physical pixels; the image may use another
/// scale, so offsets are scaled to the image.
pub fn content_rect(img_w: u32, img_h: u32, outer: Rect, inner: Rect) -> Rect {
    if outer.2 == 0 || outer.3 == 0 {
        return (0, 0, img_w, img_h);
    }
    let sx = img_w as f64 / outer.2 as f64;
    let sy = img_h as f64 / outer.3 as f64;
    let x = (((inner.0 - outer.0).max(0)) as f64 * sx).round() as u32;
    let y = (((inner.1 - outer.1).max(0)) as f64 * sy).round() as u32;
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let w = ((inner.2 as f64 * sx).round() as u32).clamp(1, img_w - x);
    let h = ((inner.3 as f64 * sy).round() as u32).clamp(1, img_h - y);
    (x as i32, y as i32, w, h)
}

/// Scales to at most [`MAX_WIDTH`] and encodes as base64 PNG.
pub fn encode(img: DynamicImage) -> String {
    let img = if img.width() > MAX_WIDTH {
        let h = (img.height() as f64 * MAX_WIDTH as f64 / img.width() as f64).round() as u32;
        img.resize_exact(MAX_WIDTH, h.max(1), FilterType::Triangle)
    } else {
        img
    };
    png_base64(&img)
}

/// Captures `window`'s page as base64 PNG.
pub fn capture(window: &WebviewWindow) -> Result<String, String> {
    let title = window.title().map_err(|e| e.to_string())?;
    let pid = std::process::id();
    let target = xcap::Window::all()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|w| w.pid().ok() == Some(pid) && w.title().ok().as_deref() == Some(&title))
        .ok_or("cvd couldn't find the apply window to capture it")?;
    let shot = target.capture_image().map_err(|e| {
        format!("Screen capture failed ({e}). On macOS, allow cvd in System Settings → Privacy & Security → Screen Recording.")
    })?;
    let rect = |p: tauri::PhysicalPosition<i32>, s: tauri::PhysicalSize<u32>| {
        (p.x, p.y, s.width, s.height)
    };
    let outer = rect(
        window.outer_position().map_err(|e| e.to_string())?,
        window.outer_size().map_err(|e| e.to_string())?,
    );
    let inner = rect(
        window.inner_position().map_err(|e| e.to_string())?,
        window.inner_size().map_err(|e| e.to_string())?,
    );
    let (x, y, w, h) = content_rect(shot.width(), shot.height(), outer, inner);
    let page = DynamicImage::ImageRgba8(shot).crop_imm(x as u32, y as u32, w, h);
    if is_blank(&page) {
        return Err("The screen capture came back empty. On macOS, allow cvd in System Settings → Privacy & Security → Screen Recording.".into());
    }
    Ok(encode(page))
}

/// Without permission some platforms return an all-black or transparent image.
fn is_blank(img: &DynamicImage) -> bool {
    let rgba = img.to_rgba8();
    let mut px = rgba.pixels().step_by(97);
    let Some(first) = px.next() else { return true };
    px.all(|p| p == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_the_title_bar_at_any_scale() {
        // 1x: 28 px title bar.
        assert_eq!(
            content_rect(1100, 928, (100, 50, 1100, 928), (100, 78, 1100, 900)),
            (0, 28, 1100, 900)
        );
        // Image at 2x the reported frame.
        assert_eq!(
            content_rect(2200, 1856, (100, 50, 1100, 928), (100, 78, 1100, 900)),
            (0, 56, 2200, 1800)
        );
    }

    #[test]
    fn odd_geometry_stays_inside_the_image() {
        let (x, y, w, h) = content_rect(800, 600, (0, 0, 800, 600), (900, 700, 800, 600));
        assert!(x as u32 + w <= 800 && y as u32 + h <= 600 && w >= 1 && h >= 1);
        assert_eq!(
            content_rect(10, 10, (0, 0, 0, 0), (0, 0, 5, 5)),
            (0, 0, 10, 10)
        );
    }

    #[test]
    fn wide_captures_are_scaled_down() {
        use base64::Engine as _;
        let img = DynamicImage::new_rgb8(2560, 1600);
        let png = base64::engine::general_purpose::STANDARD
            .decode(encode(img))
            .unwrap();
        let back = image::load_from_memory(&png).unwrap();
        assert_eq!((back.width(), back.height()), (1280, 800));
    }

    #[test]
    fn uniform_images_are_blank() {
        assert!(is_blank(&DynamicImage::new_rgb8(50, 50)));
        let mut img = image::RgbImage::new(50, 50);
        for (x, y, p) in img.enumerate_pixels_mut() {
            if x < 25 && y < 25 {
                *p = image::Rgb([255, 0, 0]);
            }
        }
        assert!(!is_blank(&DynamicImage::ImageRgb8(img)));
    }
}
