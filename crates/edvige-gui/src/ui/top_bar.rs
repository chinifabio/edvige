use egui::{Color32, RichText, Ui};

use crate::state::AppState;

pub enum TopBarAction {
    Compose,
    SyncFolder,
    Search(String),
    ClearSearch,
    OpenAccountWizard,
    OpenAbout,
}

pub fn render_top_bar(ui: &mut Ui, state: &mut AppState) -> Option<TopBarAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        // App brand title
        ui.heading(RichText::new("Edvige").strong());
        ui.separator();

        // Compose Button
        if ui
            .button(RichText::new("✏️ Compose").strong())
            .clicked()
        {
            action = Some(TopBarAction::Compose);
        }

        // Sync Folder Button
        if let Some(folder) = state.selected_folder() {
            let sync_text = format!("🔁 Sync '{}'", folder.display_name);
            if ui.button(sync_text).clicked() {
                action = Some(TopBarAction::SyncFolder);
            }
        }

        // Accounts button
        if ui.button("⚙️ Accounts").clicked() {
            action = Some(TopBarAction::OpenAccountWizard);
        }

        // About button
        if ui.button("ℹ About").clicked() {
            action = Some(TopBarAction::OpenAbout);
        }

        ui.separator();

        // Search Input
        ui.label("🔍");
        let search_response = ui.add(
            egui::TextEdit::singleline(&mut state.search_query)
                .hint_text("Search messages (subject, sender, body)...")
                .desired_width(280.0),
        );

        if search_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if !state.search_query.trim().is_empty() {
                action = Some(TopBarAction::Search(state.search_query.clone()));
            }
        }

        if !state.search_query.is_empty() {
            if ui.small_button("✖").clicked() {
                state.search_query.clear();
                state.is_searching = false;
                action = Some(TopBarAction::ClearSearch);
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Status message toast
            if let Some((msg, created_at)) = &state.status_message {
                if created_at.elapsed().as_secs() < 4 {
                    ui.label(RichText::new(msg).italics().color(Color32::LIGHT_GRAY));
                }
            }
        });
    });

    action
}
