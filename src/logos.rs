use eframe::egui::{self, ColorImage, Context, TextureHandle};

/// Embedded service app icons, decoded once for the native gallery.
pub struct Logos {
    pub moodle: TextureHandle,
    pub proxmox: TextureHandle,
    pub poly: TextureHandle,    // Student record later: Polytechnique symbol
    pub outlook: TextureHandle, // Courriel later: Outlook
}

/// Clonic handles: egui textures are Arc-backed Rc handles, cloning is cheap.
impl Clone for Logos {
    fn clone(&self) -> Self {
        Logos {
            moodle: self.moodle.clone(),
            proxmox: self.proxmox.clone(),
            poly: self.poly.clone(),
            outlook: self.outlook.clone(),
        }
    }
}

fn texture(ctx: &Context, name: &str, bytes: &[u8]) -> TextureHandle {
    let decoded = image::load_from_memory(bytes)
        .expect("bundled logo decodes")
        .to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}

impl Logos {
    pub fn load(ctx: &Context) -> Logos {
        Logos {
            moodle: texture(ctx, "moodle", include_bytes!("../assets/moodle-icon.png")),
            proxmox: texture(ctx, "proxmox", include_bytes!("../assets/proxmox-icon.png")),
            poly: texture(ctx, "poly", include_bytes!("../assets/poly.png")),
            outlook: texture(ctx, "outlook", include_bytes!("../assets/outlook.png")),
        }
    }
}
