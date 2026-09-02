use egui::{Color32, ColorImage, Context, RichText, TextureHandle, TextureOptions, Window};

const LOGO_PNG_BYTES: &[u8] = include_bytes!("../../../../packaging/edvige.png");

pub struct AboutModal {
    texture: Option<TextureHandle>,
}

impl AboutModal {
    pub fn new() -> Self {
        Self { texture: None }
    }

    pub fn render(&mut self, ctx: &Context, is_open: &mut bool) {
        if !*is_open {
            return;
        }

        if self.texture.is_none() {
            if let Ok(img) = image::load_from_memory(LOGO_PNG_BYTES) {
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                let color_image = ColorImage::from_rgba_unmultiplied(
                    [width as usize, height as usize],
                    &rgba.into_raw(),
                );
                self.texture = Some(ctx.load_texture(
                    "edvige_logo",
                    color_image,
                    TextureOptions::LINEAR,
                ));
            }
        }

        let mut open = *is_open;
        let mut close_clicked = false;

        Window::new(RichText::new("About Edvige Mail").strong())
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(ref texture) = self.texture {
                        ui.image((texture.id(), egui::vec2(96.0, 96.0)));
                    } else {
                        ui.heading(RichText::new("✉ Edvige").strong().size(24.0));
                    }

                    ui.heading(RichText::new("Edvige Mail").strong());
                    ui.label(RichText::new("Version 0.1.0").color(Color32::GRAY));
                    ui.label(RichText::new("Decoupled, Local-First Desktop Email Client").italics());

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label("• Background Daemon Architecture (gRPC over Unix Domain Sockets)");
                    ui.label("• Local-First SQLite + FTS5 Full-Text Search");
                    ui.label("• Real-Time IMAP Synchronization & IDLE Push");
                    ui.label("• Content-Addressable Blob Store & Fast MIME Parser");
                    ui.label("• Responsive immediate-mode GUI with egui");

                    ui.add_space(12.0);
                    if ui.button("Close").clicked() {
                        close_clicked = true;
                    }
                });
            });

        if !open || close_clicked {
            *is_open = false;
        }
    }
}

impl Default for AboutModal {
    fn default() -> Self {
        Self::new()
    }
}
