use egui::{vec2, Color32, RichText, Window};

use crate::state::AppState;

pub enum ComposerAction {
    Send,
    SaveDraft,
    PickAttachment,
    RemoveAttachment(usize),
    Cancel,
}

pub fn render_composer(ctx: &egui::Context, state: &mut AppState) -> Option<ComposerAction> {
    if !state.show_compose {
        return None;
    }

    let mut action = None;
    let mut is_open = state.show_compose;

    Window::new(RichText::new("✉ New Message").strong())
        .open(&mut is_open)
        .default_size([650.0, 500.0])
        .min_size([400.0, 300.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // To field
                ui.horizontal(|ui| {
                    ui.label(RichText::new("To:").strong().size(13.0));
                    ui.text_edit_singleline(&mut state.composer_to);

                    if !state.composer_show_cc_bcc {
                        if ui.small_button("Cc/Bcc").clicked() {
                            state.composer_show_cc_bcc = true;
                        }
                    }
                });

                // Cc / Bcc fields (optional)
                if state.composer_show_cc_bcc {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Cc:").strong().size(13.0));
                        ui.text_edit_singleline(&mut state.composer_cc);
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Bcc:").strong().size(13.0));
                        ui.text_edit_singleline(&mut state.composer_bcc);
                    });
                }

                // Subject field
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Subject:").strong().size(13.0));
                    ui.text_edit_singleline(&mut state.composer_subject);
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Attachments bar
                ui.horizontal_wrapped(|ui| {
                    if ui.button("📎 Attach File...").clicked() {
                        action = Some(ComposerAction::PickAttachment);
                    }

                    for (idx, (filename, _, bytes)) in state.composer_attachments.iter().enumerate() {
                        let size_kb = bytes.len() / 1024;
                        let label = format!("{} ({} KB) ✖", filename, size_kb);
                        if ui.small_button(label).clicked() {
                            action = Some(ComposerAction::RemoveAttachment(idx));
                        }
                    }
                });

                ui.add_space(4.0);

                // Message Body editor
                let available_height = ui.available_height() - 45.0;
                ui.add(
                    egui::TextEdit::multiline(&mut state.composer_body)
                        .desired_width(ui.available_width())
                        .desired_rows(12)
                        .min_size(vec2(ui.available_width(), available_height.max(150.0))),
                );

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // Actions: Send / Save Draft / Cancel
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("🚀 Send").strong().color(Color32::WHITE))
                        .clicked()
                    {
                        action = Some(ComposerAction::Send);
                    }

                    if ui.button("💾 Save Draft").clicked() {
                        action = Some(ComposerAction::SaveDraft);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cancel").clicked() {
                            action = Some(ComposerAction::Cancel);
                        }
                    });
                });
            });
        });

    if !is_open {
        state.show_compose = false;
    }

    action
}
