use edvige_core::MessageFlags;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ok,
    No,
    Bad,
    PreAuth,
    Bye,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedResponse {
    pub tag: String,
    pub status: Status,
    pub code: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FetchResponse {
    pub seq: u32,
    pub uid: Option<u32>,
    pub flags: Option<MessageFlags>,
    pub rfc822_size: Option<u64>,
    pub rfc822_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UntaggedResponse {
    /// * LIST (\HasNoChildren \Drafts) "/" "Drafts"
    List {
        flags: Vec<String>,
        delimiter: Option<String>,
        name: String,
    },
    /// * 23 EXISTS
    Exists(u32),
    /// * 2 RECENT
    Recent(u32),
    /// * 5 EXPUNGE
    Expunge(u32),
    /// * 1 FETCH (...)
    Fetch(FetchResponse),
    /// * STATUS "INBOX" (MESSAGES 123 UIDNEXT 456 UIDVALIDITY 789)
    Status {
        name: String,
        messages: Option<u32>,
        recent: Option<u32>,
        uid_next: Option<u32>,
        uid_validity: Option<u32>,
        unseen: Option<u32>,
    },
    /// * OK [UIDVALIDITY 12345] UIDs valid
    Ok {
        code: Option<String>,
        text: String,
    },
    /// * NO ...
    No {
        code: Option<String>,
        text: String,
    },
    /// * BAD ...
    Bad {
        code: Option<String>,
        text: String,
    },
    /// * PREAUTH / * BYE / Other untagged line
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum ImapLine {
    Tagged(TaggedResponse),
    Untagged(UntaggedResponse),
    Continuation(Continuation),
}
