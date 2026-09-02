use edvige_core::MessageFlags;

#[derive(Debug, Clone)]
pub enum ImapCommand {
    Capability,
    Login {
        username: String,
        password: String,
    },
    Logout,
    List {
        reference: String,
        wildcard: String,
    },
    Select {
        mailbox: String,
    },
    Examine {
        mailbox: String,
    },
    UidSearch {
        query: String,
    },
    UidFetch {
        uid_range: String,
        items: String,
    },
    UidStoreFlags {
        uid: u32,
        add: bool,
        flags: MessageFlags,
    },
    UidCopy {
        uid: u32,
        target_mailbox: String,
    },
    UidMove {
        uid: u32,
        target_mailbox: String,
    },
    Idle,
    Noop,
}

impl ImapCommand {
    pub fn serialize(&self, tag: &str) -> String {
        match self {
            ImapCommand::Capability => format!("{} CAPABILITY\r\n", tag),
            ImapCommand::Login { username, password } => {
                format!(
                    "{} LOGIN \"{}\" \"{}\"\r\n",
                    tag,
                    escape_quote(username),
                    escape_quote(password)
                )
            }
            ImapCommand::Logout => format!("{} LOGOUT\r\n", tag),
            ImapCommand::List { reference, wildcard } => {
                format!(
                    "{} LIST \"{}\" \"{}\"\r\n",
                    tag,
                    escape_quote(reference),
                    escape_quote(wildcard)
                )
            }
            ImapCommand::Select { mailbox } => {
                format!("{} SELECT \"{}\"\r\n", tag, escape_quote(mailbox))
            }
            ImapCommand::Examine { mailbox } => {
                format!("{} EXAMINE \"{}\"\r\n", tag, escape_quote(mailbox))
            }
            ImapCommand::UidSearch { query } => {
                format!("{} UID SEARCH {}\r\n", tag, query)
            }
            ImapCommand::UidFetch { uid_range, items } => {
                format!("{} UID FETCH {} ({})\r\n", tag, uid_range, items)
            }
            ImapCommand::UidStoreFlags { uid, add, flags } => {
                let op = if *add { "+FLAGS" } else { "-FLAGS" };
                let flag_str = flags_to_imap_string(flags);
                format!("{} UID STORE {} {} ({})\r\n", tag, uid, op, flag_str)
            }
            ImapCommand::UidCopy { uid, target_mailbox } => {
                format!("{} UID COPY {} \"{}\"\r\n", tag, uid, escape_quote(target_mailbox))
            }
            ImapCommand::UidMove { uid, target_mailbox } => {
                format!("{} UID MOVE {} \"{}\"\r\n", tag, uid, escape_quote(target_mailbox))
            }
            ImapCommand::Idle => format!("{} IDLE\r\n", tag),
            ImapCommand::Noop => format!("{} NOOP\r\n", tag),
        }
    }
}

fn escape_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn flags_to_imap_string(flags: &MessageFlags) -> String {
    let mut parts = Vec::new();
    if flags.seen {
        parts.push("\\Seen");
    }
    if flags.flagged {
        parts.push("\\Flagged");
    }
    if flags.answered {
        parts.push("\\Answered");
    }
    if flags.draft {
        parts.push("\\Draft");
    }
    if flags.deleted {
        parts.push("\\Deleted");
    }
    parts.join(" ")
}
