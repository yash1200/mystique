-- Secrets vault. One AES-256-GCM envelope per secret, sealed with the vault
-- key (one keychain item; debug builds cache it in a 0600 file). The AAD binds each
-- envelope to its install, owner and kind.
-- owner_kind: 'connection' (owner_id = connection id) or 'app' (owner_id 'app').
-- kind: password | ssh | tls | ai:<provider> | mcp_token.

CREATE TABLE IF NOT EXISTS secrets (
  owner_kind  TEXT    NOT NULL,
  owner_id    TEXT    NOT NULL,
  kind        TEXT    NOT NULL,
  envelope    TEXT    NOT NULL,
  key_version INTEGER NOT NULL,
  updated_at  TEXT    NOT NULL,
  PRIMARY KEY (owner_kind, owner_id, kind)
);
