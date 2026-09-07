use chrono::{DateTime, Utc};
use edvige_core::MessageId;
use egui::{Color32, Frame, Margin, RichText, Rounding, ScrollArea, Sense, Ui};

use crate::state::AppState;
use crate::ui::card_stroke;

pub enum MessageListAction {
    SelectMessage(MessageId),
    ToggleFlag(MessageId, bool), // (message_id, new_flag_state)
    ToggleRead(MessageId, bool), // (message_id, new_seen_state)
    DeleteMessage(MessageId),
}

pub fn render_message_list(ui: &mut Ui, state: &mut AppState) -> Option<MessageListAction> {
    let mut action = None;

    if state.messages.is_empty() {
        ui.centered_and_justified(|ui| {
            if state.is_searching {
                ui.label(RichText::new("No messages matching your search.").italics());
            } else {
                ui.label(RichText::new("No messages in this folder.").italics());
            }
        });
        return None;
    }

    ScrollArea::vertical()
        .id_salt("message_list_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for msg in &state.messages {
                let is_selected = state.selected_message_id == Some(msg.id);
                let is_seen = msg.flags.seen;
                let is_flagged = msg.flags.flagged;

                let bg_color = if is_selected {
                    Color32::from_rgb(45, 55, 75)
                } else if !is_seen {
                    Color32::from_rgb(32, 36, 46)
                } else {
                    Color32::from_rgb(26, 28, 34)
                };

                let stroke = card_stroke(is_selected);

                let frame = Frame::none()
                    .fill(bg_color)
                    .stroke(stroke)
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::symmetric(10.0, 8.0));

                let frame_resp = frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    // Top line: Unread dot + Sender + Attachment + Date + Star
                    let star_icon = if is_flagged { "⭐" } else { "☆" };
                    let date_display = msg.date.map(|dt| format_date(&dt)).unwrap_or_default();

                    let sender_name = msg
                        .sender
                        .as_ref()
                        .and_then(|s| s.name.as_deref())
                        .filter(|n| !n.is_empty())
                        .or_else(|| msg.sender.as_ref().map(|s| s.address.as_str()))
                        .unwrap_or("Unknown");

                    let mut sender_text = RichText::new(sender_name);
                    if !is_seen {
                        sender_text = sender_text.strong();
                    }

                    // Reserve width for date and star on the right
                    let right_reserve = 85.0 + if msg.has_attachments { 18.0 } else { 0.0 };
                    let dot_reserve = if !is_seen { 14.0 } else { 0.0 };
                    let sender_max_w = (ui.available_width() - right_reserve - dot_reserve).max(40.0);

                    ui.horizontal(|ui| {
                        if !is_seen {
                            ui.label(RichText::new("●").color(Color32::from_rgb(70, 130, 240)).size(10.0));
                        }

                        ui.add_sized([sender_max_w, 18.0], egui::Label::new(sender_text).truncate());

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(star_icon).clicked() {
                                action = Some(MessageListAction::ToggleFlag(msg.id, !is_flagged));
                            }

                            if !date_display.is_empty() {
                                ui.label(RichText::new(date_display).size(11.0).color(Color32::GRAY));
                            }

                            if msg.has_attachments {
                                ui.label("📎");
                            }
                        });
                    });

                    ui.add_space(2.0);

                    // Subject line
                    let subject = if msg.subject.is_empty() {
                        "(No Subject)"
                    } else {
                        &msg.subject
                    };

                    let mut subject_text = RichText::new(subject);
                    if !is_seen {
                        subject_text = subject_text.strong().color(Color32::WHITE);
                    } else {
                        subject_text = subject_text.color(Color32::LIGHT_GRAY);
                    }
                    ui.add(egui::Label::new(subject_text).truncate());

                    // Snippet preview
                    if !msg.snippet.is_empty() {
                        ui.add_space(1.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&msg.snippet)
                                    .size(11.0)
                                    .color(Color32::from_gray(160))
                                    .italics(),
                            )
                            .truncate(),
                        );
                    }
                });

                // Clicking anywhere on the card selects the message
                if frame_resp.response.interact(Sense::click()).clicked() {
                    action = Some(MessageListAction::SelectMessage(msg.id));
                }

                ui.add_space(4.0);
            }
        });

    action
}

fn format_date(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);

    if diff.num_hours() < 24 && now.date_naive() == dt.date_naive() {
        dt.format("%H:%M").to_string()
    } else if diff.num_days() < 7 {
        dt.format("%a %H:%M").to_string()
    } else {
        dt.format("%b %d, %Y").to_string()
    }
}
