use std::sync::{Arc, OnceLock};

use uuid::Uuid;

pub(crate) fn ensure_token(existing: Option<&str>) -> String {
    let t = existing.map(str::trim).unwrap_or("");
    if t.len() >= 16 {
        t.to_string()
    } else {
        Uuid::new_v4().simple().to_string()
    }
}

type Loader = dyn Fn() -> Result<String, String> + Send + Sync;

/// The bearer token. The server can start at launch before it is known: the
/// first request loads it from the vault (never prompting), then it is kept.
#[derive(Clone)]
pub(crate) struct TokenGate {
    known: Arc<OnceLock<String>>,
    load: Arc<Loader>,
}

impl TokenGate {
    pub fn known(token: String) -> Self {
        let known = OnceLock::new();
        let _ = known.set(token);
        Self {
            known: Arc::new(known),
            load: Arc::new(|| Err("MCP token not loaded".into())),
        }
    }

    pub fn lazy(load: impl Fn() -> Result<String, String> + Send + Sync + 'static) -> Self {
        Self {
            known: Arc::new(OnceLock::new()),
            load: Arc::new(load),
        }
    }

    pub fn peek(&self) -> Option<String> {
        self.known.get().cloned()
    }

    pub async fn get(&self) -> Result<String, String> {
        if let Some(token) = self.known.get() {
            return Ok(token.clone());
        }
        let load = Arc::clone(&self.load);
        let token = tokio::task::spawn_blocking(move || load())
            .await
            .map_err(|e| format!("MCP token: {e}"))??;
        Ok(self.known.get_or_init(|| token).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn a_lazy_token_loads_once_and_a_failure_is_retried() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let gate = TokenGate::lazy(move || match seen.fetch_add(1, Ordering::SeqCst) {
            0 => Err("locked".into()),
            _ => Ok("t".repeat(32)),
        });
        assert_eq!(gate.peek(), None);
        assert_eq!(gate.get().await.unwrap_err(), "locked");
        assert_eq!(gate.get().await.unwrap(), "t".repeat(32));
        assert_eq!(gate.get().await.unwrap(), "t".repeat(32));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(gate.peek(), Some("t".repeat(32)));
    }
}
