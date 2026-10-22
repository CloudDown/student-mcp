use std::{env, fs, io::BufReader, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    for name in ["moodle", "proxmox", "brightspace"] {
        let path = format!("assets/{name}-icon.png");
        println!("cargo:rerun-if-changed={path}");
        let decoder = png::Decoder::new(BufReader::new(fs::File::open(&path).expect("bundled logo")));
        let mut reader = decoder.read_info().expect("bundled PNG header");
        let mut pixels = vec![0; reader.output_buffer_size().expect("logo fits in memory")];
        let info = reader.next_frame(&mut pixels).expect("bundled logo decodes");
        // The bundled assets are RGBA8; fail at build time if that contract changes.
        assert_eq!(info.color_type, png::ColorType::Rgba, "{path}");
        assert_eq!(info.bit_depth, png::BitDepth::Eight, "{path}");
        // Keep the exact pixels and dimensions; only move PNG decoding to build time.
        let mut bytes = Vec::with_capacity(8 + info.buffer_size());
        bytes.extend_from_slice(&info.width.to_le_bytes());
        bytes.extend_from_slice(&info.height.to_le_bytes());
        bytes.extend_from_slice(&pixels[..info.buffer_size()]);
        fs::write(out.join(format!("{name}.rgba")), bytes).expect("write bundled logo");
    }
}
