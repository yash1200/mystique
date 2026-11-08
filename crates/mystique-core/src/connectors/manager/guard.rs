use mystique_connector_api::{
    confirmation_for, confirmation_unlocks, ConnectorError, ConnectorResult, Environment,
    WriteAction, WriteGuardPrompt, WriteRisk, WriteState,
};

use super::ConnectionManager;

/// One write about to run, as the policy sees it.
pub(super) struct WriteCheck<'a> {
    pub session_id: &'a str,
    pub action: WriteAction,
    pub risk: WriteRisk,
    pub preview: String,
    pub statements: Vec<String>,
    /// Name the user types (or the dialog sends back) to confirm a destructive write.
    pub object_name: Option<&'a str>,
    pub confirm_object_name: Option<&'a str>,
    pub profile_read_only: bool,
    /// Manual-commit tab.
    pub pinned: bool,
}

impl<'a> WriteCheck<'a> {
    pub fn new(session_id: &'a str, action: WriteAction, risk: WriteRisk) -> Self {
        Self {
            session_id,
            action,
            risk,
            preview: String::new(),
            statements: Vec::new(),
            object_name: None,
            confirm_object_name: None,
            profile_read_only: false,
            pinned: false,
        }
    }
}

impl ConnectionManager {
    pub(super) async fn enforce_write_guard(&self, check: WriteCheck<'_>) -> ConnectorResult<()> {
        match self.write_prompt(check).await? {
            Some(prompt) => Err(ConnectorError::WriteGuard(prompt)),
            None => Ok(()),
        }
    }

    /// The prompt to show, or `None` when the write may run.
    pub(super) async fn write_prompt(
        &self,
        check: WriteCheck<'_>,
    ) -> ConnectorResult<Option<WriteGuardPrompt>> {
        if check.risk == WriteRisk::Read {
            return Ok(None);
        }
        let (environment, read_only, connection_name) = {
            let guard = self.sessions.read().await;
            let live = guard.get(check.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", check.session_id))
            })?;
            (
                live.environment.clone(),
                live.read_only || check.profile_read_only,
                live.connection_name.clone(),
            )
        };
        let env = Environment::parse(&environment);
        let expected = check
            .object_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("DROP");
        let typed = check.confirm_object_name.map(str::trim).unwrap_or("");
        let state = WriteState {
            read_only,
            unlocked: self.write_unlocks.lock().await.contains(check.session_id),
            pinned: check.pinned,
            confirmed: typed.eq_ignore_ascii_case(expected),
        };
        let ask = confirmation_for(
            env,
            check.action,
            check.risk,
            state,
            expected,
            check.statements,
        )?;
        let prompt = WriteGuardPrompt::from_confirmation(
            ask,
            environment,
            connection_name,
            check.preview,
            expected,
        );
        if prompt.is_none() && confirmation_unlocks(env, check.risk, state) {
            self.write_unlocks
                .lock()
                .await
                .insert(check.session_id.to_string());
        }
        Ok(prompt)
    }
}
