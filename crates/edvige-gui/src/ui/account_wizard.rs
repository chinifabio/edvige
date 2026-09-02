use edvige_proto::SecurityModeProto;
use egui::{Color32, RichText, Window};

use crate::state::AppState;

pub enum AccountWizardAction {
    SaveAccount,
    Cancel,
}

pub fn render_account_wizard(ctx: &egui::Context, state: &mut AppState) -> Option<AccountWizardAction> {
    if !state.show_account_wizard {
        return None;
    }

    let mut action = None;
    let mut is_open = state.show_account_wizard;

    Window::new(RichText::new("⚙ Account Setup").strong())
        .open(&mut is_open)
        .default_size([480.0, 420.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Configure your Email Account (IMAP & SMTP)").italics());
                ui.add_space(8.0);

                // Account Name & Email
                egui::Grid::new("account_wizard_grid_basic")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Display Name:");
                        ui.text_edit_singleline(&mut state.wizard_name);
                        ui.end_row();

                        ui.label("Email Address:");
                        ui.text_edit_singleline(&mut state.wizard_email);
                        ui.end_row();
                    });

                ui.add_space(6.0);
                ui.separator();
                ui.label(RichText::new("IMAP Settings (Incoming)").strong());

                egui::Grid::new("account_wizard_grid_imap")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("IMAP Host:");
                        ui.text_edit_singleline(&mut state.wizard_imap_host);
                        ui.end_row();

                        ui.label("IMAP Port:");
                        ui.add(egui::DragValue::new(&mut state.wizard_imap_port));
                        ui.end_row();

                        ui.label("Security:");
                        egui::ComboBox::from_id_salt("wizard_imap_sec_combo")
                            .selected_text(format!("{:?}", state.wizard_imap_sec))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut state.wizard_imap_sec, SecurityModeProto::SecurityTls, "TLS (Port 993)");
                                ui.selectable_value(&mut state.wizard_imap_sec, SecurityModeProto::SecurityStarttls, "STARTTLS (Port 143)");
                                ui.selectable_value(&mut state.wizard_imap_sec, SecurityModeProto::SecurityPlain, "Plain (Port 143)");
                            });
                        ui.end_row();
                    });

                ui.add_space(6.0);
                ui.separator();
                ui.label(RichText::new("SMTP Settings (Outgoing)").strong());

                egui::Grid::new("account_wizard_grid_smtp")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("SMTP Host:");
                        ui.text_edit_singleline(&mut state.wizard_smtp_host);
                        ui.end_row();

                        ui.label("SMTP Port:");
                        ui.add(egui::DragValue::new(&mut state.wizard_smtp_port));
                        ui.end_row();

                        ui.label("Security:");
                        egui::ComboBox::from_id_salt("wizard_smtp_sec_combo")
                            .selected_text(format!("{:?}", state.wizard_smtp_sec))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut state.wizard_smtp_sec, SecurityModeProto::SecurityTls, "TLS (Port 465)");
                                ui.selectable_value(&mut state.wizard_smtp_sec, SecurityModeProto::SecurityStarttls, "STARTTLS (Port 587)");
                                ui.selectable_value(&mut state.wizard_smtp_sec, SecurityModeProto::SecurityPlain, "Plain (Port 25/587)");
                            });
                        ui.end_row();
                    });

                ui.add_space(6.0);
                ui.separator();
                ui.label(RichText::new("Credentials").strong());

                egui::Grid::new("account_wizard_grid_creds")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Username:");
                        ui.text_edit_singleline(&mut state.wizard_user);
                        ui.end_row();

                        ui.label("Password / App Password:");
                        ui.add(egui::TextEdit::singleline(&mut state.wizard_pass).password(true));
                        ui.end_row();
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.button(RichText::new("💾 Save Account").strong().color(Color32::WHITE)).clicked() {
                        action = Some(AccountWizardAction::SaveAccount);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cancel").clicked() {
                            action = Some(AccountWizardAction::Cancel);
                        }
                    });
                });
            });
        });

    if !is_open {
        state.show_account_wizard = false;
    }

    action
}
