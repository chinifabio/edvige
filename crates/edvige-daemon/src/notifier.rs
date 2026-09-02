use notify_rust::Notification;

pub struct DesktopNotifier;

impl DesktopNotifier {
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

        tokio::task::spawn_blocking(move || {
            let res = Notification::new()
                .appname("Edvige Mail")
                .summary(&summary)
                .body(&body)
                .icon("mail-unread")
                .show();

            if let Err(e) = res {
                tracing::debug!("Failed to send desktop notification: {:?}", e);
            }
        });
    }

    pub fn notify_send_failed(subject: &str, error: &str) {
        let summary = "Failed to send email";
        let body = format!("Subject: {}\nError: {}", subject, error);

        tokio::task::spawn_blocking(move || {
            let res = Notification::new()
                .appname("Edvige Mail")
                .summary(summary)
                .body(&body)
                .icon("dialog-error")
                .show();

            if let Err(e) = res {
                tracing::debug!("Failed to send desktop error notification: {:?}", e);
            }
        });
    }
}

