use egui::{Color32, Frame, Margin, RichText, Rounding, ScrollArea, Stroke, Ui};

use crate::state::AppState;

pub enum MessageViewAction {
    Reply(String),
    Forward(String),
    ToggleFlag(String, bool),
    ToggleRead(String, bool),
    Delete(String),
    OpenHtmlInBrowser(String), // HTML string
    DownloadAttachment(String, String), // (blob_hash, filename)
}

pub fn render_message_view(ui: &mut Ui, state: &mut AppState) -> Option<MessageViewAction> {
    let mut action = None;

    let detail = match &state.selected_message_detail {
        Some(d) => d,
        None => {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Select a message to view its content.").italics());
            });
            return None;
        }
    };

    let summary = match &detail.summary {
        Some(s) => s,
        None => return None,
    };

    let is_seen = summary.flags.as_ref().map_or(false, |f| f.seen);
    let is_flagged = summary.flags.as_ref().map_or(false, |f| f.flagged);

    ui.vertical(|ui| {
        // --- 1. Action Toolbar ---
        ui.horizontal(|ui| {
            if ui.button("↩ Reply").clicked() {
                action = Some(MessageViewAction::Reply(summary.id.clone()));
            }
            if ui.button("↪ Forward").clicked() {
                action = Some(MessageViewAction::Forward(summary.id.clone()));
            }

            let star_text = if is_flagged { "⭐ Starred" } else { "☆ Star" };
            if ui.button(star_text).clicked() {
                action = Some(MessageViewAction::ToggleFlag(summary.id.clone(), !is_flagged));
            }

            let read_text = if is_seen { "✉ Mark Unread" } else { "✉ Mark Read" };
            if ui.button(read_text).clicked() {
                action = Some(MessageViewAction::ToggleRead(summary.id.clone(), !is_seen));
            }

            if ui.button("🗑 Delete").clicked() {
                action = Some(MessageViewAction::Delete(summary.id.clone()));
            }

            if let Some(ref html) = detail.body_html {
                if !html.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🌐 Open HTML in Browser").on_hover_text("Open full formatted HTML email in default web browser").clicked() {
                            action = Some(MessageViewAction::OpenHtmlInBrowser(html.clone()));
                        }
                    });
                }
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // --- 2. Message Header Card ---
        let header_frame = Frame::none()
            .fill(Color32::from_rgb(30, 32, 40))
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(12.0))
            .stroke(Stroke::new(1.0_f32, Color32::from_gray(50)));

        header_frame.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Subject
            let subject = if summary.subject.is_empty() {
                "(No Subject)"
            } else {
                &summary.subject
            };
            ui.heading(RichText::new(subject).strong().color(Color32::WHITE));

            ui.add_space(6.0);

            // From
            if let Some(ref sender) = summary.sender {
                let from_str = match &sender.name {
                    Some(name) if !name.is_empty() => format!("{} <{}>", name, sender.address),
                    _ => sender.address.clone(),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("From:").strong().color(Color32::GRAY));
                    ui.label(RichText::new(from_str).color(Color32::LIGHT_GRAY));
                });
            }

            // To
            if !summary.recipients.is_empty() {
                let to_str = summary
                    .recipients
                    .iter()
                    .map(|r| match &r.name {
                        Some(name) if !name.is_empty() => format!("{} <{}>", name, r.address),
                        _ => r.address.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                ui.horizontal(|ui| {
                    ui.label(RichText::new("To:").strong().color(Color32::GRAY));
                    ui.label(RichText::new(to_str).color(Color32::LIGHT_GRAY));
                });
            }

            // Date
            if let Some(ref date_str) = summary.date {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Date:").strong().color(Color32::GRAY));
                    ui.label(RichText::new(date_str).color(Color32::LIGHT_GRAY));
                });
            }
        });

        // --- 3. Attachments Bar ---
        if !detail.attachments.is_empty() {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Attachments:").strong());
                for att in &detail.attachments {
                    let size_kb = att.size / 1024;
                    let label = format!("📎 {} ({} KB)", att.filename, size_kb);
                    if ui.button(label).on_hover_text("Click to save attachment").clicked() {
                        action = Some(MessageViewAction::DownloadAttachment(
                            att.blob_hash.clone(),
                            att.filename.clone(),
                        ));
                    }
                }
            });
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // --- 4. Message Body ---
        ScrollArea::vertical()
            .id_salt("message_body_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(ref text) = detail.body_text {
                    if !text.is_empty() {
                        ui.label(RichText::new(text).size(13.5).line_height(Some(20.0)));
                        return;
                    }
                }

                if let Some(ref html) = detail.body_html {
                    if !html.is_empty() {
                        // Render plain text fallback
                        let plain = html2text_simple(html);
                        ui.label(RichText::new(plain).size(13.5).line_height(Some(20.0)));
                        return;
                    }
                }

                ui.label(RichText::new("(No message body content)").italics());
            });
    });

    action
}

fn html2text_simple(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
            result.push(' ');
        } else if !in_tag {
            result.push(c);
        }
    }

    result
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}
