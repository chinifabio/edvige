-- 0002_outbox.sql
-- Outbox and message draft storage for Edvige

CREATE TABLE IF NOT EXISTS outbox_messages (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    from_json TEXT NOT NULL,
    to_json TEXT NOT NULL,
    cc_json TEXT NOT NULL DEFAULT '[]',
    bcc_json TEXT NOT NULL DEFAULT '[]',
    subject TEXT NOT NULL DEFAULT '',
    body_text TEXT,
    body_html TEXT,
    in_reply_to TEXT,
    references_header TEXT,
    attachments_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'draft',
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sent_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_outbox_status ON outbox_messages(status, account_id);
CREATE INDEX IF NOT EXISTS idx_outbox_account ON outbox_messages(account_id, updated_at DESC);

