//! IPC communication between Supervisor, GUI child process, and CLI commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use edvige_core::{AccountId, FolderId, MessageFlags, MessageId};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CliCommand {
    Show,
    Compose,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuiToSupervisor {
    Hello,
    SyncAccountFolders { account_id: AccountId },
    SyncFolder { account_id: AccountId, folder_id: FolderId },
    DispatchOutbox { account_id: AccountId },
    AccountAdded { account_id: AccountId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SupervisorToGui {
    Focus,
    Compose,
    StatusMessage { message: String },
    FolderUpdated {
        account_id: AccountId,
        folder_id: FolderId,
        total_count: u32,
        unread_count: u32,
    },
    NewMessages {
        account_id: AccountId,
        folder_id: FolderId,
        count: u32,
    },
    FlagsChanged {
        folder_id: FolderId,
        message_id: MessageId,
        flags: MessageFlags,
    },
}

pub enum Outcome {
    Supervisor(SupervisorSocketGuard),
    Surfaced,
}

pub struct SupervisorSocketGuard {
    pub socket_path: PathBuf,
}

impl Drop for SupervisorSocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Try sending a CLI command to an already-running supervisor.
pub async fn send_cli_command(
    socket_path: impl AsRef<Path>,
    cmd: CliCommand,
) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket_path).await?;
    let json = serde_json::to_string(&cmd)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    stream.write_all(format!("{}\n", json).as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader.read_line(&mut reply).await?;
    if reply.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Invalid reply from running instance",
        ))
    }
}

/// Connect a GUI child process to the running supervisor.
pub async fn connect_gui(
    socket_path: impl AsRef<Path>,
    waker: Option<Arc<dyn Fn() + Send + Sync>>,
) -> std::io::Result<(
    mpsc::UnboundedSender<GuiToSupervisor>,
    mpsc::UnboundedReceiver<SupervisorToGui>,
)> {
    let stream = UnixStream::connect(socket_path).await?;
    let (read_half, mut write_half) = stream.into_split();

    let (gui_tx, mut gui_rx) = mpsc::unbounded_channel::<GuiToSupervisor>();
    let (sup_tx, sup_rx) = mpsc::unbounded_channel::<SupervisorToGui>();

    // Initial handshake
    let hello = serde_json::to_string(&GuiToSupervisor::Hello)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
        + "\n";
    write_half.write_all(hello.as_bytes()).await?;

    // Outgoing messages
    tokio::spawn(async move {
        while let Some(msg) = gui_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if write_half.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                    break;
                }
            }
        }
    });

    // Incoming messages
    tokio::spawn(async move {
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line).await {
            if n == 0 {
                break;
            }
            if let Ok(cmd) = serde_json::from_str::<SupervisorToGui>(line.trim()) {
                let _ = sup_tx.send(cmd);
                if let Some(ref w) = waker {
                    w();
                }
            }
            line.clear();
        }
    });

    Ok((gui_tx, sup_rx))
}

pub type GuiClientSlot = Arc<Mutex<Option<mpsc::UnboundedSender<SupervisorToGui>>>>;

/// Acquire the supervisor socket lock, or notify existing instance.
pub async fn acquire_supervisor(
    socket_path: PathBuf,
    initial_cmd: CliCommand,
    cli_cmd_tx: mpsc::UnboundedSender<CliCommand>,
    gui_cmd_tx: mpsc::UnboundedSender<GuiToSupervisor>,
    gui_client_slot: GuiClientSlot,
) -> Outcome {
    // 1. Check if supervisor is already active
    if send_cli_command(&socket_path, initial_cmd).await.is_ok() {
        return Outcome::Surfaced;
    }

    // 2. Remove stale socket
    let _ = std::fs::remove_file(&socket_path);
    if let Some(parent) = socket_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("Could not bind supervisor socket: {:?}. Running unguarded.", e);
            return Outcome::Supervisor(SupervisorSocketGuard { socket_path });
        }
    };

    // 3. Listen for connections
    let cli_tx = cli_cmd_tx.clone();
    let gui_tx = gui_cmd_tx.clone();
    let slot = Arc::clone(&gui_client_slot);

    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::error!("Error accepting IPC connection: {:?}", e);
                    break;
                }
            };

            let cli_tx_inner = cli_tx.clone();
            let gui_tx_inner = gui_tx.clone();
            let slot_inner = Arc::clone(&slot);

            tokio::spawn(async move {
                let (read_half, mut write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let mut first_line = String::new();

                if reader.read_line(&mut first_line).await.is_err() || first_line.is_empty() {
                    return;
                }

                let trimmed = first_line.trim();

                // Check if CLI invocation
                if let Ok(cmd) = serde_json::from_str::<CliCommand>(trimmed) {
                    let _ = cli_tx_inner.send(cmd);
                    let _ = write_half.write_all(b"ok\n").await;
                    return;
                }

                if trimmed == "show" {
                    let _ = cli_tx_inner.send(CliCommand::Show);
                    let _ = write_half.write_all(b"ok\n").await;
                    return;
                }

                if trimmed == "compose" {
                    let _ = cli_tx_inner.send(CliCommand::Compose);
                    let _ = write_half.write_all(b"ok\n").await;
                    return;
                }

                // Check if GUI child connection
                if let Ok(GuiToSupervisor::Hello) = serde_json::from_str::<GuiToSupervisor>(trimmed) {
                    tracing::info!("GUI child process connected to supervisor");

                    let (sup_to_gui_tx, mut sup_to_gui_rx) = mpsc::unbounded_channel::<SupervisorToGui>();
                    if let Ok(mut guard) = slot_inner.lock() {
                        *guard = Some(sup_to_gui_tx);
                    }

                    // Task to write messages to GUI
                    let write_task = tokio::spawn(async move {
                        while let Some(msg) = sup_to_gui_rx.recv().await {
                            if let Ok(json) = serde_json::to_string(&msg) {
                                if write_half.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    });

                    // Loop reading messages from GUI
                    let mut line = String::new();
                    while let Ok(n) = reader.read_line(&mut line).await {
                        if n == 0 {
                            break;
                        }
                        if let Ok(msg) = serde_json::from_str::<GuiToSupervisor>(line.trim()) {
                            let _ = gui_tx_inner.send(msg);
                        }
                        line.clear();
                    }

                    write_task.abort();
                    if let Ok(mut guard) = slot_inner.lock() {
                        *guard = None;
                    }
                    tracing::info!("GUI child process disconnected from supervisor");
                }
            });
        }
    });

    Outcome::Supervisor(SupervisorSocketGuard { socket_path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_supervisor_cli_interaction() {
        let dir = tempdir().unwrap();
        let sock_path = dir.path().join("test_sup.sock");

        let (cli_tx, mut cli_rx) = mpsc::unbounded_channel();
        let (gui_tx, _) = mpsc::unbounded_channel();
        let slot: GuiClientSlot = Arc::new(Mutex::new(None));

        let outcome = acquire_supervisor(
            sock_path.clone(),
            CliCommand::Show,
            cli_tx,
            gui_tx,
            slot,
        )
        .await;

        let _guard = match outcome {
            Outcome::Supervisor(guard) => {
                assert_eq!(guard.socket_path, sock_path);
                guard
            }
            Outcome::Surfaced => panic!("Expected Supervisor outcome"),
        };

        // Send show command
        let res = send_cli_command(&sock_path, CliCommand::Show).await;
        assert!(res.is_ok());
        let received = cli_rx.recv().await.unwrap();
        assert_eq!(received, CliCommand::Show);

        // Send compose command
        let res = send_cli_command(&sock_path, CliCommand::Compose).await;
        assert!(res.is_ok());
        let received = cli_rx.recv().await.unwrap();
        assert_eq!(received, CliCommand::Compose);
    }

    #[tokio::test]
    async fn test_supervisor_gui_bidirectional_ipc() {
        let dir = tempdir().unwrap();
        let sock_path = dir.path().join("test_gui_ipc.sock");

        let (cli_tx, _) = mpsc::unbounded_channel();
        let (gui_tx, mut gui_rx) = mpsc::unbounded_channel();
        let slot: GuiClientSlot = Arc::new(Mutex::new(None));

        let _guard = acquire_supervisor(
            sock_path.clone(),
            CliCommand::Show,
            cli_tx,
            gui_tx,
            Arc::clone(&slot),
        )
        .await;

        // Connect GUI client
        let (gui_to_sup_tx, mut sup_to_gui_rx) = connect_gui(&sock_path, None).await.unwrap();

        // Wait a moment for slot to be populated
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        {
            let guard = slot.lock().unwrap();
            assert!(guard.is_some(), "GUI client slot should be populated");
        }

        // GUI -> Supervisor
        let account_id = AccountId::new();
        let folder_id = FolderId::new();
        gui_to_sup_tx
            .send(GuiToSupervisor::SyncFolder { account_id, folder_id })
            .unwrap();

        let received = gui_rx.recv().await.unwrap();
        match received {
            GuiToSupervisor::SyncFolder { account_id: acc, folder_id: fol } => {
                assert_eq!(acc, account_id);
                assert_eq!(fol, folder_id);
            }
            other => panic!("Unexpected message: {:?}", other),
        }

        // Supervisor -> GUI
        if let Ok(guard) = slot.lock() {
            if let Some(ref tx) = *guard {
                tx.send(SupervisorToGui::StatusMessage { message: "Hello GUI".into() }).unwrap();
            }
        }

        let from_sup = sup_to_gui_rx.recv().await.unwrap();
        match from_sup {
            SupervisorToGui::StatusMessage { message } => {
                assert_eq!(message, "Hello GUI");
            }
            other => panic!("Unexpected message: {:?}", other),
        }

        // Drop GUI client and verify slot is cleared
        drop(gui_to_sup_tx);
        drop(sup_to_gui_rx);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        {
            let guard = slot.lock().unwrap();
            assert!(guard.is_none(), "GUI client slot should be cleared after disconnect");
        }
    }
}
