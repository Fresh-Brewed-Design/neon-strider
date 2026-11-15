// Tray icon variants (running, offline) derived from icons/tray.png; see
// src/tray_art.rs.
#[path = "src/tray_art.rs"]
mod tray_art;

use std::io::{BufWriter, Cursor};
use std::path::Path;

fn write_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    let file = std::fs::File::create(path).expect("create tray variant");
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(rgba).expect("png data");
}

fn tray_variants() {
    println!("cargo:rerun-if-changed=icons/tray.png");
    println!("cargo:rerun-if-changed=src/tray_art.rs");
    let bytes = std::fs::read("icons/tray.png").expect("read icons/tray.png");
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("tray.png header");
    let mut buf = vec![0; reader.output_buffer_size().expect("tray.png size")];
    let info = reader.next_frame(&mut buf).expect("tray.png data");
    assert!(
        info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight,
        "icons/tray.png must be 8-bit RGBA"
    );
    let rgba = &buf[..info.buffer_size()];
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let out = Path::new(&out);

    let mut running = rgba.to_vec();
    tray_art::running(&mut running, info.width, info.height);
    write_png(
        &out.join("tray-running.png"),
        &running,
        info.width,
        info.height,
    );

    let mut offline = rgba.to_vec();
    tray_art::offline(&mut offline);
    write_png(
        &out.join("tray-offline.png"),
        &offline,
        info.width,
        info.height,
    );
}

fn main() {
    tray_variants();
    tauri_build::build()
}
