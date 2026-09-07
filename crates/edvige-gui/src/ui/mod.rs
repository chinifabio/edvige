pub mod about;
pub mod account_wizard;
pub mod composer;
pub mod message_list;
pub mod message_view;
pub mod sidebar;
pub mod top_bar;

pub use about::AboutModal;
pub use account_wizard::{render_account_wizard, AccountWizardAction};
pub use composer::{render_composer, ComposerAction};
pub use message_list::{render_message_list, MessageListAction};
pub use message_view::{render_message_view, MessageViewAction};
pub use sidebar::{render_sidebar, SidebarAction};
pub use top_bar::{render_top_bar, TopBarAction};

use egui::{Color32, Stroke};

pub fn card_stroke(selected: bool) -> Stroke {
    if selected {
        Stroke::new(1.5_f32, Color32::from_rgb(70, 130, 240))
    } else {
        Stroke::new(1.0_f32, Color32::from_gray(50))
    }
}
