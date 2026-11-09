use mystique_connector_api::{
    confirmation_for, confirmation_unlocks, ConnectorError, ConnectorResult, Environment,
    WriteAction, WriteGuardPrompt, WriteRisk, WriteState,
};

use super::principal::{is_tool_session_id, TOOL_WRITE_MESSAGE};
use super::{ConnectionManager, SessionOrigin};

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
    /// The query text as sent, for an agent or MCP authorization (F-01).
    pub request: Option<&'a str>,
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
            request: None,
        }
    }
}

impl ConnectionManager {
    pub(super) async fn enforce_write_guard(&self, check: WriteCheck<'_>) -> ConnectorResult<()> {
        match self.write_prompt(check).await? {
            Some(prompt) => Err(ConnectorError::WriteGuard(Box::new(prompt))),
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
        let (environment, read_only, connection_name, tool) = {
            let guard = self.sessions.read().await;
            let live = guard.get(check.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", check.session_id))
            })?;
            (
                live.environment.clone(),
                live.read_only || check.profile_read_only,
                live.connection_name.clone(),
                live.origin != SessionOrigin::Ui || is_tool_session_id(check.session_id),
            )
        };
        if tool {
            return self.tool_write_check(&check, read_only);
        }
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

    /// Agent and MCP (F-01): no unlock, no pin, no dialog. Read-only still
    /// wins; otherwise the write needs `AgentTools`' one-shot authorization
    /// for exactly this text.
    fn tool_write_check(
        &self,
        check: &WriteCheck<'_>,
        read_only: bool,
    ) -> ConnectorResult<Option<WriteGuardPrompt>> {
        if read_only {
            return Err(ConnectorError::Policy(
                mystique_connector_api::READ_ONLY_MESSAGE.into(),
            ));
        }
        match check.request {
            Some(text) if self.take_tool_write(check.session_id, text) => Ok(None),
            _ => Err(ConnectorError::Policy(TOOL_WRITE_MESSAGE.into())),
        }
    }
}
