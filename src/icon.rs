//! Application icon, loaded once from `assets/icon.png` and reused for the
//! executable resource, the tray icon and the settings window.

use image::{DynamicImage, imageops::FilterType};
use std::sync::OnceLock;

static IMAGE: OnceLock<DynamicImage> = OnceLock::new();

fn image() -> &'static DynamicImage {
    IMAGE.get_or_init(|| {
        image::load_from_memory(include_bytes!("../assets/icon.png")).expect("decode assets/icon.png")
    })
}

/// RGBA pixels, row by row from the top.
pub fn rgba(size: u32) -> Vec<u8> {
    image()
        .resize_exact(size, size, FilterType::Lanczos3)
        .to_rgba8()
        .into_raw()
}
