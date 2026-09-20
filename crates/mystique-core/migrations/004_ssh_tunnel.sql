-- SSH as shared transport on the connection profile (not plugin_config).
ALTER TABLE connections ADD COLUMN ssh_config TEXT NOT NULL DEFAULT '{}';
ALTER TABLE connections ADD COLUMN ssh_secret_ref TEXT;

CREATE TABLE IF NOT EXISTS ssh_known_hosts (
  host         TEXT NOT NULL,
  port         INTEGER NOT NULL,
  key_type     TEXT NOT NULL,
  fingerprint  TEXT NOT NULL,
  public_key   TEXT NOT NULL,
  accepted_at  TEXT NOT NULL,
  PRIMARY KEY (host, port)
);
