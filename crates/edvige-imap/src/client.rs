use edvige_core::{
    AccountCredentials, FolderRole, MessageFlags, ServerConfig,
};

use crate::connection::ImapConnection;
use crate::error::ImapError;
use crate::protocol::commands::ImapCommand;
use crate::protocol::response::{FetchResponse, UntaggedResponse};

#[derive(Debug, Clone)]
pub struct RemoteFolderInfo {
    pub name: String,
    pub delimiter: Option<String>,
    pub flags: Vec<String>,
    pub role: FolderRole,
}

#[derive(Debug, Clone, Default)]
pub struct SelectedFolderState {
    pub name: String,
    pub uid_validity: Option<u32>,
    pub uid_next: Option<u32>,
    pub exists: u32,
    pub recent: u32,
    pub read_only: bool,
}

pub struct ImapSession {
    connection: ImapConnection,
}

impl ImapSession {
    pub async fn connect(
        config: &ServerConfig,
        credentials: &AccountCredentials,
    ) -> Result<Self, ImapError> {
        let mut connection = ImapConnection::connect(config).await?;

        // Authenticate with LOGIN
        let login_cmd = ImapCommand::Login {
            username: credentials.username.clone(),
            password: credentials.password.clone(),
        };

        connection.execute(login_cmd).await.map_err(|e| {
            ImapError::Authentication(format!("Login failed for {}: {}", credentials.username, e))
        })?;

        tracing::info!("Authenticated successfully as {}", credentials.username);
        Ok(Self { connection })
    }

    pub async fn list_folders(&mut self) -> Result<Vec<RemoteFolderInfo>, ImapError> {
        let cmd = ImapCommand::List {
            reference: "".to_string(),
            wildcard: "*".to_string(),
        };

        let (_tagged, untagged) = self.connection.execute(cmd).await?;
        let mut folders = Vec::new();

        for resp in untagged {
            if let UntaggedResponse::List {
                flags,
                delimiter,
                name,
            } = resp
            {
                // Don't list \Noselect folders as standard selectable folders
                let role = FolderRole::from_name(&name);
                folders.push(RemoteFolderInfo {
                    name,
                    delimiter,
                    flags,
                    role,
                });
            }
        }

        Ok(folders)
    }

    pub async fn select_folder(
        &mut self,
        folder_name: &str,
    ) -> Result<SelectedFolderState, ImapError> {
        let cmd = ImapCommand::Select {
            mailbox: folder_name.to_string(),
        };

        let (tagged, untagged) = self.connection.execute(cmd).await?;
        let mut state = SelectedFolderState {
            name: folder_name.to_string(),
            ..Default::default()
        };

        if let Some(code) = &tagged.code {
            if code.eq_ignore_ascii_case("READ-ONLY") {
                state.read_only = true;
            }
        }

        for resp in untagged {
            match resp {
                UntaggedResponse::Exists(count) => {
                    state.exists = count;
                }
                UntaggedResponse::Recent(count) => {
                    state.recent = count;
                }
                UntaggedResponse::Ok { code, .. } => {
                    if let Some(code_str) = code {
                        if code_str.to_ascii_uppercase().starts_with("UIDVALIDITY ") {
                            if let Ok(val) = code_str[12..].trim().parse::<u32>() {
                                state.uid_validity = Some(val);
                            }
                        } else if code_str.to_ascii_uppercase().starts_with("UIDNEXT ") {
                            if let Ok(val) = code_str[8..].trim().parse::<u32>() {
                                state.uid_next = Some(val);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(state)
    }

    pub async fn search_uids(&mut self, query: &str) -> Result<Vec<u32>, ImapError> {
        let cmd = ImapCommand::UidSearch {
            query: query.to_string(),
        };

        let (_tagged, untagged) = self.connection.execute(cmd).await?;
        let mut uids = Vec::new();

        for resp in untagged {
            if let UntaggedResponse::Other(line) = resp {
                if line.to_ascii_uppercase().starts_with("SEARCH") {
                    for part in line[6..].split_whitespace() {
                        if let Ok(uid) = part.parse::<u32>() {
                            uids.push(uid);
                        }
                    }
                }
            }
        }

        uids.sort_unstable();
        Ok(uids)
    }

    pub async fn fetch_messages(
        &mut self,
        uid_range: &str,
    ) -> Result<Vec<FetchResponse>, ImapError> {
        let cmd = ImapCommand::UidFetch {
            uid_range: uid_range.to_string(),
            items: "UID FLAGS RFC822.SIZE RFC822".to_string(),
        };

        let (_tagged, untagged) = self.connection.execute(cmd).await?;
        let mut results = Vec::new();

        for resp in untagged {
            if let UntaggedResponse::Fetch(fetch_data) = resp {
                results.push(fetch_data);
            }
        }

        Ok(results)
    }

    pub async fn store_flags(
        &mut self,
        uid: u32,
        add: bool,
        flags: MessageFlags,
    ) -> Result<(), ImapError> {
        let cmd = ImapCommand::UidStoreFlags { uid, add, flags };
        self.connection.execute(cmd).await?;
        Ok(())
    }

    pub async fn move_message(&mut self, uid: u32, target_mailbox: &str) -> Result<(), ImapError> {
        let cmd = ImapCommand::UidMove {
            uid,
            target_mailbox: target_mailbox.to_string(),
        };

        // If UID MOVE succeeds, return Ok. If not supported, fallback to COPY + STORE \Deleted + EXPUNGE
        match self.connection.execute(cmd).await {
            Ok(_) => Ok(()),
            Err(_) => {
                let copy_cmd = ImapCommand::UidCopy {
                    uid,
                    target_mailbox: target_mailbox.to_string(),
                };
                self.connection.execute(copy_cmd).await?;

                let delete_flags = MessageFlags {
                    deleted: true,
                    ..Default::default()
                };
                self.store_flags(uid, true, delete_flags).await?;
                Ok(())
            }
        }
    }

    pub async fn delete_message(&mut self, uid: u32) -> Result<(), ImapError> {
        let delete_flags = MessageFlags {
            deleted: true,
            ..Default::default()
        };
        self.store_flags(uid, true, delete_flags).await?;
        Ok(())
    }

    pub async fn start_idle(&mut self) -> Result<String, ImapError> {
        self.connection.start_idle().await
    }

    pub async fn stop_idle(&mut self, tag: &str) -> Result<(), ImapError> {
        self.connection.stop_idle(tag).await?;
        Ok(())
    }

    pub async fn read_unsolicited_line(&mut self) -> Result<UntaggedResponse, ImapError> {
        let line = self.connection.read_line().await?;
        match crate::protocol::parser::parse_line(&line) {
            Ok(crate::protocol::response::ImapLine::Untagged(u)) => Ok(u),
            _ => Ok(UntaggedResponse::Other(line)),
        }
    }
}
