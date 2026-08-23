use crate::domain::{
    AgentChat, AgentMessage, AppendAgentMessageInput, CreateAgentChatInput,
};
use crate::error::AppResult;
use crate::storage::sqlite::repo;
use crate::storage::AppDatabase;

pub struct AgentChatService<'a> {
    pub(crate) db: &'a AppDatabase,
}

impl AgentChatService<'_> {
    pub fn list(&self, connection_id: &str) -> AppResult<Vec<AgentChat>> {
        self.db
            .with_conn(|c| repo::agent_chats::list_for_connection(c, connection_id))
    }

    pub fn get(&self, id: &str) -> AppResult<AgentChat> {
        self.db.with_conn(|c| repo::agent_chats::get_by_id(c, id))
    }

    pub fn create(&self, input: CreateAgentChatInput) -> AppResult<AgentChat> {
        self.db.with_conn(|c| repo::agent_chats::create(c, input))
    }

    pub fn update_meta(
        &self,
        id: &str,
        title: Option<&str>,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<AgentChat> {
        self.db
            .with_conn(|c| repo::agent_chats::update_meta(c, id, title, provider, model))
    }

    pub fn delete(&self, id: &str) -> AppResult<bool> {
        self.db.with_conn(|c| repo::agent_chats::delete(c, id))
    }

    pub fn list_messages(&self, chat_id: &str) -> AppResult<Vec<AgentMessage>> {
        self.db
            .with_conn(|c| repo::agent_chats::list_messages(c, chat_id))
    }

    pub fn append_message(&self, input: AppendAgentMessageInput) -> AppResult<AgentMessage> {
        self.db
            .with_conn(|c| repo::agent_chats::append_message(c, input))
    }
}
