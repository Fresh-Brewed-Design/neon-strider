// Tray icon variants (running, offline, and each with the update dot) derived from icons/tray.png; see
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

    let (w, h) = (info.width, info.height);
    let mut running = rgba.to_vec();
    tray_art::running(&mut running, w, h);
    let mut offline = rgba.to_vec();
    tray_art::offline(&mut offline);
    for (name, px) in [
        ("idle", rgba.to_vec()),
        ("running", running),
        ("offline", offline),
    ] {
        if name != "idle" {
            write_png(&out.join(format!("tray-{name}.png")), &px, w, h);
        }
        let mut dotted = px;
        tray_art::update_dot(&mut dotted, w, h);
        write_png(&out.join(format!("tray-{name}-dot.png")), &dotted, w, h);
    }

    // The macOS menu bar's template variants (tray_art::template).
    for (name, belt, opacity) in [
        ("idle", false, 1.0),
        ("running", true, 1.0),
        ("offline", false, tray_art::TEMPLATE_OFFLINE_ALPHA),
    ] {
        let mut px = rgba.to_vec();
        tray_art::template(&mut px, belt, opacity);
        write_png(&out.join(format!("tray-template-{name}.png")), &px, w, h);
        tray_art::template_dot(&mut px, w, h);
        write_png(
            &out.join(format!("tray-template-{name}-dot.png")),
            &px,
            w,
            h,
        );
    }
}

fn main() {
    tray_variants();
    tauri_build::build()
}
