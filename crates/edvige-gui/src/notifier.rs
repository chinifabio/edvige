use std::sync::OnceLock;
#[cfg(target_os = "linux")]
use notify_rust::{Hint, Notification};

static NOTIFIER_OPEN_CALLBACK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

pub struct DesktopNotifier;

impl DesktopNotifier {
    pub fn set_open_callback<F>(cb: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        let _ = NOTIFIER_OPEN_CALLBACK.set(Box::new(cb));
    }

    pub fn notify_new_mail(
        account_email: &str,
        folder_name: &str,
        count: u32,
        latest_subject: Option<&str>,
    ) {
        let summary = if count == 1 {
            format!("New email in {}", folder_name)
        } else {
            format!("{} new emails in {}", count, folder_name)
        };

        let body = match latest_subject {
            Some(subj) if !subj.is_empty() => {
                format!("{}: {}", account_email, subj)
            }
            _ => format!("Received for {}", account_email),
        };

        #[cfg(target_os = "linux")]
        tokio::task::spawn_blocking(move || {
            let res = Notification::new()
                .appname("Edvige Mail")
                .summary(&summary)
                .body(&body)
                .icon("edvige")
                .hint(Hint::DesktopEntry("edvige".to_string()))
                .action("default", "Open")
                .show();

            match res {
                Ok(handle) => {
                    handle.wait_for_action(move |action| {
                        if action == "default" {
                            tracing::info!("Desktop notification clicked; requesting UI surface");
                            if let Some(cb) = NOTIFIER_OPEN_CALLBACK.get() {
                                cb();
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::debug!("Failed to send desktop notification: {:?}", e);
                }
            }
        });
    }

    pub fn notify_send_failed(subject: &str, error: &str) {
        let summary = "Failed to send email";
        let body = format!("Subject: {}\nError: {}", subject, error);

        #[cfg(target_os = "linux")]
        tokio::task::spawn_blocking(move || {
            let res = Notification::new()
                .appname("Edvige Mail")
                .summary(summary)
                .body(&body)
                .icon("dialog-error")
                .hint(Hint::DesktopEntry("edvige".to_string()))
                .action("default", "Open")
                .show();

            match res {
                Ok(handle) => {
                    handle.wait_for_action(move |action| {
                        if action == "default" {
                            tracing::info!("Error notification clicked; requesting UI surface");
                            if let Some(cb) = NOTIFIER_OPEN_CALLBACK.get() {
                                cb();
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::debug!("Failed to send desktop error notification: {:?}", e);
                }
            }
        });
    }
}

