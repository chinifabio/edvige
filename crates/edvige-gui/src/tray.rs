//! System Tray Integration for Edvige (StatusNotifierItem via ksni on Linux)

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Compose,
    Quit,
}

#[cfg(target_os = "linux")]
const LOGO_PNG_BYTES: &[u8] = include_bytes!("../../../packaging/edvige.png");

#[cfg(target_os = "linux")]
struct EdvigeTray {
    unread_count: u32,
    icon_data: Vec<u8>,
    icon_size: usize,
    cmd_tx: mpsc::UnboundedSender<TrayCommand>,
}

#[cfg(target_os = "linux")]
impl EdvigeTray {
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
impl ksni::Tray for EdvigeTray {
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
        let _ = self.cmd_tx.send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let cmd_tx = self.cmd_tx.clone();
        let cmd_tx_compose = self.cmd_tx.clone();
        let cmd_tx_quit = self.cmd_tx.clone();

        vec![
            StandardItem {
                label: "✉ Open Edvige Mail".into(),
                activate: Box::new(move |_| {
                    let _ = cmd_tx.send(TrayCommand::Show);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "✏️ Compose New Mail".into(),
                activate: Box::new(move |_| {
                    let _ = cmd_tx_compose.send(TrayCommand::Compose);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Edvige".into(),
                activate: Box::new(move |_| {
                    let _ = cmd_tx_quit.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[derive(Clone)]
pub struct AppTrayHandle {
    #[cfg(target_os = "linux")]
    handle: Arc<Mutex<Option<Arc<ksni::blocking::Handle<EdvigeTray>>>>>,
    latest_count: Arc<AtomicU32>,
}

impl AppTrayHandle {
    #[cfg(target_os = "linux")]
    pub fn spawn(cmd_tx: mpsc::UnboundedSender<TrayCommand>) -> Self {
        use ksni::blocking::TrayMethods;

        let handle: Arc<Mutex<Option<Arc<ksni::blocking::Handle<EdvigeTray>>>>> =
            Arc::new(Mutex::new(None));
        let latest_count = Arc::new(AtomicU32::new(0));

        let handle_clone = Arc::clone(&handle);
        let count_clone = Arc::clone(&latest_count);

        std::thread::spawn(move || {
            let (icon_data, icon_size) = EdvigeTray::generate_icon_pixmap(48);

            // Retry loop: keep trying until StatusNotifierWatcher is available on the desktop bus
            let mut retry_delay = std::time::Duration::from_secs(2);
            loop {
                let initial_count = count_clone.load(Ordering::Relaxed);
                let tray = EdvigeTray {
                    unread_count: initial_count,
                    icon_data: icon_data.clone(),
                    icon_size,
                    cmd_tx: cmd_tx.clone(),
                };

                match tray.spawn() {
                    Ok(h) => {
                        tracing::info!("Application system tray (StatusNotifierItem) initialized successfully");
                        if let Ok(mut guard) = handle_clone.lock() {
                            *guard = Some(Arc::new(h));
                        }
                        break;
                    }
                    Err(e) => {
                        tracing::debug!(
                            "StatusNotifierWatcher not ready yet ({:?}); retrying tray registration in {:?}",
                            e,
                            retry_delay
                        );
                        std::thread::sleep(retry_delay);
                        retry_delay = (retry_delay + std::time::Duration::from_secs(1))
                            .min(std::time::Duration::from_secs(10));
                    }
                }
            }
        });

        Self { handle, latest_count }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn spawn(_cmd_tx: mpsc::UnboundedSender<TrayCommand>) -> Self {
        Self {
            latest_count: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn update_unread_count(&self, count: u32) {
        self.latest_count.store(count, Ordering::Relaxed);
        #[cfg(target_os = "linux")]
        {
            if let Ok(guard) = self.handle.lock() {
                if let Some(ref handle) = *guard {
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
    }
}

