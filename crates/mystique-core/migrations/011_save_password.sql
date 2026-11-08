-- Per-connection "save password". 0: the DB password is never stored; the
-- app asks for it once per session and keeps it in memory. SSH and TLS
-- secrets are not affected.
ALTER TABLE connections ADD COLUMN save_password INTEGER NOT NULL DEFAULT 1 CHECK (
  save_password IN (0, 1)
);
