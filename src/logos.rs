use eframe::egui::{self, ColorImage, Context, TextureHandle};

/// Embedded service app icons, decoded at build time for the native gallery.
pub struct Logos {
    pub moodle: TextureHandle,
    pub omnivox: TextureHandle,
    pub brightspace: TextureHandle,
}

fn texture(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let image = color_image(bytes);
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}

fn color_image(bytes: &[u8]) -> ColorImage {
    let width = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    ColorImage::from_rgba_unmultiplied([width, height], &bytes[8..])
}

impl Logos {
    pub fn load(ctx: &Context) -> Logos {
        Logos {
            moodle: texture(
                ctx,
                "moodle",
                include_bytes!(concat!(env!("OUT_DIR"), "/moodle.rgba")),
            ),
            omnivox: texture(
                ctx,
                "omnivox",
                include_bytes!(concat!(env!("OUT_DIR"), "/omnivox.rgba")),
            ),
            brightspace: texture(
                ctx,
                "brightspace",
                include_bytes!(concat!(env!("OUT_DIR"), "/brightspace.rgba")),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICONS: [(&[u8], &[u8]); 3] = [
        (
            include_bytes!("../assets/moodle-icon.png"),
            include_bytes!(concat!(env!("OUT_DIR"), "/moodle.rgba")),
        ),
        (
            include_bytes!("../assets/omnivox-icon.png"),
            include_bytes!(concat!(env!("OUT_DIR"), "/omnivox.rgba")),
        ),
        (
            include_bytes!("../assets/brightspace-icon.png"),
            include_bytes!(concat!(env!("OUT_DIR"), "/brightspace.rgba")),
        ),
    ];

    fn from_png(bytes: &[u8]) -> ColorImage {
        let decoded = image::load_from_memory(bytes).unwrap().to_rgba8();
        ColorImage::from_rgba_unmultiplied(
            [decoded.width() as usize, decoded.height() as usize],
            decoded.as_raw(),
        )
    }

    #[test]
    fn prepared_icons_preserve_every_pixel() {
        for (png, rgba) in ICONS {
            let original = from_png(png);
            let prepared = color_image(rgba);
            assert_eq!(original.size, prepared.size);
            assert_eq!(original.pixels, prepared.pixels);
        }
    }

    #[test]
    #[ignore = "manual performance measurement"]
    fn benchmark_logo_loading() {
        use std::{hint::black_box, time::Instant};
        for prepared in [false, true] {
            let start = Instant::now();
            for _ in 0..100 {
                for (png, rgba) in ICONS {
                    black_box(if prepared {
                        color_image(rgba)
                    } else {
                        from_png(png)
                    });
                }
            }
            eprintln!("100 logo sets (prepared={prepared}): {:?}", start.elapsed());
        }
    }
}
