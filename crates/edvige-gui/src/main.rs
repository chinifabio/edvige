use std::path::PathBuf;
use std::sync::Arc;
use directories::ProjectDirs;
use edvige_gui::EdvigeApp;
use eframe::NativeOptions;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter("edvige_gui=info")
        .init();

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    );

    let socket_path = if let Some(proj_dirs) = ProjectDirs::from("com", "edvige", "edvige") {
        proj_dirs.data_dir().join("edvige.sock")
    } else {
        PathBuf::from("/tmp/edvige.sock")
    };

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Edvige Mail")
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Edvige Mail",
        options,
        Box::new(|_cc| Ok(Box::new(EdvigeApp::new(runtime, socket_path)))),
    )
    .map_err(|e| anyhow::anyhow!("Eframe error: {}", e))?;

    Ok(())
}
