pub mod account_wizard;
pub mod composer;
pub mod message_list;
pub mod message_view;
pub mod sidebar;
pub mod top_bar;

use egui::{Color32, Stroke};

pub fn card_stroke(selected: bool) -> Stroke {
    if selected {
        Stroke::new(1.5_f32, Color32::from_rgb(70, 130, 240))
    } else {
        Stroke::new(1.0_f32, Color32::from_gray(50))
    }
}
