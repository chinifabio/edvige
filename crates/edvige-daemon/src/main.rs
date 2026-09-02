use std::net::SocketAddr;
use std::path::PathBuf;
use clap::Parser;
use directories::ProjectDirs;
use edvige_daemon::{DaemonCoordinator, DaemonServer, EventBroadcaster};
use edvige_storage::{StorageConfig, StorageEngine};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "edvige-daemon", about = "Edvige Email Client Background Daemon", version)]
struct Args {
    /// Path to Unix Domain Socket
    #[arg(short, long)]
    socket: Option<PathBuf>,

    /// TCP bind address (e.g. 127.0.0.1:50051)
    #[arg(short, long)]
    tcp: Option<SocketAddr>,

    /// SQLite database file path
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Blob storage directory path
    #[arg(long)]
    blob_dir: Option<PathBuf>,

    /// Log level filter (e.g. info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&args.log_level)),
        )
        .init();

    tracing::info!("Starting Edvige Email Daemon...");

    // 1. Resolve storage configuration
    let default_config = StorageConfig::default_user_dirs().unwrap_or_else(|_| {
        let fallback_dir = PathBuf::from(".edvige");
        StorageConfig::new(fallback_dir.join("edvige.db"), fallback_dir.join("blobs"))
    });

    let config = StorageConfig::new(
        args.db_path.unwrap_or(default_config.db_path),
        args.blob_dir.unwrap_or(default_config.blob_dir),
    );

    tracing::info!("Using database at {}", config.db_path.display());
    tracing::info!("Using blob store at {}", config.blob_dir.display());

    let storage = StorageEngine::open(config).await?;
    let events = EventBroadcaster::new();
    let coordinator = DaemonCoordinator::new(storage.clone(), events);

    // 2. Start background account sync workers
    coordinator.start().await?;

    // 3. Resolve socket path
    let socket_path = args.socket.unwrap_or_else(|| {
        if let Some(proj_dirs) = ProjectDirs::from("com", "edvige", "edvige") {
            proj_dirs.data_dir().join("edvige.sock")
        } else {
            PathBuf::from("/tmp/edvige.sock")
        }
    });

    let server = DaemonServer::new(coordinator.clone());

    // 4. Setup system tray integration
    let (tray_shutdown_tx, mut tray_shutdown_rx) = tokio::sync::watch::channel(false);
    let tray_handle = edvige_daemon::DaemonTrayHandle::spawn(tray_shutdown_tx);

    let storage_for_tray = storage.clone();
    let tray_clone = tray_handle.clone();
    tokio::spawn(async move {
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

    // 5. Setup graceful shutdown handler (Ctrl+C or Tray Quit)
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let coord_clone = coordinator.clone();

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C signal. Stopping daemon...");
            }
            _ = tray_shutdown_rx.changed() => {
                tracing::info!("Received Quit signal from system tray. Stopping daemon...");
            }
        }
        coord_clone.shutdown().await;
        let _ = shutdown_tx.send(());
    });

    let shutdown_signal = async {
        let _ = shutdown_rx.await;
    };

    // 5. Start gRPC server
    if let Some(tcp_addr) = args.tcp {
        server.serve_tcp(tcp_addr, shutdown_signal).await?;
    } else {
        server.serve_uds(&socket_path, shutdown_signal).await?;
    }

    tracing::info!("Edvige daemon stopped cleanly.");
    Ok(())
}

