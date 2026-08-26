pub(super) fn extract_draft(text: &str) -> Option<String> {
    for fence in ["```sql", "```mql", "```mongodb"] {
        if let Some(rest) = text.split_once(fence) {
            let body = rest.1.split_once("```").map(|(b, _)| b).unwrap_or(rest.1);
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(rest) = text.split_once("```") {
        let after = rest.1.trim_start();
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("javascript"))
            .unwrap_or(after);
        let body = after.split_once("```").map(|(b, _)| b).unwrap_or(after);
        let trimmed = body.trim();
        if !trimmed.is_empty() && trimmed.len() < 20_000 {
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::extract_draft;

    #[test]
    fn prefers_sql_fence() {
        let text = "Here you go:\n```sql\nselect 1\n```\n";
        assert_eq!(extract_draft(text).as_deref(), Some("select 1"));
    }

    #[test]
    fn accepts_mql_fence() {
        let text = "```mql\ndb.users.find({})\n```";
        assert_eq!(extract_draft(text).as_deref(), Some("db.users.find({})"));
    }
}
