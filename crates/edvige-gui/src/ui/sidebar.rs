use edvige_proto::FolderRoleProto;
use egui::{Color32, RichText, ScrollArea, Ui};

use crate::state::AppState;

pub enum SidebarAction {
    SelectAccount(String),
    SelectFolder(String),
    SyncAllFolders,
}

pub fn render_sidebar(ui: &mut Ui, state: &mut AppState) -> Option<SidebarAction> {
    let mut action = None;

    ui.vertical(|ui| {
        ui.add_space(4.0);
        ui.label(RichText::new("ACCOUNTS").strong().size(11.0).color(Color32::GRAY));

        // Account Selector
        if state.accounts.is_empty() {
            ui.label(RichText::new("No accounts configured").italics());
        } else {
            let current_account_name = state
                .selected_account()
                .map(|a| format!("{} ({})", a.name, a.email))
                .unwrap_or_else(|| "Select Account".to_string());

            egui::ComboBox::from_id_salt("sidebar_account_combo")
                .selected_text(current_account_name)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for acc in &state.accounts {
                        let label = format!("{} ({})", acc.name, acc.email);
                        let is_selected = state.selected_account_id.as_deref() == Some(&acc.id);
                        if ui.selectable_label(is_selected, label).clicked() {
                            action = Some(SidebarAction::SelectAccount(acc.id.clone()));
                        }
                    }
                });
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("FOLDERS").strong().size(11.0).color(Color32::GRAY));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("↻").on_hover_text("Refresh folder list").clicked() {
                    action = Some(SidebarAction::SyncAllFolders);
                }
            });
        });

        ui.add_space(4.0);

        // Folders List
        ScrollArea::vertical()
            .id_salt("sidebar_folders_scroll")
            .show(ui, |ui| {
                for folder in &state.folders {
                    let is_selected = state.selected_folder_id.as_deref() == Some(&folder.id);
                    let role = FolderRoleProto::try_from(folder.role).unwrap_or(FolderRoleProto::FolderRoleCustom);
                    let icon = match role {
                        FolderRoleProto::FolderRoleInbox => "📥",
                        FolderRoleProto::FolderRoleSent => "📤",
                        FolderRoleProto::FolderRoleDrafts => "📝",
                        FolderRoleProto::FolderRoleTrash => "🗑️",
                        FolderRoleProto::FolderRoleArchive => "📦",
                        FolderRoleProto::FolderRoleSpam => "🚫",
                        FolderRoleProto::FolderRoleJunk => "⚠️",
                        FolderRoleProto::FolderRoleCustom => "📁",
                    };

                    let folder_text = if folder.unread_count > 0 {
                        format!("{} {} ({})", icon, folder.display_name, folder.unread_count)
                    } else {
                        format!("{} {}", icon, folder.display_name)
                    };

                    let mut text = RichText::new(folder_text);
                    if folder.unread_count > 0 {
                        text = text.strong();
                    }

                    let btn = ui.selectable_label(is_selected, text);
                    if btn.clicked() {
                        action = Some(SidebarAction::SelectFolder(folder.id.clone()));
                    }
                }
            });
    });

    action
}

