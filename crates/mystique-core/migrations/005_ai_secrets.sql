-- Encrypted AI provider API keys. Ciphertext only — never plaintext.
-- Not listed by settings IPC.

CREATE TABLE IF NOT EXISTS ai_secrets (
  provider    TEXT PRIMARY KEY,
  ciphertext  TEXT NOT NULL,
  updated_at  TEXT NOT NULL
);
