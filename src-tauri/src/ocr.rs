use crate::{error::AppError, media};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::atomic::AtomicBool};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScreenBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl PixelRect {
    pub fn validate(&self, bounds: &ScreenBounds) -> Result<(), AppError> {
        if self.width < 4
            || self.height < 4
            || self
                .x
                .checked_add(self.width)
                .is_none_or(|right| right > bounds.width)
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|bottom| bottom > bounds.height)
        {
            return Err(AppError::new(
                "invalid_input",
                "截图选区为空、过小或超出屏幕，请重新选择。",
            ));
        }
        Ok(())
    }
}
pub fn infer_kind(text: &str) -> &'static str {
    if text.ends_with(['.', '!', '?']) || text.contains('\n') {
        "sentence"
    } else if text.split_whitespace().count() > 1 {
        "phrase"
    } else {
        "word"
    }
}
pub fn recognize(program: &Path, image: &Path, cancelled: &AtomicBool) -> Result<String, AppError> {
    let output = media::run_tool(
        program,
        &[
            image.to_string_lossy().into_owned(),
            "stdout".into(),
            "-l".into(),
            "eng".into(),
            "--psm".into(),
            "6".into(),
        ],
        cancelled,
    )?;
    let text = String::from_utf8(output)
        .map_err(|_| AppError::new("invalid_data", "OCR 没有返回可用文字。"))?
        .trim()
        .to_owned();
    if text.is_empty() {
        return Err(AppError::new(
            "empty_recognition",
            "本次选区没有识别到文字，请重新选择清晰的英语文字。",
        ));
    }
    if text.chars().count() > 4000 {
        return Err(AppError::new("invalid_input", "识别文字过长，请缩小选区。"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_uses_snapshot_relative_pixels_even_with_negative_virtual_origin() {
        let screen = ScreenBounds {
            x: -2560,
            y: -100,
            width: 5120,
            height: 1440,
        };
        PixelRect {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
        }
        .validate(&screen)
        .unwrap();
        PixelRect {
            x: 5000,
            y: 1200,
            width: 120,
            height: 240,
        }
        .validate(&screen)
        .unwrap();
        assert!(
            PixelRect {
                x: 5000,
                y: 0,
                width: 121,
                height: 20
            }
            .validate(&screen)
            .is_err()
        );
    }
    #[test]
    fn empty_and_overflowing_rectangles_are_not_valid_crops() {
        let screen = ScreenBounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        for rect in [
            PixelRect {
                x: 0,
                y: 0,
                width: 0,
                height: 20,
            },
            PixelRect {
                x: u32::MAX,
                y: 0,
                width: 10,
                height: 20,
            },
            PixelRect {
                x: 0,
                y: 1000,
                width: 20,
                height: 100,
            },
        ] {
            assert!(rect.validate(&screen).is_err());
        }
    }
}
