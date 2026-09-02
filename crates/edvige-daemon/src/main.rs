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
    let coordinator = DaemonCoordinator::new(storage, events);

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

    // 4. Setup graceful shutdown handler
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let coord_clone = coordinator.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl+c");
        tracing::info!("Received shutdown signal. Stopping daemon...");
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

