use std::time::Instant;
use edvige_proto::{
    AccountProto, FolderProto, MessageDetailProto, MessageSummaryProto, SecurityModeProto,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected(String),
}

pub struct AppState {
    pub connection_status: ConnectionStatus,
    pub accounts: Vec<AccountProto>,
    pub selected_account_id: Option<String>,
    pub folders: Vec<FolderProto>,
    pub selected_folder_id: Option<String>,
    pub messages: Vec<MessageSummaryProto>,
    pub selected_message_id: Option<String>,
    pub selected_message_detail: Option<MessageDetailProto>,
    pub search_query: String,
    pub is_searching: bool,
    pub status_message: Option<(String, Instant)>,

    // Composer modal
    pub show_compose: bool,
    pub composer_to: String,
    pub composer_cc: String,
    pub composer_bcc: String,
    pub composer_show_cc_bcc: bool,
    pub composer_subject: String,
    pub composer_body: String,
    pub composer_is_html: bool,
    pub composer_attachments: Vec<(String, String, Vec<u8>)>,

    // Account wizard modal
    pub show_account_wizard: bool,
    pub wizard_name: String,
    pub wizard_email: String,
    pub wizard_imap_host: String,
    pub wizard_imap_port: u16,
    pub wizard_imap_sec: SecurityModeProto,
    pub wizard_smtp_host: String,
    pub wizard_smtp_port: u16,
    pub wizard_smtp_sec: SecurityModeProto,
    pub wizard_user: String,
    pub wizard_pass: String,

    // About modal
    pub show_about: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            connection_status: ConnectionStatus::Connecting,
            accounts: Vec::new(),
            selected_account_id: None,
            folders: Vec::new(),
            selected_folder_id: None,
            messages: Vec::new(),
            selected_message_id: None,
            selected_message_detail: None,
            search_query: String::new(),
            is_searching: false,
            status_message: None,

            show_compose: false,
            composer_to: String::new(),
            composer_cc: String::new(),
            composer_bcc: String::new(),
            composer_show_cc_bcc: false,
            composer_subject: String::new(),
            composer_body: String::new(),
            composer_is_html: false,
            composer_attachments: Vec::new(),

            show_account_wizard: false,
            wizard_name: String::new(),
            wizard_email: String::new(),
            wizard_imap_host: "imap.gmail.com".into(),
            wizard_imap_port: 993,
            wizard_imap_sec: SecurityModeProto::SecurityTls,
            wizard_smtp_host: "smtp.gmail.com".into(),
            wizard_smtp_port: 465,
            wizard_smtp_sec: SecurityModeProto::SecurityTls,
            wizard_user: String::new(),
            wizard_pass: String::new(),
            show_about: false,
        }
    }
}

impl AppState {
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    pub fn selected_account(&self) -> Option<&AccountProto> {
        self.selected_account_id
            .as_ref()
            .and_then(|id| self.accounts.iter().find(|a| &a.id == id))
    }

    pub fn selected_folder(&self) -> Option<&FolderProto> {
        self.selected_folder_id
            .as_ref()
            .and_then(|id| self.folders.iter().find(|f| &f.id == id))
    }
}
