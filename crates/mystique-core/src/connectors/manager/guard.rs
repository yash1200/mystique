use mystique_connector_api::{
    environment_needs_type_confirm, environment_needs_unlock, ConnectorError, ConnectorResult,
    WriteGuardPrompt, WriteRisk,
};

use super::ConnectionManager;

impl ConnectionManager {
    pub(super) async fn enforce_write_guard(
        &self,
        session_id: &str,
        risk: WriteRisk,
        preview: &str,
        object_name: Option<&str>,
        confirm_object_name: Option<&str>,
        profile_read_only_flag: bool,
    ) -> ConnectorResult<()> {
        if risk == WriteRisk::Read {
            return Ok(());
        }

        let (environment, read_only, connection_name) = {
            let guard = self.sessions.read().await;
            let live = guard.get(session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
            })?;
            (
                live.environment.clone(),
                live.read_only || profile_read_only_flag,
                live.connection_name.clone(),
            )
        };

        if read_only {
            return Err(ConnectorError::Policy(
                "This connection is read-only. Disable read-only to run writes.".into(),
            ));
        }

        if !environment_needs_unlock(&environment) {
            return Ok(());
        }

        let unlocked = self.write_unlocks.lock().await.contains(session_id);
        if !unlocked {
            return Err(ConnectorError::WriteGuard(WriteGuardPrompt::unlock(
                environment,
                connection_name,
                preview.to_string(),
            )));
        }

        if environment_needs_type_confirm(&environment, risk) {
            let expected = object_name
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("DROP");
            let typed = confirm_object_name.map(str::trim).unwrap_or("");
            if !typed.eq_ignore_ascii_case(expected) {
                return Err(ConnectorError::WriteGuard(WriteGuardPrompt::type_confirm(
                    environment,
                    connection_name,
                    preview.to_string(),
                    expected.to_string(),
                )));
            }
        }

        Ok(())
    }
}
