use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use edvige_proto::{
    AccountCredentialsProto, AccountProto, CreateAccountRequest, DaemonEventProto,
    DraftAttachmentProto, FolderProto, MessageDetailProto, MessageFlagsProto,
    MessageSummaryProto, OutboxMessageProto, OutboxStatusProto, ServerConfigProto,
};
use eframe::App;
use egui::{CentralPanel, Context, SidePanel, TopBottomPanel};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::client::DaemonClient;
use crate::state::{AppState, ConnectionStatus};
use crate::ui::account_wizard::{render_account_wizard, AccountWizardAction};
use crate::ui::composer::{render_composer, ComposerAction};
use crate::ui::message_list::{render_message_list, MessageListAction};
use crate::ui::message_view::{render_message_view, MessageViewAction};
use crate::ui::sidebar::{render_sidebar, SidebarAction};
use crate::ui::top_bar::{render_top_bar, TopBarAction};

pub enum AppResponse {
    Connected(DaemonClient),
    ConnectionFailed(String),
    AccountsLoaded(Vec<AccountProto>),
    FoldersLoaded(Vec<FolderProto>),
    MessagesLoaded(Vec<MessageSummaryProto>),
    MessageDetailLoaded(Option<MessageDetailProto>),
    SearchResultsLoaded(Vec<MessageSummaryProto>),
    AccountCreated(AccountProto),
    FlagsUpdated(String, MessageFlagsProto),
    MessageDeleted(String),
    DraftSaved(OutboxMessageProto),
    StatusMessage(String),
}

pub struct EdvigeApp {
    state: AppState,
    client: Option<DaemonClient>,
    runtime: Arc<Runtime>,
    event_rx: mpsc::UnboundedReceiver<DaemonEventProto>,
    event_tx: mpsc::UnboundedSender<DaemonEventProto>,
    response_rx: mpsc::UnboundedReceiver<AppResponse>,
    response_tx: mpsc::UnboundedSender<AppResponse>,
    socket_path: PathBuf,
    last_reconnect_attempt: Option<Instant>,
    about_modal: crate::ui::AboutModal,
}

impl EdvigeApp {
    pub fn new(runtime: Arc<Runtime>, socket_path: PathBuf) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        let mut app = Self {
            state: AppState::default(),
            client: None,
            runtime,
            event_rx,
            event_tx,
            response_rx,
            response_tx,
            socket_path,
            last_reconnect_attempt: None,
            about_modal: crate::ui::AboutModal::new(),
        };

        app.connect_to_daemon();
        app
    }

    pub fn connect_to_daemon(&mut self) {
        self.state.connection_status = ConnectionStatus::Connecting;
        self.last_reconnect_attempt = Some(Instant::now());

        let socket_path = self.socket_path.clone();
        let event_tx = self.event_tx.clone();
        let response_tx = self.response_tx.clone();

        self.runtime.spawn(async move {
            match DaemonClient::connect_uds(&socket_path).await {
                Ok(client) => {
                    client.start_event_listener(None, event_tx);
                    let _ = response_tx.send(AppResponse::Connected(client));
                }
                Err(e) => {
                    let err_msg = format!("Cannot connect to socket {}: {}", socket_path.display(), e);
                    tracing::warn!("{}", err_msg);
                    let _ = response_tx.send(AppResponse::ConnectionFailed(err_msg));
                }
            }
        });
    }

    pub fn refresh_accounts(&self) {
        if let Some(client) = &self.client {
            let client = client.clone();
            let response_tx = self.response_tx.clone();
            self.runtime.spawn(async move {
                match client.list_accounts().await {
                    Ok(accounts) => {
                        let _ = response_tx.send(AppResponse::AccountsLoaded(accounts));
                    }
                    Err(e) => {
                        let _ = response_tx.send(AppResponse::StatusMessage(format!("Failed to load accounts: {}", e)));
                    }
                }
            });
        }
    }

    pub fn select_account(&mut self, account_id: &str) {
        self.state.selected_account_id = Some(account_id.to_string());
        self.refresh_folders();
    }

    pub fn refresh_folders(&self) {
        let account_id = match &self.state.selected_account_id {
            Some(id) => id.clone(),
            None => return,
        };

        if let Some(client) = &self.client {
            let client = client.clone();
            let response_tx = self.response_tx.clone();
            self.runtime.spawn(async move {
                match client.list_folders(&account_id).await {
                    Ok(folders) => {
                        let _ = response_tx.send(AppResponse::FoldersLoaded(folders));
                    }
                    Err(e) => {
                        let _ = response_tx.send(AppResponse::StatusMessage(format!("Failed to load folders: {}", e)));
                    }
                }
            });
        }
    }

    pub fn select_folder(&mut self, folder_id: &str) {
        self.state.selected_folder_id = Some(folder_id.to_string());
        self.state.is_searching = false;
        self.refresh_messages();
    }

    pub fn refresh_messages(&self) {
        let folder_id = match &self.state.selected_folder_id {
            Some(id) => id.clone(),
            None => return,
        };

        if let Some(client) = &self.client {
            let client = client.clone();
            let response_tx = self.response_tx.clone();
            self.runtime.spawn(async move {
                match client.list_messages(&folder_id, 100, 0).await {
                    Ok(messages) => {
                        let _ = response_tx.send(AppResponse::MessagesLoaded(messages));
                    }
                    Err(e) => {
                        let _ = response_tx.send(AppResponse::StatusMessage(format!("Failed to load messages: {}", e)));
                    }
                }
            });
        }
    }

    pub fn select_message(&self, message_id: &str) {
        if let Some(client) = &self.client {
            let client = client.clone();
            let msg_id = message_id.to_string();
            let response_tx = self.response_tx.clone();
            self.runtime.spawn(async move {
                match client.get_message(&msg_id).await {
                    Ok(detail) => {
                        let _ = response_tx.send(AppResponse::MessageDetailLoaded(detail));
                    }
                    Err(e) => {
                        let _ = response_tx.send(AppResponse::StatusMessage(format!("Failed to load message detail: {}", e)));
                    }
                }
            });
        }
    }
}

impl App for EdvigeApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Auto-reconnect if disconnected
        if matches!(self.state.connection_status, ConnectionStatus::Disconnected(_)) {
            if let Some(last) = self.last_reconnect_attempt {
                if last.elapsed() > Duration::from_secs(3) {
                    self.connect_to_daemon();
                }
            }
        }

        // 1. Process asynchronous responses from background tasks
        while let Ok(resp) = self.response_rx.try_recv() {
            match resp {
                AppResponse::Connected(client) => {
                    self.client = Some(client);
                    self.state.connection_status = ConnectionStatus::Connected;
                    self.state.set_status("Connected to Edvige Daemon");
                    self.refresh_accounts();
                }
                AppResponse::ConnectionFailed(err) => {
                    self.client = None;
                    self.state.connection_status = ConnectionStatus::Disconnected(err);
                }
                AppResponse::AccountsLoaded(accounts) => {
                    self.state.accounts = accounts;
                    if self.state.selected_account_id.is_none() && !self.state.accounts.is_empty() {
                        let first_id = self.state.accounts[0].id.clone();
                        self.select_account(&first_id);
                    }
                }
                AppResponse::FoldersLoaded(folders) => {
                    self.state.folders = folders;
                    if self.state.selected_folder_id.is_none() && !self.state.folders.is_empty() {
                        let first_id = self.state.folders[0].id.clone();
                        self.select_folder(&first_id);
                    }
                }
                AppResponse::MessagesLoaded(messages) => {
                    self.state.messages = messages;
                    if self.state.selected_message_id.is_none() && !self.state.messages.is_empty() {
                        let first_msg_id = self.state.messages[0].id.clone();
                        self.state.selected_message_id = Some(first_msg_id.clone());
                        self.select_message(&first_msg_id);
                    }
                }
                AppResponse::MessageDetailLoaded(detail) => {
                    self.state.selected_message_detail = detail;
                }
                AppResponse::SearchResultsLoaded(messages) => {
                    self.state.messages = messages;
                }
                AppResponse::AccountCreated(account) => {
                    self.state.accounts.push(account.clone());
                    self.state.show_account_wizard = false;
                    self.state.set_status("Account created successfully");
                    self.select_account(&account.id);
                }
                AppResponse::FlagsUpdated(msg_id, flags) => {
                    if let Some(msg) = self.state.messages.iter_mut().find(|m| m.id == msg_id) {
                        msg.flags = Some(flags.clone());
                    }
                    if let Some(detail) = &mut self.state.selected_message_detail {
                        if let Some(summary) = &mut detail.summary {
                            if summary.id == msg_id {
                                summary.flags = Some(flags);
                            }
                        }
                    }
                }
                AppResponse::MessageDeleted(msg_id) => {
                    self.state.messages.retain(|m| m.id != msg_id);
                    if self.state.selected_message_id.as_deref() == Some(&msg_id) {
                        self.state.selected_message_id = None;
                        self.state.selected_message_detail = None;
                    }
                    self.state.set_status("Message deleted");
                }
                AppResponse::DraftSaved(_msg) => {
                    self.state.show_compose = false;
                    self.state.set_status("Draft saved");
                }
                AppResponse::StatusMessage(msg) => {
                    self.state.set_status(msg);
                }
            }
            ctx.request_repaint();
        }

        // 2. Process incoming live events from daemon broadcast stream
        while let Ok(event) = self.event_rx.try_recv() {
            if let Some(e) = event.event {
                match e {
                    edvige_proto::daemon_event_proto::Event::FolderUpdated(f) => {
                        if let Some(folder) = self.state.folders.iter_mut().find(|folder| folder.id == f.folder_id) {
                            folder.total_count = f.total_count;
                            folder.unread_count = f.unread_count;
                        }
                    }
                    edvige_proto::daemon_event_proto::Event::NewMessages(n) => {
                        if self.state.selected_folder_id.as_deref() == Some(&n.folder_id) {
                            self.refresh_messages();
                        }
                        self.state.set_status(format!("Synced {} new message(s)", n.count));
                    }
                    edvige_proto::daemon_event_proto::Event::FlagsChanged(fc) => {
                        if let Some(msg) = self.state.messages.iter_mut().find(|m| m.id == fc.message_id) {
                            msg.flags = fc.flags.clone();
                        }
                        if let Some(detail) = &mut self.state.selected_message_detail {
                            if let Some(summary) = &mut detail.summary {
                                if summary.id == fc.message_id {
                                    summary.flags = fc.flags;
                                }
                            }
                        }
                    }
                    edvige_proto::daemon_event_proto::Event::OutboxStatus(o) => {
                        let status_label = match OutboxStatusProto::try_from(o.status).unwrap_or(OutboxStatusProto::OutboxStatusDraft) {
                            OutboxStatusProto::OutboxStatusDraft => "Draft",
                            OutboxStatusProto::OutboxStatusQueued => "Queued for sending",
                            OutboxStatusProto::OutboxStatusSending => "Sending email...",
                            OutboxStatusProto::OutboxStatusSent => "Email sent successfully!",
                            OutboxStatusProto::OutboxStatusFailed => "Failed to send email",
                        };
                        self.state.set_status(format!("Outbox: {}", status_label));
                    }
                    edvige_proto::daemon_event_proto::Event::SyncProgress(p) => {
                        self.state.set_status(p.message);
                    }
                }
            }
            ctx.request_repaint();
        }

        // 3. Top Bar
        TopBottomPanel::top("top_bar_panel").show(ctx, |ui| {
            if let Some(action) = render_top_bar(ui, &mut self.state) {
                match action {
                    TopBarAction::Compose => {
                        self.state.show_compose = true;
                    }
                    TopBarAction::SyncFolder => {
                        if let (Some(client), Some(acc_id), Some(fld_id)) = (
                            &self.client,
                            &self.state.selected_account_id,
                            &self.state.selected_folder_id,
                        ) {
                            let client = client.clone();
                            let acc_id = acc_id.clone();
                            let fld_id = fld_id.clone();
                            let response_tx = self.response_tx.clone();
                            self.runtime.spawn(async move {
                                let _ = response_tx.send(AppResponse::StatusMessage("Syncing folder messages...".into()));
                                match client.sync_folder_messages(&acc_id, &fld_id).await {
                                    Ok(stats) => {
                                        let _ = response_tx.send(AppResponse::StatusMessage(format!(
                                            "Sync complete: {} new messages",
                                            stats.messages_fetched
                                        )));
                                    }
                                    Err(e) => {
                                        let _ = response_tx.send(AppResponse::StatusMessage(format!("Sync failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                    TopBarAction::Search(query) => {
                        self.state.is_searching = true;
                        if let (Some(client), Some(acc_id)) = (&self.client, &self.state.selected_account_id) {
                            let client = client.clone();
                            let acc_id = acc_id.clone();
                            let response_tx = self.response_tx.clone();
                            self.runtime.spawn(async move {
                                match client.search_messages(&acc_id, &query, 100, 0).await {
                                    Ok(msgs) => {
                                        let _ = response_tx.send(AppResponse::SearchResultsLoaded(msgs));
                                    }
                                    Err(e) => {
                                        let _ = response_tx.send(AppResponse::StatusMessage(format!("Search failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                    TopBarAction::ClearSearch => {
                        self.refresh_messages();
                    }
                    TopBarAction::OpenAccountWizard => {
                        self.state.show_account_wizard = true;
                    }
                    TopBarAction::OpenAbout => {
                        self.state.show_about = true;
                    }
                }
            }
        });

        // 4. Left Sidebar (Accounts & Folders)
        SidePanel::left("sidebar_panel")
            .default_width(200.0)
            .min_width(160.0)
            .show(ctx, |ui| {
                if let Some(action) = render_sidebar(ui, &mut self.state) {
                    match action {
                        SidebarAction::SelectAccount(acc_id) => {
                            self.select_account(&acc_id);
                        }
                        SidebarAction::SelectFolder(fld_id) => {
                            self.select_folder(&fld_id);
                        }
                        SidebarAction::SyncAllFolders => {
                            if let (Some(client), Some(acc_id)) = (&self.client, &self.state.selected_account_id) {
                                let client = client.clone();
                                let acc_id = acc_id.clone();
                                let response_tx = self.response_tx.clone();
                                self.runtime.spawn(async move {
                                    let _ = response_tx.send(AppResponse::StatusMessage("Refreshing folder list...".into()));
                                    match client.sync_folders(&acc_id).await {
                                        Ok(folders) => {
                                            let _ = response_tx.send(AppResponse::FoldersLoaded(folders));
                                            let _ = response_tx.send(AppResponse::StatusMessage("Folders updated".into()));
                                        }
                                        Err(e) => {
                                            let _ = response_tx.send(AppResponse::StatusMessage(format!("Folder sync failed: {}", e)));
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
            });

        // 5. Middle Pane (Message List)
        SidePanel::left("message_list_panel")
            .default_width(320.0)
            .min_width(220.0)
            .show(ctx, |ui| {
                if let Some(action) = render_message_list(ui, &mut self.state) {
                    match action {
                        MessageListAction::SelectMessage(msg_id) => {
                            self.state.selected_message_id = Some(msg_id.clone());
                            self.select_message(&msg_id);
                        }
                        MessageListAction::ToggleFlag(msg_id, new_flag) => {
                            if let (Some(client), Some(fld_id)) = (&self.client, &self.state.selected_folder_id) {
                                let client = client.clone();
                                let fld_id = fld_id.clone();
                                let mut add = MessageFlagsProto::default();
                                let mut remove = MessageFlagsProto::default();
                                if new_flag { add.flagged = true; } else { remove.flagged = true; }
                                let response_tx = self.response_tx.clone();
                                self.runtime.spawn(async move {
                                    let _ = client.set_flags(&msg_id, &fld_id, Some(add), Some(remove)).await;
                                    let mut flags = MessageFlagsProto::default();
                                    flags.flagged = new_flag;
                                    let _ = response_tx.send(AppResponse::FlagsUpdated(msg_id, flags));
                                });
                            }
                        }
                        MessageListAction::ToggleRead(msg_id, new_seen) => {
                            if let (Some(client), Some(fld_id)) = (&self.client, &self.state.selected_folder_id) {
                                let client = client.clone();
                                let fld_id = fld_id.clone();
                                let mut add = MessageFlagsProto::default();
                                let mut remove = MessageFlagsProto::default();
                                if new_seen { add.seen = true; } else { remove.seen = true; }
                                let response_tx = self.response_tx.clone();
                                self.runtime.spawn(async move {
                                    let _ = client.set_flags(&msg_id, &fld_id, Some(add), Some(remove)).await;
                                    let mut flags = MessageFlagsProto::default();
                                    flags.seen = new_seen;
                                    let _ = response_tx.send(AppResponse::FlagsUpdated(msg_id, flags));
                                });
                            }
                        }
                        MessageListAction::DeleteMessage(msg_id) => {
                            if let (Some(client), Some(fld_id)) = (&self.client, &self.state.selected_folder_id) {
                                let client = client.clone();
                                let fld_id = fld_id.clone();
                                let response_tx = self.response_tx.clone();
                                self.runtime.spawn(async move {
                                    let _ = client.delete_message(&msg_id, &fld_id, false).await;
                                    let _ = response_tx.send(AppResponse::MessageDeleted(msg_id));
                                });
                            }
                        }
                    }
                }
            });

        // 6. Right Central Pane (Message View / Reader)
        CentralPanel::default().show(ctx, |ui| {
            if let Some(action) = render_message_view(ui, &mut self.state) {
                match action {
                    MessageViewAction::Reply(_) | MessageViewAction::Forward(_) => {
                        self.state.show_compose = true;
                    }
                    MessageViewAction::ToggleFlag(msg_id, new_flag) => {
                        if let (Some(client), Some(fld_id)) = (&self.client, &self.state.selected_folder_id) {
                            let client = client.clone();
                            let fld_id = fld_id.clone();
                            let mut add = MessageFlagsProto::default();
                            let mut remove = MessageFlagsProto::default();
                            if new_flag { add.flagged = true; } else { remove.flagged = true; }
                            let response_tx = self.response_tx.clone();
                            self.runtime.spawn(async move {
                                let _ = client.set_flags(&msg_id, &fld_id, Some(add), Some(remove)).await;
                                let mut flags = MessageFlagsProto::default();
                                flags.flagged = new_flag;
                                let _ = response_tx.send(AppResponse::FlagsUpdated(msg_id, flags));
                            });
                        }
                    }
                    MessageViewAction::ToggleRead(msg_id, new_seen) => {
                        if let (Some(client), Some(fld_id)) = (&self.client, &self.state.selected_folder_id) {
                            let client = client.clone();
                            let fld_id = fld_id.clone();
                            let mut add = MessageFlagsProto::default();
                            let mut remove = MessageFlagsProto::default();
                            if new_seen { add.seen = true; } else { remove.seen = true; }
                            let response_tx = self.response_tx.clone();
                            self.runtime.spawn(async move {
                                let _ = client.set_flags(&msg_id, &fld_id, Some(add), Some(remove)).await;
                                let mut flags = MessageFlagsProto::default();
                                flags.seen = new_seen;
                                let _ = response_tx.send(AppResponse::FlagsUpdated(msg_id, flags));
                            });
                        }
                    }
                    MessageViewAction::Delete(msg_id) => {
                        if let (Some(client), Some(fld_id)) = (&self.client, &self.state.selected_folder_id) {
                            let client = client.clone();
                            let fld_id = fld_id.clone();
                            let response_tx = self.response_tx.clone();
                            self.runtime.spawn(async move {
                                let _ = client.delete_message(&msg_id, &fld_id, false).await;
                                let _ = response_tx.send(AppResponse::MessageDeleted(msg_id));
                            });
                        }
                    }
                    MessageViewAction::OpenHtmlInBrowser(html) => {
                        let tmp_path = std::env::temp_dir().join(format!("edvige_mail_{}.html", Uuid::now_v7()));
                        if std::fs::write(&tmp_path, html).is_ok() {
                            let _ = open::that(&tmp_path);
                        }
                    }
                    MessageViewAction::DownloadAttachment(blob_hash, filename) => {
                        if let Some(client) = &self.client {
                            let client = client.clone();
                            let response_tx = self.response_tx.clone();
                            self.runtime.spawn(async move {
                                if let Ok(bytes) = client.get_blob(&blob_hash).await {
                                    if let Some(dest_path) = rfd::FileDialog::new().set_file_name(&filename).save_file() {
                                        if tokio::fs::write(&dest_path, bytes).await.is_ok() {
                                            let _ = response_tx.send(AppResponse::StatusMessage(format!(
                                                "Saved attachment to {}",
                                                dest_path.display()
                                            )));
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            }
        });

        // 7. Modals (Composer and Account Wizard)
        if let Some(action) = render_composer(ctx, &mut self.state) {
            match action {
                ComposerAction::Send => {
                    if let (Some(client), Some(acc)) = (&self.client, self.state.selected_account()) {
                        let outbox_proto = OutboxMessageProto {
                            id: Uuid::now_v7().to_string(),
                            account_id: acc.id.clone(),
                            from: Some(edvige_proto::EmailAddressProto {
                                name: Some(acc.name.clone()),
                                address: acc.email.clone(),
                            }),
                            to: vec![edvige_proto::EmailAddressProto {
                                name: None,
                                address: self.state.composer_to.trim().to_string(),
                            }],
                            cc: vec![],
                            bcc: vec![],
                            subject: self.state.composer_subject.clone(),
                            body_text: Some(self.state.composer_body.clone()),
                            body_html: None,
                            in_reply_to: None,
                            references: None,
                            attachments: self
                                .state
                                .composer_attachments
                                .iter()
                                .map(|(name, ctype, data)| DraftAttachmentProto {
                                    filename: name.clone(),
                                    content_type: ctype.clone(),
                                    data: data.clone(),
                                    content_id: None,
                                    is_inline: false,
                                })
                                .collect(),
                            status: OutboxStatusProto::OutboxStatusQueued.into(),
                            retry_count: 0,
                            last_error: None,
                            created_at: chrono::Utc::now().to_rfc3339(),
                            updated_at: chrono::Utc::now().to_rfc3339(),
                            sent_at: None,
                        };

                        let client = client.clone();
                        let response_tx = self.response_tx.clone();
                        self.runtime.spawn(async move {
                            if let Ok(saved) = client.save_draft(outbox_proto).await {
                                let _ = client.queue_send(&saved.id).await;
                                let _ = response_tx.send(AppResponse::StatusMessage("Message sent to outbox".into()));
                            }
                        });

                        self.state.show_compose = false;
                        self.state.set_status("Message queued for sending");
                    }
                }
                ComposerAction::SaveDraft => {
                    if let (Some(client), Some(acc)) = (&self.client, self.state.selected_account()) {
                        let outbox_proto = OutboxMessageProto {
                            id: Uuid::now_v7().to_string(),
                            account_id: acc.id.clone(),
                            from: Some(edvige_proto::EmailAddressProto {
                                name: Some(acc.name.clone()),
                                address: acc.email.clone(),
                            }),
                            to: vec![edvige_proto::EmailAddressProto {
                                name: None,
                                address: self.state.composer_to.trim().to_string(),
                            }],
                            cc: vec![],
                            bcc: vec![],
                            subject: self.state.composer_subject.clone(),
                            body_text: Some(self.state.composer_body.clone()),
                            body_html: None,
                            in_reply_to: None,
                            references: None,
                            attachments: vec![],
                            status: OutboxStatusProto::OutboxStatusDraft.into(),
                            retry_count: 0,
                            last_error: None,
                            created_at: chrono::Utc::now().to_rfc3339(),
                            updated_at: chrono::Utc::now().to_rfc3339(),
                            sent_at: None,
                        };

                        let client = client.clone();
                        let response_tx = self.response_tx.clone();
                        self.runtime.spawn(async move {
                            if let Ok(saved) = client.save_draft(outbox_proto).await {
                                let _ = response_tx.send(AppResponse::DraftSaved(saved));
                            }
                        });
                    }
                }
                ComposerAction::PickAttachment => {
                    if let Some(file_path) = rfd::FileDialog::new().pick_file() {
                        if let Ok(bytes) = std::fs::read(&file_path) {
                            let filename = file_path
                                .file_name()
                                .and_then(|f| f.to_str())
                                .unwrap_or("attachment")
                                .to_string();
                            self.state.composer_attachments.push((
                                filename,
                                "application/octet-stream".to_string(),
                                bytes,
                            ));
                        }
                    }
                }
                ComposerAction::RemoveAttachment(idx) => {
                    if idx < self.state.composer_attachments.len() {
                        self.state.composer_attachments.remove(idx);
                    }
                }
                ComposerAction::Cancel => {
                    self.state.show_compose = false;
                }
            }
        }

        if let Some(action) = render_account_wizard(ctx, &mut self.state) {
            match action {
                AccountWizardAction::SaveAccount => {
                    if let Some(client) = &self.client {
                        let client = client.clone();
                        let req = CreateAccountRequest {
                            name: self.state.wizard_name.clone(),
                            email: self.state.wizard_email.clone(),
                            imap_config: Some(ServerConfigProto {
                                host: self.state.wizard_imap_host.clone(),
                                port: self.state.wizard_imap_port as u32,
                                security: self.state.wizard_imap_sec.into(),
                            }),
                            smtp_config: Some(ServerConfigProto {
                                host: self.state.wizard_smtp_host.clone(),
                                port: self.state.wizard_smtp_port as u32,
                                security: self.state.wizard_smtp_sec.into(),
                            }),
                            credentials: Some(AccountCredentialsProto {
                                username: self.state.wizard_user.clone(),
                                password: self.state.wizard_pass.clone(),
                            }),
                        };

                        let response_tx = self.response_tx.clone();
                        self.runtime.spawn(async move {
                            match client.create_account(req).await {
                                Ok(created) => {
                                    let _ = response_tx.send(AppResponse::AccountCreated(created));
                                }
                                Err(e) => {
                                    let _ = response_tx.send(AppResponse::StatusMessage(format!("Failed to create account: {}", e)));
                                }
                            }
                        });
                    } else {
                        self.state.set_status("Cannot save account: Not connected to daemon.");
                    }
                }
                AccountWizardAction::Cancel => {
                    self.state.show_account_wizard = false;
                }
            }
        }

        self.about_modal.render(ctx, &mut self.state.show_about);
    }
}
