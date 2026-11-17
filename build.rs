//! Embed the icon drawn in `src/icon.rs` as the executable's Windows icon.
//! The ICO is generated into OUT_DIR from the same pixels used by the tray
//! icon and the settings window, so Windows and the app show one single logo.

#[path = "src/icon.rs"]
mod icon;

use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let ico = out.join("desktop_switcher.ico");
    let rc = out.join("desktop_switcher.rc");

    fs::write(&ico, build_ico()).expect("write ICO");
    let path = ico.to_string_lossy().replace('\\', "\\\\");
    fs::write(&rc, format!("1 ICON \"{path}\"\n")).expect("write RC");

    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_optional()
        .expect("embed Windows icon resource");
}

fn build_ico() -> Vec<u8> {
    let sizes = [16u32, 32, 48, 64, 128, 256];
    let images: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .map(|&s| (s, png(s, &icon::rgba(s))))
        .collect();

    let mut ico = Vec::new();
    u16_le(&mut ico, 0); // reserved
    u16_le(&mut ico, 1); // type: icon
    u16_le(&mut ico, images.len() as u16);

    let mut offset = (6 + 16 * images.len()) as u32;
    for (size, data) in &images {
        let dim = if *size == 256 { 0 } else { *size as u8 };
        ico.push(dim);
        ico.push(dim);
        ico.push(0); // palette colours
        ico.push(0); // reserved
        u16_le(&mut ico, 1); // planes
        u16_le(&mut ico, 32); // bits per pixel
        u32_le(&mut ico, data.len() as u32);
        u32_le(&mut ico, offset);
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend_from_slice(data);
    }
    ico
}

fn png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(rgba, size, size, ExtendedColorType::Rgba8)
        .expect("encode PNG icon");
    buf
}

fn u16_le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn u32_le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
