//! System Tray Integration for edvige-daemon (StatusNotifierItem via ksni on Linux)

use std::sync::Arc;
use tokio::sync::watch;

#[cfg(target_os = "linux")]
const LOGO_PNG_BYTES: &[u8] = include_bytes!("../../../packaging/edvige.png");

#[cfg(target_os = "linux")]
struct EdvigeDaemonTray {
    unread_count: u32,
    icon_data: Vec<u8>,
    icon_size: usize,
    shutdown_tx: watch::Sender<bool>,
}

#[cfg(target_os = "linux")]
impl EdvigeDaemonTray {
    fn launch_gui(args: &[&str]) {
        let bin_candidates = [
            format!("{}/.local/bin/edvige", std::env::var("HOME").unwrap_or_default()),
            format!("{}/.cargo/bin/edvige-gui", std::env::var("HOME").unwrap_or_default()),
            "edvige".to_string(),
        ];

        for bin in &bin_candidates {
            if let Ok(mut child) = std::process::Command::new(bin).args(args).spawn() {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return;
            }
        }

        tracing::warn!("Could not launch edvige GUI: binary not found in ~/.local/bin or PATH");
    }

    fn generate_icon_pixmap(size: usize) -> (Vec<u8>, usize) {
        if let Ok(img) = image::load_from_memory(LOGO_PNG_BYTES) {
            let resized = img.resize_exact(
                size as u32,
                size as u32,
                image::imageops::FilterType::Lanczos3,
            );
            let rgba = resized.to_rgba8();
            let raw = rgba.into_raw();
            let mut data = Vec::with_capacity(raw.len());
            // ksni expects ARGB32 in network byte order: [A, R, G, B]
            for chunk in raw.chunks_exact(4) {
                let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
                data.extend_from_slice(&[a, r, g, b]);
            }
            (data, size)
        } else {
            (vec![0, 0, 0, 0], 1)
        }
    }
}

#[cfg(target_os = "linux")]
impl ksni::Tray for EdvigeDaemonTray {
    fn id(&self) -> String {
        "edvige-mail".into()
    }

    fn title(&self) -> String {
        if self.unread_count > 0 {
            format!("Edvige Mail ({})", self.unread_count)
        } else {
            "Edvige Mail".into()
        }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let desc = if self.unread_count > 0 {
            format!("{} unread email(s)", self.unread_count)
        } else {
            "All mail read - Background sync active".into()
        };
        ksni::ToolTip {
            title: "Edvige Mail".into(),
            description: desc,
            icon_name: "mail-unread".into(),
            icon_pixmap: Vec::new(),
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![ksni::Icon {
            width: self.icon_size as i32,
            height: self.icon_size as i32,
            data: self.icon_data.clone(),
        }]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        Self::launch_gui(&[]);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "✉ Open Edvige Mail".into(),
                activate: Box::new(|_| Self::launch_gui(&[])),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "✏️ Compose New Mail".into(),
                activate: Box::new(|_| Self::launch_gui(&["--compose"])),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Daemon".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.shutdown_tx.send(true);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[derive(Clone)]
pub struct DaemonTrayHandle {
    #[cfg(target_os = "linux")]
    handle: Option<Arc<ksni::blocking::Handle<EdvigeDaemonTray>>>,
}

impl DaemonTrayHandle {
    #[cfg(target_os = "linux")]
    pub fn spawn(shutdown_tx: watch::Sender<bool>) -> Self {
        use ksni::blocking::TrayMethods;

        let (icon_data, icon_size) = EdvigeDaemonTray::generate_icon_pixmap(48);
        let tray = EdvigeDaemonTray {
            unread_count: 0,
            icon_data,
            icon_size,
            shutdown_tx,
        };

        // Spawn ksni on a dedicated OS thread outside Tokio's runtime to avoid nested runtime panic
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = tray.spawn();
            let _ = tx.send(res);
        });

        let handle = rx.recv().ok().and_then(|r| match r {
            Ok(h) => {
                tracing::info!("Daemon system tray (StatusNotifierItem) initialized");
                Some(Arc::new(h))
            }
            Err(e) => {
                tracing::info!("System tray not available on host: {:?}", e);
                None
            }
        });

        Self { handle }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn spawn(_shutdown_tx: watch::Sender<bool>) -> Self {
        Self {}
    }

    pub fn update_unread_count(&self, count: u32) {
        #[cfg(target_os = "linux")]
        if let Some(ref handle) = self.handle {
            let handle = Arc::clone(handle);
            std::thread::spawn(move || {
                handle.update(move |tray| {
                    if tray.unread_count != count {
                        tray.unread_count = count;
                    }
                });
            });
        }
    }
}
