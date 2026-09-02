use std::path::PathBuf;
use std::sync::Arc;
use directories::ProjectDirs;
use edvige_gui::EdvigeApp;
use eframe::NativeOptions;

const LOGO_PNG_BYTES: &[u8] = include_bytes!("../../../packaging/edvige.png");

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

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Edvige Mail")
        .with_inner_size([1100.0, 700.0])
        .with_min_inner_size([800.0, 500.0]);

    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = NativeOptions {
        viewport,
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
