use edvige_core::{
    Account, AccountCredentials, AccountId, DraftAttachment, EmailAddress, Folder, FolderId,
    FolderRole, MessageDetail, MessageFlags, MessageId, MessageSummary, OutboxMessage,
    ServerConfig,
};
use eframe::egui::{self, CentralPanel, SidePanel, TopBottomPanel};
use edvige_storage::StorageEngine;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ipc::{GuiToSupervisor, SupervisorToGui};
use crate::state::AppState;
use crate::ui::{
    render_account_wizard, render_composer, render_message_list, render_message_view,
    render_sidebar, render_top_bar, AboutModal, AccountWizardAction, ComposerAction,
    MessageListAction, MessageViewAction, SidebarAction, TopBarAction,
};

pub enum AppResponse {
    AccountsLoaded(Vec<Account>),
    FoldersLoaded(Vec<Folder>),
    MessagesLoaded(Vec<MessageSummary>),
    MessageDetailLoaded(Option<MessageDetail>),
    FlagsUpdated(MessageId, MessageFlags),
    MessageDeleted(MessageId),
    StatusMessage(String),
}

pub struct EdvigeApp {
    pub state: AppState,
    storage: StorageEngine,
    ipc_tx: mpsc::UnboundedSender<GuiToSupervisor>,
    ipc_rx: mpsc::UnboundedReceiver<SupervisorToGui>,
    tokio_handle: tokio::runtime::Handle,
    response_rx: mpsc::UnboundedReceiver<AppResponse>,
    response_tx: mpsc::UnboundedSender<AppResponse>,
    about_modal: AboutModal,
}

impl EdvigeApp {
    pub fn new(
        state: AppState,
        storage: StorageEngine,
        ipc_tx: mpsc::UnboundedSender<GuiToSupervisor>,
        ipc_rx: mpsc::UnboundedReceiver<SupervisorToGui>,
    ) -> Self {
        let tokio_handle = tokio::runtime::Handle::current();
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        let app = Self {
            state,
            storage,
            ipc_tx,
            ipc_rx,
            tokio_handle,
            response_rx,
            response_tx,
            about_modal: AboutModal::new(),
        };

        app.load_accounts();
        app
    }

    pub fn load_accounts(&self) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.list_accounts().await {
                Ok(accounts) => {
                    let _ = tx.send(AppResponse::AccountsLoaded(accounts));
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to load accounts: {}", e)));
                }
            }
        });
    }

    pub fn load_folders(&self, account_id: AccountId) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.list_folders_for_account(account_id).await {
                Ok(folders) => {
                    let _ = tx.send(AppResponse::FoldersLoaded(folders));
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to load folders: {}", e)));
                }
            }
        });
    }

    pub fn load_messages(&self, folder_id: FolderId) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.list_messages_summary(folder_id, 100, 0).await {
                Ok(messages) => {
                    let _ = tx.send(AppResponse::MessagesLoaded(messages));
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to load messages: {}", e)));
                }
            }
        });
    }

    pub fn load_message_detail(&self, message_id: MessageId) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.get_message_detail(message_id).await {
                Ok(detail) => {
                    let _ = tx.send(AppResponse::MessageDetailLoaded(detail));
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to load message: {}", e)));
                }
            }
        });
    }

    pub fn update_message_flags(&self, message_id: MessageId, flags: MessageFlags) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.update_message_flags(message_id, flags).await {
                Ok(()) => {
                    let _ = tx.send(AppResponse::FlagsUpdated(message_id, flags));
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to update flags: {}", e)));
                }
            }
        });
    }

    pub fn delete_message(&self, message_id: MessageId) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.delete_message(message_id).await {
                Ok(_) => {
                    let _ = tx.send(AppResponse::MessageDeleted(message_id));
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to delete message: {}", e)));
                }
            }
        });
    }

    pub fn save_account(&self, account: Account) {
        let storage = self.storage.clone();
        let ipc_tx = self.ipc_tx.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            match storage.insert_account(&account).await {
                Ok(()) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Account {} added", account.email)));
                    let _ = ipc_tx.send(GuiToSupervisor::AccountAdded { account_id: account.id });
                    if let Ok(accounts) = storage.list_accounts().await {
                        let _ = tx.send(AppResponse::AccountsLoaded(accounts));
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to save account: {}", e)));
                }
            }
        });
    }

    pub fn send_message(&self, outbox: OutboxMessage) {
        let storage = self.storage.clone();
        let ipc_tx = self.ipc_tx.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            let account_id = outbox.account_id;
            match storage.save_outbox_message(&outbox).await {
                Ok(()) => {
                    let _ = tx.send(AppResponse::StatusMessage("Message queued for sending".into()));
                    let _ = ipc_tx.send(GuiToSupervisor::DispatchOutbox { account_id });
                }
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to queue message: {}", e)));
                }
            }
        });
    }

    pub fn download_attachment(&self, blob_hash: String, default_filename: String) {
        let storage = self.storage.clone();
        let tx = self.response_tx.clone();
        self.tokio_handle.spawn(async move {
            let data = match storage.blobs().read(&blob_hash).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to read attachment blob: {}", e)));
                    return;
                }
            };

            if let Some(dest_path) = rfd::FileDialog::new().set_file_name(&default_filename).save_file() {
                if let Err(e) = tokio::fs::write(&dest_path, data).await {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Failed to save file: {}", e)));
                } else {
                    let _ = tx.send(AppResponse::StatusMessage(format!("Saved {}", dest_path.display())));
                }
            }
        });
    }

    fn handle_incoming_ipc(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.ipc_rx.try_recv() {
            match msg {
                SupervisorToGui::Focus => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                }
                SupervisorToGui::Compose => {
                    self.state.show_compose = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                }
                SupervisorToGui::StatusMessage { message } => {
                    self.state.set_status(message);
                }
                SupervisorToGui::FolderUpdated {
                    account_id,
                    folder_id,
                    total_count,
                    unread_count,
                } => {
                    if self.state.selected_account_id == Some(account_id) {
                        if let Some(folder) = self.state.folders.iter_mut().find(|f| f.id == folder_id) {
                            folder.total_count = total_count;
                            folder.unread_count = unread_count;
                        }
                    }
                }
                SupervisorToGui::NewMessages {
                    account_id,
                    folder_id,
                    ..
                } => {
                    if self.state.selected_account_id == Some(account_id)
                        && self.state.selected_folder_id == Some(folder_id)
                        && !self.state.is_searching
                    {
                        self.load_messages(folder_id);
                    }
                }
                SupervisorToGui::FlagsChanged {
                    folder_id,
                    message_id,
                    flags,
                } => {
                    if self.state.selected_folder_id == Some(folder_id) {
                        if let Some(msg) = self.state.messages.iter_mut().find(|m| m.id == message_id) {
                            msg.flags = flags;
                        }
                        if let Some(detail) = &mut self.state.selected_message_detail {
                            if detail.summary.id == message_id {
                                detail.summary.flags = flags;
                            }
                        }
                    }
                }
            }
        }
    }

    fn handle_responses(&mut self) {
        while let Ok(resp) = self.response_rx.try_recv() {
            match resp {
                AppResponse::AccountsLoaded(accounts) => {
                    let prev_account_id = self.state.selected_account_id;
                    self.state.accounts = accounts;
                    if let Some(first) = self.state.accounts.first() {
                        if prev_account_id.is_none() || !self.state.accounts.iter().any(|a| a.id == prev_account_id.unwrap()) {
                            let first_id = first.id;
                            self.state.selected_account_id = Some(first_id);
                            self.load_folders(first_id);
                        }
                    }
                }
                AppResponse::FoldersLoaded(folders) => {
                    let prev_folder_id = self.state.selected_folder_id;
                    self.state.folders = folders;
                    // Select Inbox if available, else first folder
                    let inbox = self.state.folders.iter().find(|f| f.role == FolderRole::Inbox);
                    let folder_to_select = inbox.or_else(|| self.state.folders.first());
                    if let Some(folder) = folder_to_select {
                        if prev_folder_id.is_none() || !self.state.folders.iter().any(|f| f.id == prev_folder_id.unwrap()) {
                            let folder_id = folder.id;
                            self.state.selected_folder_id = Some(folder_id);
                            self.load_messages(folder_id);
                        }
                    }
                }
                AppResponse::MessagesLoaded(messages) => {
                    self.state.messages = messages;
                }
                AppResponse::MessageDetailLoaded(detail) => {
                    self.state.selected_message_detail = detail;
                }
                AppResponse::FlagsUpdated(msg_id, flags) => {
                    if let Some(msg) = self.state.messages.iter_mut().find(|m| m.id == msg_id) {
                        msg.flags = flags;
                    }
                    if let Some(detail) = &mut self.state.selected_message_detail {
                        if detail.summary.id == msg_id {
                            detail.summary.flags = flags;
                        }
                    }
                }
                AppResponse::MessageDeleted(msg_id) => {
                    self.state.messages.retain(|m| m.id != msg_id);
                    if self.state.selected_message_id == Some(msg_id) {
                        self.state.selected_message_id = None;
                        self.state.selected_message_detail = None;
                    }
                }
                AppResponse::StatusMessage(msg) => {
                    self.state.set_status(msg);
                }
            }
        }
    }
}

impl eframe::App for EdvigeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_incoming_ipc(ctx);
        self.handle_responses();

        // 1. Top Bar
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            if let Some(action) = render_top_bar(ui, &mut self.state) {
                match action {
                    TopBarAction::Compose => {
                        self.state.show_compose = true;
                    }
                    TopBarAction::SyncFolder => {
                        if let (Some(account_id), Some(folder_id)) = (self.state.selected_account_id, self.state.selected_folder_id) {
                            let _ = self.ipc_tx.send(GuiToSupervisor::SyncFolder { account_id, folder_id });
                            self.state.set_status("Synchronizing folder...");
                        }
                    }
                    TopBarAction::Search(query) => {
                        self.state.is_searching = true;
                        if let Some(account_id) = self.state.selected_account_id {
                            let storage = self.storage.clone();
                            let tx = self.response_tx.clone();
                            self.tokio_handle.spawn(async move {
                                match storage.search_messages(account_id, &query, 100, 0).await {
                                    Ok(msgs) => {
                                        let _ = tx.send(AppResponse::MessagesLoaded(msgs));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(AppResponse::StatusMessage(format!("Search failed: {}", e)));
                                    }
                                }
                            });
                        }
                    }
                    TopBarAction::ClearSearch => {
                        self.state.is_searching = false;
                        if let Some(folder_id) = self.state.selected_folder_id {
                            self.load_messages(folder_id);
                        }
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

        // 2. Sidebar
        SidePanel::left("sidebar")
            .resizable(true)
            .default_width(220.0)
            .width_range(160.0..=350.0)
            .show(ctx, |ui| {
                if let Some(action) = render_sidebar(ui, &mut self.state) {
                    match action {
                        SidebarAction::SelectAccount(account_id) => {
                            self.state.selected_account_id = Some(account_id);
                            self.state.selected_folder_id = None;
                            self.state.selected_message_id = None;
                            self.state.selected_message_detail = None;
                            self.state.messages.clear();
                            self.load_folders(account_id);
                        }
                        SidebarAction::SelectFolder(folder_id) => {
                            self.state.selected_folder_id = Some(folder_id);
                            self.state.selected_message_id = None;
                            self.state.selected_message_detail = None;
                            self.load_messages(folder_id);
                        }
                        SidebarAction::SyncAllFolders => {
                            if let Some(account_id) = self.state.selected_account_id {
                                let _ = self.ipc_tx.send(GuiToSupervisor::SyncAccountFolders { account_id });
                                self.state.set_status("Synchronizing folders...");
                            }
                        }
                    }
                }
            });

        // 3. Message List Panel
        SidePanel::left("message_list_panel")
            .resizable(true)
            .default_width(330.0)
            .width_range(240.0..=550.0)
            .show(ctx, |ui| {
                if let Some(action) = render_message_list(ui, &mut self.state) {
                    match action {
                        MessageListAction::SelectMessage(id) => {
                            self.state.selected_message_id = Some(id);
                            self.load_message_detail(id);
                        }
                        MessageListAction::ToggleFlag(id, flag) => {
                            let mut flags = self.state.messages.iter()
                                .find(|m| m.id == id)
                                .map(|m| m.flags)
                                .unwrap_or_default();
                            flags.flagged = flag;
                            self.update_message_flags(id, flags);
                        }
                        MessageListAction::ToggleRead(id, seen) => {
                            let mut flags = self.state.messages.iter()
                                .find(|m| m.id == id)
                                .map(|m| m.flags)
                                .unwrap_or_default();
                            flags.seen = seen;
                            self.update_message_flags(id, flags);
                        }
                        MessageListAction::DeleteMessage(id) => {
                            self.delete_message(id);
                        }
                    }
                }
            });

        // 4. Central Panel: Message View
        CentralPanel::default().show(ctx, |ui| {
            if let Some(action) = render_message_view(ui, &mut self.state) {
                match action {
                    MessageViewAction::Reply(id) => {
                        if let Some(detail) = &self.state.selected_message_detail {
                            if detail.summary.id == id {
                                if let Some(sender) = &detail.summary.sender {
                                    self.state.composer_to = sender.address.clone();
                                    self.state.composer_subject = if detail.summary.subject.starts_with("Re:") {
                                        detail.summary.subject.clone()
                                    } else {
                                        format!("Re: {}", detail.summary.subject)
                                    };
                                    self.state.composer_body = format!(
                                        "\n\n--- On {} wrote ---\n{}",
                                        detail.summary.date.map(|d| d.to_rfc3339()).unwrap_or_default(),
                                        detail.body_text.as_deref().unwrap_or("")
                                    );
                                    self.state.show_compose = true;
                                }
                            }
                        }
                    }
                    MessageViewAction::Forward(id) => {
                        if let Some(detail) = &self.state.selected_message_detail {
                            if detail.summary.id == id {
                                self.state.composer_to.clear();
                                self.state.composer_subject = if detail.summary.subject.starts_with("Fwd:") {
                                    detail.summary.subject.clone()
                                } else {
                                    format!("Fwd: {}", detail.summary.subject)
                                };
                                self.state.composer_body = format!(
                                    "\n\n--- Forwarded Message ---\n{}",
                                    detail.body_text.as_deref().unwrap_or("")
                                );
                                self.state.show_compose = true;
                            }
                        }
                    }
                    MessageViewAction::ToggleFlag(id, flag) => {
                        let mut flags = self.state.messages.iter()
                            .find(|m| m.id == id)
                            .map(|m| m.flags)
                            .unwrap_or_default();
                        flags.flagged = flag;
                        self.update_message_flags(id, flags);
                    }
                    MessageViewAction::ToggleRead(id, seen) => {
                        let mut flags = self.state.messages.iter()
                            .find(|m| m.id == id)
                            .map(|m| m.flags)
                            .unwrap_or_default();
                        flags.seen = seen;
                        self.update_message_flags(id, flags);
                    }
                    MessageViewAction::Delete(id) => {
                        self.delete_message(id);
                    }
                    MessageViewAction::OpenHtmlInBrowser(html) => {
                        let tmp_path = std::env::temp_dir().join(format!("edvige_mail_{}.html", Uuid::now_v7()));
                        if std::fs::write(&tmp_path, html).is_ok() {
                            let _ = open::that(&tmp_path);
                        }
                    }
                    MessageViewAction::DownloadAttachment(blob_hash, filename) => {
                        self.download_attachment(blob_hash, filename);
                    }
                }
            }
        });

        // 4. Modals
        if let Some(action) = render_composer(ctx, &mut self.state) {
            match action {
                ComposerAction::Send => {
                    if let Some(acc) = self.state.selected_account().cloned() {
                        let to_addrs: Vec<EmailAddress> = self
                            .state
                            .composer_to
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(EmailAddress::new)
                            .collect();
                        let cc_addrs: Vec<EmailAddress> = self
                            .state
                            .composer_cc
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(EmailAddress::new)
                            .collect();
                        let bcc_addrs: Vec<EmailAddress> = self
                            .state
                            .composer_bcc
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(EmailAddress::new)
                            .collect();

                        let mut outbox = OutboxMessage::new_draft(
                            acc.id,
                            EmailAddress::with_name(&acc.name, &acc.email),
                            to_addrs,
                            &self.state.composer_subject,
                        );
                        outbox.cc = cc_addrs;
                        outbox.bcc = bcc_addrs;
                        outbox.body_text = Some(self.state.composer_body.clone());
                        outbox.attachments = self
                            .state
                            .composer_attachments
                            .iter()
                            .map(|(name, ctype, data)| DraftAttachment {
                                filename: name.clone(),
                                content_type: ctype.clone(),
                                data: data.clone(),
                                content_id: None,
                                is_inline: false,
                            })
                            .collect();
                        outbox.queue();

                        self.send_message(outbox);
                        self.state.show_compose = false;
                        self.state.composer_to.clear();
                        self.state.composer_cc.clear();
                        self.state.composer_bcc.clear();
                        self.state.composer_subject.clear();
                        self.state.composer_body.clear();
                        self.state.composer_attachments.clear();
                    }
                }
                ComposerAction::SaveDraft => {
                    if let Some(acc) = self.state.selected_account().cloned() {
                        let to_addrs: Vec<EmailAddress> = self
                            .state
                            .composer_to
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(EmailAddress::new)
                            .collect();
                        let mut outbox = OutboxMessage::new_draft(
                            acc.id,
                            EmailAddress::with_name(&acc.name, &acc.email),
                            to_addrs,
                            &self.state.composer_subject,
                        );
                        outbox.body_text = Some(self.state.composer_body.clone());
                        let storage = self.storage.clone();
                        let tx = self.response_tx.clone();
                        self.tokio_handle.spawn(async move {
                            if storage.save_outbox_message(&outbox).await.is_ok() {
                                let _ = tx.send(AppResponse::StatusMessage("Draft saved".into()));
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
                    let name = self.state.wizard_name.clone();
                    let email = self.state.wizard_email.clone();
                    let imap_cfg = ServerConfig {
                        host: self.state.wizard_imap_host.clone(),
                        port: self.state.wizard_imap_port,
                        security: self.state.wizard_imap_sec,
                    };
                    let smtp_cfg = ServerConfig {
                        host: self.state.wizard_smtp_host.clone(),
                        port: self.state.wizard_smtp_port,
                        security: self.state.wizard_smtp_sec,
                    };
                    let creds = AccountCredentials {
                        username: self.state.wizard_user.clone(),
                        password: self.state.wizard_pass.clone(),
                    };

                    let account = Account::new(&name, &email, imap_cfg, smtp_cfg, creds);
                    self.save_account(account);
                    self.state.show_account_wizard = false;
                }
                AccountWizardAction::Cancel => {
                    self.state.show_account_wizard = false;
                }
            }
        }

        self.about_modal.render(ctx, &mut self.state.show_about);
    }
}
