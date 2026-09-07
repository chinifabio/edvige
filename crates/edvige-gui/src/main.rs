use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use clap::Parser;
use directories::ProjectDirs;
use edvige_gui::app::EdvigeApp;
use edvige_gui::engine::{AppCoordinator, AppEvent, EventBroadcaster};
use edvige_gui::ipc::{
    acquire_supervisor, connect_gui, CliCommand, GuiClientSlot, GuiToSupervisor, Outcome,
    SupervisorToGui,
};
use edvige_gui::notifier::DesktopNotifier;
use edvige_gui::state::AppState;
use edvige_gui::tray::{AppTrayHandle, TrayCommand};
use edvige_storage::{StorageConfig, StorageEngine};
use eframe::NativeOptions;
use tokio::sync::mpsc;

const LOGO_PNG_BYTES: &[u8] = include_bytes!("../../../packaging/edvige.png");

#[derive(Parser, Debug)]
#[command(name = "edvige", version = "0.1.0", about = "Edvige Email Client")]
struct Cli {
    /// Internal flag: run the GUI window process
    #[arg(long, hide = true)]
    gui: bool,

    /// Start minimized to system tray without showing main window
    #[arg(long)]
    hidden: bool,

    /// Open directly to new message composer
    #[arg(long)]
    compose: bool,
}

fn load_app_icon() -> Option<egui::IconData> {
    if let Ok(img) = image::load_from_memory(LOGO_PNG_BYTES) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Some(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        })
    } else {
        None
    }
}

fn get_paths() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    if let Some(proj_dirs) = ProjectDirs::from("com", "edvige", "edvige") {
        let d = proj_dirs.data_dir().to_path_buf();
        (d.clone(), d.join("edvige.db"), d.join("blobs"), d.join("edvige.sock"))
    } else {
        let d = PathBuf::from("/tmp/edvige");
        (d.clone(), d.join("edvige.db"), d.join("blobs"), d.join("edvige.sock"))
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter("edvige=info,edvige_gui=info")
        .init();

    let cli = Cli::parse();
    let (data_dir, db_path, blob_path, socket_path) = get_paths();
    std::fs::create_dir_all(&data_dir)?;

    if cli.gui {
        run_gui(cli.compose, db_path, blob_path, socket_path)
    } else {
        run_supervisor(cli.hidden, cli.compose, db_path, blob_path, socket_path)
    }
}

/// GUI Window Child Process
fn run_gui(
    compose: bool,
    db_path: PathBuf,
    blob_path: PathBuf,
    socket_path: PathBuf,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _runtime_guard = runtime.enter();

    let egui_ctx_slot: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
    let waker_slot = Arc::clone(&egui_ctx_slot);
    let waker: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Ok(guard) = waker_slot.lock() {
            if let Some(ref ctx) = *guard {
                ctx.request_repaint();
            }
        }
    });

    // 1. Connect to supervisor over IPC
    let (gui_tx, sup_rx) = match runtime.block_on(connect_gui(&socket_path, Some(waker))) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::warn!("Could not connect to supervisor socket ({:?}); running in standalone GUI mode.", e);
            let (dummy_tx, _) = mpsc::unbounded_channel();
            let (_, dummy_rx) = mpsc::unbounded_channel();
            (dummy_tx, dummy_rx)
        }
    };

    // 2. Open shared storage directly
    let storage = runtime.block_on(async {
        StorageEngine::open(StorageConfig::new(&db_path, &blob_path)).await
    })?;

    // 3. Setup initial state
    let mut app_state = AppState::default();
    if compose {
        app_state.show_compose = true;
    }

    // 4. Configure window
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Edvige Mail")
        .with_inner_size([1100.0, 700.0])
        .with_min_inner_size([800.0, 500.0]);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = NativeOptions {
        viewport,
        run_and_return: false, // Clean process exit on close for 100% Wayland surface destruction
        ..Default::default()
    };

    let ctx_slot_init = Arc::clone(&egui_ctx_slot);
    let _ = eframe::run_native(
        "Edvige Mail",
        options,
        Box::new(move |cc| {
            if let Ok(mut guard) = ctx_slot_init.lock() {
                *guard = Some(cc.egui_ctx.clone());
            }
            let app = EdvigeApp::new(app_state, storage, gui_tx, sup_rx);
            Ok(Box::new(app))
        }),
    );

    tracing::info!("GUI window closed; terminating GUI process");
    std::process::exit(0);
}

/// Supervisor Process (Tray + Background Sync Engine + GUI Process Manager)
fn run_supervisor(
    hidden: bool,
    compose: bool,
    db_path: PathBuf,
    blob_path: PathBuf,
    socket_path: PathBuf,
) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _runtime_guard = runtime.enter();

    // 1. Single-instance check and socket listener
    let (cli_cmd_tx, mut cli_cmd_rx) = mpsc::unbounded_channel::<CliCommand>();
    let (gui_cmd_tx, mut gui_cmd_rx) = mpsc::unbounded_channel::<GuiToSupervisor>();
    let gui_client_slot: GuiClientSlot = Arc::new(Mutex::new(None));

    let initial_cmd = if compose {
        CliCommand::Compose
    } else {
        CliCommand::Show
    };

    let instance_guard = runtime.block_on(async {
        acquire_supervisor(
            socket_path,
            initial_cmd,
            cli_cmd_tx,
            gui_cmd_tx,
            Arc::clone(&gui_client_slot),
        )
        .await
    });

    let _guard = match instance_guard {
        Outcome::Supervisor(guard) => guard,
        Outcome::Surfaced => {
            tracing::info!("Edvige is already running; requested existing instance to surface.");
            return Ok(());
        }
    };

    // 2. Open local storage and start background engine
    let (storage, events, coordinator) = runtime.block_on(async {
        let storage = StorageEngine::open(StorageConfig::new(&db_path, &blob_path)).await?;
        let events = EventBroadcaster::new();
        let coordinator = AppCoordinator::new(storage.clone(), events.clone());
        coordinator.start().await?;
        Ok::<_, anyhow::Error>((storage, events, coordinator))
    })?;

    // 3. Setup system tray
    let (tray_cmd_tx, mut tray_cmd_rx) = mpsc::unbounded_channel();
    let tray_handle = AppTrayHandle::spawn(tray_cmd_tx);

    let storage_for_tray = storage.clone();
    let tray_clone = tray_handle.clone();
    runtime.spawn(async move {
        loop {
            let mut total_unread = 0u32;
            if let Ok(accounts) = storage_for_tray.list_accounts().await {
                for acc in accounts {
                    if let Ok(folders) = storage_for_tray.list_folders_for_account(acc.id).await {
                        for f in folders {
                            total_unread += f.unread_count;
                        }
                    }
                }
            }
            tray_clone.update_unread_count(total_unread);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });

    // 4. Forward background engine events to connected GUI child process
    let mut event_rx = events.subscribe();
    let slot_for_events = Arc::clone(&gui_client_slot);
    runtime.spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            if let Ok(guard) = slot_for_events.lock() {
                if let Some(ref tx) = *guard {
                    match event {
                        AppEvent::FolderUpdated {
                            account_id,
                            folder_id,
                            total_count,
                            unread_count,
                        } => {
                            let _ = tx.send(SupervisorToGui::FolderUpdated {
                                account_id,
                                folder_id,
                                total_count,
                                unread_count,
                            });
                        }
                        AppEvent::NewMessages {
                            account_id,
                            folder_id,
                            count,
                        } => {
                            let _ = tx.send(SupervisorToGui::NewMessages {
                                account_id,
                                folder_id,
                                count,
                            });
                        }
                        AppEvent::FlagsChanged {
                            folder_id,
                            message_id,
                            flags,
                        } => {
                            let _ = tx.send(SupervisorToGui::FlagsChanged {
                                folder_id,
                                message_id,
                                flags,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    // 5. Handle requests from GUI child process
    let coord_for_gui = coordinator.clone();
    let slot_for_gui = Arc::clone(&gui_client_slot);
    runtime.spawn(async move {
        while let Some(msg) = gui_cmd_rx.recv().await {
            match msg {
                GuiToSupervisor::Hello => {}
                GuiToSupervisor::SyncAccountFolders { account_id } => {
                    let c = coord_for_gui.clone();
                    tokio::spawn(async move {
                        let _ = c.sync_account_folders(account_id).await;
                    });
                }
                GuiToSupervisor::SyncFolder { account_id, folder_id } => {
                    let c = coord_for_gui.clone();
                    let slot = Arc::clone(&slot_for_gui);
                    tokio::spawn(async move {
                        match c.sync_folder_messages(account_id, folder_id).await {
                            Ok(stats) => {
                                if let Ok(guard) = slot.lock() {
                                    if let Some(ref tx) = *guard {
                                        let _ = tx.send(SupervisorToGui::StatusMessage {
                                            message: format!("Synced {} messages", stats.messages_fetched),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                if let Ok(guard) = slot.lock() {
                                    if let Some(ref tx) = *guard {
                                        let _ = tx.send(SupervisorToGui::StatusMessage {
                                            message: format!("Sync error: {}", e),
                                        });
                                    }
                                }
                            }
                        }
                    });
                }
                GuiToSupervisor::DispatchOutbox { account_id } => {
                    let c = coord_for_gui.clone();
                    tokio::spawn(async move {
                        let _ = c.dispatch_outbox(account_id).await;
                    });
                }
                GuiToSupervisor::AccountAdded { account_id } => {
                    let c = coord_for_gui.clone();
                    tokio::spawn(async move {
                        c.start_account_worker(account_id).await;
                        let _ = c.sync_account_folders(account_id).await;
                    });
                }
            }
        }
    });

    // 6. Child Process Management & Surface Control
    let gui_child: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
    let spawn_or_focus = {
        let slot = Arc::clone(&gui_client_slot);
        let child_holder = Arc::clone(&gui_child);
        Arc::new(move |compose: bool| {
            // Check if GUI is already connected
            if let Ok(guard) = slot.lock() {
                if let Some(ref tx) = *guard {
                    let cmd = if compose {
                        SupervisorToGui::Compose
                    } else {
                        SupervisorToGui::Focus
                    };
                    let _ = tx.send(cmd);
                    return;
                }
            }

            // If GUI is not connected, reap any stale child process and spawn a new one
            if let Ok(mut guard) = child_holder.lock() {
                if let Some(mut child) = guard.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }

                if let Ok(exe) = std::env::current_exe() {
                    let mut cmd = std::process::Command::new(exe);
                    cmd.arg("--gui");
                    if compose {
                        cmd.arg("--compose");
                    }
                    match cmd.spawn() {
                        Ok(c) => {
                            let pid = c.id();
                            *guard = Some(c);
                            tracing::info!("Spawned Edvige GUI child process (PID {})", pid);
                        }
                        Err(e) => {
                            tracing::error!("Failed to spawn Edvige GUI: {:?}", e);
                        }
                    }
                }
            }
        })
    };

    // Desktop notification click callback
    let notif_open = Arc::clone(&spawn_or_focus);
    DesktopNotifier::set_open_callback(move || {
        notif_open(false);
    });

    // Initial launch if not started hidden
    if !hidden {
        spawn_or_focus(compose);
    }

    // 7. Supervisor Event Loop
    let mut should_quit = false;
    runtime.block_on(async {
        loop {
            tokio::select! {
                Some(cli_cmd) = cli_cmd_rx.recv() => {
                    match cli_cmd {
                        CliCommand::Show => spawn_or_focus(false),
                        CliCommand::Compose => spawn_or_focus(true),
                    }
                }
                Some(tray_cmd) = tray_cmd_rx.recv() => {
                    match tray_cmd {
                        TrayCommand::Show => spawn_or_focus(false),
                        TrayCommand::Compose => spawn_or_focus(true),
                        TrayCommand::Quit => {
                            should_quit = true;
                            break;
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    should_quit = true;
                    break;
                }
            }
        }
    });

    if should_quit {
        tracing::info!("Supervisor shutting down; cleaning up GUI child process and services");
        if let Ok(mut guard) = gui_child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        runtime.block_on(async {
            coordinator.shutdown().await;
        });
    }

    tracing::info!("Edvige Mail supervisor exited cleanly.");
    std::process::exit(0);
}
