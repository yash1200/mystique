use mystique_connector_api::{
    confirmation_for, confirmation_unlocks, target_environment, ConnectorError, ConnectorResult,
    Environment, WriteAction, WriteGuardPrompt, WriteRisk, WriteState, WriteTargets,
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
    /// The database the request names (grid, explorer, Mongo). `None`: the
    /// one the session opened with.
    pub database: Option<&'a str>,
    /// The schema the request names, with `database`.
    pub schema: Option<&'a str>,
    /// Databases the write lands in, read off the statement. `None`: the
    /// request's database (see `database`).
    pub targets: Option<WriteTargets>,
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
            database: None,
            schema: None,
            targets: None,
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

    /// The database unqualified names in this session go to. `None` when a
    /// `USE` moved it, so a guess would not be safe.
    pub(super) async fn session_database(&self, session_id: &str) -> Option<String> {
        if self.database_marks.switched(session_id) {
            return None;
        }
        let guard = self.sessions.read().await;
        guard.get(session_id)?.default_database.clone()
    }

    /// Document engines run in the request's database, else the session's.
    pub(super) async fn document_database(
        &self,
        session_id: &str,
        requested: Option<&str>,
    ) -> Option<String> {
        match requested.map(str::trim).filter(|db| !db.is_empty()) {
            Some(db) => Some(db.to_string()),
            None => self.session_database(session_id).await,
        }
    }

    /// Databases the statements in `text` write to, read by the engine's
    /// policy. No policy: unresolved.
    pub(super) async fn text_targets(
        &self,
        session_id: &str,
        text: &str,
        requested: Option<&str>,
    ) -> WriteTargets {
        let engine = {
            let guard = self.sessions.read().await;
            match guard.get(session_id) {
                Some(live) => live.session.engine_id().to_string(),
                None => return WriteTargets::unresolved(),
            }
        };
        let Some(policy) = self.registry.policy(&engine) else {
            return WriteTargets::unresolved();
        };
        let current = if engine.eq_ignore_ascii_case("mongodb") {
            self.document_database(session_id, requested).await
        } else {
            self.session_database(session_id).await
        };
        policy.write_targets(text, current.as_deref())
    }

    /// The prompt to show, or `None` when the write may run.
    pub(super) async fn write_prompt(
        &self,
        check: WriteCheck<'_>,
    ) -> ConnectorResult<Option<WriteGuardPrompt>> {
        if check.risk == WriteRisk::Read {
            return Ok(None);
        }
        let (connection_env, read_only, connection_name, marks, default_database) = {
            let guard = self.sessions.read().await;
            let live = guard.get(check.session_id).ok_or_else(|| {
                ConnectorError::InvalidConfig(format!("session not found: {}", check.session_id))
            })?;
            (
                Environment::parse(&live.environment),
                self.effective_read_only(check.session_id, live, check.profile_read_only),
                live.connection_name.clone(),
                self.database_marks.get(live.profile_key(check.session_id)),
                live.default_database.clone(),
            )
        };
        let targets = match check.targets {
            Some(targets) => targets,
            None if marks.is_empty() => WriteTargets::default(),
            None => {
                let current = check
                    .database
                    .map(str::trim)
                    .filter(|db| !db.is_empty())
                    .map(str::to_string)
                    .or(default_database);
                match current {
                    Some(db) => WriteTargets::database(&db, check.schema),
                    None => WriteTargets::unresolved(),
                }
            }
        };
        let judged = target_environment(connection_env, &marks, &targets);
        let env = judged.environment;
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
            env.as_str().to_string(),
            connection_name,
            check.preview,
            expected,
        )
        .map(|mut prompt| {
            prompt.database = judged.database;
            prompt
        });
        if prompt.is_none() && confirmation_unlocks(env, check.risk, state) {
            self.write_unlocks
                .lock()
                .await
                .insert(check.session_id.to_string());
        }
        Ok(prompt)
    }
}
