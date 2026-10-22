//! Whether a Fast Model reads images. Provider metadata when it says;
//! otherwise a probe: two generated digit images the model must name.
use std::collections::BTreeMap;
use std::io::Cursor;

use base64::Engine as _;
use image::{ImageFormat, Rgb, RgbImage};

use crate::engine::{Answer, DecisionEngine, EngineError, Image, Question};
use crate::providers::ModelRef;
use crate::state::AppState;

/// Settings key holding `true`/`false` for a model id.
pub fn cache_key(model_id: &str) -> String {
    format!("vision:{model_id}")
}

/// A probe answer counts only with at least this probability.
const PROBE_CONFIDENCE: f64 = 0.5;
const PLACEHOLDER: &str = "cvd-probe-image";

/// Cached result, then provider metadata, then the probe. Transient engine
/// errors answer `false` without caching, so the next run asks again.
pub async fn supports_images(st: &AppState, model_id: &str, engine: &dyn DecisionEngine) -> bool {
    let key = cache_key(model_id);
    if let Ok(Some(v)) = st.store.get_setting(&key).await {
        return v == "true";
    }
    let known = known_from_metadata(st, model_id).await;
    let result = match known {
        Some(v) => Ok(v),
        None => probe(engine, random_digits()).await,
    };
    match result {
        Ok(v) => {
            let _ = st.store.set_setting(&key, if v { "true" } else { "false" }).await;
            v
        }
        Err(_) => false,
    }
}

async fn known_from_metadata(st: &AppState, model_id: &str) -> Option<bool> {
    let r = ModelRef::parse(model_id);
    st.providers()
        .remote_models(false)
        .await
        .into_iter()
        .filter(|p| p.provider == r.provider)
        .flat_map(|p| p.models)
        .find(|m| m.id == r.id)
        .and_then(|m| m.vision)
}

fn random_digits() -> [u8; 2] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(7, |d| d.subsec_nanos());
    let a = (nanos % 10) as u8;
    // A different second digit, so a model stuck on one answer fails.
    [a, (a + 1 + (nanos / 10 % 9) as u8) % 10]
}

/// `Ok(true)` when the model names both digits confidently. A model that
/// rejects images is `Ok(false)`; other errors pass through.
pub async fn probe(engine: &dyn DecisionEngine, digits: [u8; 2]) -> Result<bool, EngineError> {
    for d in digits {
        let options: Vec<String> = (0..10).map(|n| n.to_string()).collect();
        let questions = BTreeMap::from([(
            "digit".to_string(),
            Question::Choice {
                instructions: format!("Which digit is drawn in the image {PLACEHOLDER}?").into(),
                options,
                criteria: BTreeMap::new(),
            },
        )]);
        let image = Image {
            placeholder: PLACEHOLDER.into(),
            png_base64: digit_png(d),
        };
        let state = format!("An image: {PLACEHOLDER}");
        let answers = match engine.decide_with_images(&state, &questions, &[image]).await {
            Err(EngineError::ImagesUnsupported) => return Ok(false),
            r => r?,
        };
        let ok = match answers.get("digit") {
            Some(Answer::Choice {
                choice,
                probabilities,
            }) => {
                *choice == d.to_string()
                    && probabilities.get(choice).copied().unwrap_or(0.0) >= PROBE_CONFIDENCE
            }
            _ => false,
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Segments a–g lit for each digit, as a 7-bit mask (bit 0 = a).
const SEGMENTS: [u8; 10] = [
    0b0111111, 0b0000110, 0b1011011, 0b1001111, 0b1100110, 0b1101101, 0b1111101, 0b0000111,
    0b1111111, 0b1101111,
];

/// A black seven-segment digit on white, as base64 PNG. No font needed.
pub fn digit_png(d: u8) -> String {
    let (w, h, t) = (120u32, 200u32, 16u32);
    let (x0, y0, len) = (24u32, 20u32, 72u32);
    let mut img = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
    let mut fill = |x: u32, y: u32, fw: u32, fh: u32| {
        for yy in y..(y + fh).min(h) {
            for xx in x..(x + fw).min(w) {
                img.put_pixel(xx, yy, Rgb([0, 0, 0]));
            }
        }
    };
    let mid = y0 + len + t;
    let bottom = mid + len + t;
    // (x, y, width, height) for a..g.
    let segs = [
        (x0 + t, y0, len, t),               // a: top
        (x0 + len + t, y0 + t, t, len),     // b: top right
        (x0 + len + t, mid + t, t, len),    // c: bottom right
        (x0 + t, bottom, len, t),           // d: bottom
        (x0, mid + t, t, len),              // e: bottom left
        (x0, y0 + t, t, len),               // f: top left
        (x0 + t, mid, len, t),              // g: middle
    ];
    let mask = SEGMENTS[(d % 10) as usize];
    for (i, (x, y, sw, sh)) in segs.into_iter().enumerate() {
        if mask & (1 << i) != 0 {
            fill(x, y, sw, sh);
        }
    }
    png_base64(&image::DynamicImage::ImageRgb8(img))
}

pub fn png_base64(img: &image::DynamicImage) -> String {
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("PNG encoding to memory");
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
#[path = "vision_tests.rs"]
mod tests;
