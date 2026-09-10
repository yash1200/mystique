//! Turn driver/SQL dumps into a short line the UI can toast.

/// Prettify a raw engine error. Always returns something showable.
pub fn prettify_sql_error(raw: &str) -> String {
    let cleaned = strip_noise(raw);
    if let Some(msg) = classify(&cleaned) {
        return msg;
    }
    cleaned
}

fn strip_noise(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    for marker in [
        " Failing row contains",
        "\nFailing row contains",
        "\nDETAIL:",
        "\nDetail:",
        " DETAIL:",
        "\nHINT:",
        " HINT:",
        " Hint:",
        " (constraint ",
        " at character ",
    ] {
        if let Some(i) = s.find(marker) {
            s.truncate(i);
        }
    }
    s.trim().trim_end_matches('.').trim().to_string()
}

fn classify(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();

    if let Some(col) = not_null_column(s, &lower) {
        return Some(format!("\"{col}\" cannot be empty."));
    }
    if looks_unique(&lower) {
        if lower.contains("e11000") || lower.contains("dup key") {
            return Some("A row with this value already exists.".into());
        }
        if let Some(col) = unique_column(s) {
            return Some(format!("A row with this \"{col}\" already exists."));
        }
        return Some("A row with this value already exists.".into());
    }
    if looks_fk_parent(&lower) {
        return Some("This row is still used by another table.".into());
    }
    if looks_fk_child(&lower) {
        return Some("This value doesn't match a related row.".into());
    }
    if looks_too_long(&lower) {
        if let Some(col) = quoted_after(s, &["column ", "for column ", "in column "]) {
            return Some(format!("\"{col}\" is too long."));
        }
        return Some("This value is too long.".into());
    }
    if looks_type(&lower) {
        return Some("This value is the wrong type.".into());
    }
    if looks_check(&lower) {
        return Some("This value doesn't meet a check constraint.".into());
    }
    None
}

fn not_null_column(s: &str, lower: &str) -> Option<String> {
    if lower.contains("violates not-null") || lower.contains("not-null constraint") {
        return quoted_after(s, &["column "]);
    }
    if lower.contains("cannot be null") || lower.contains("cannot be empty") {
        return quoted_after(s, &["column ", "column '"]);
    }
    if lower.contains("not null constraint failed") {
        return ident_after(s, "not null constraint failed:");
    }
    None
}

fn unique_column(s: &str) -> Option<String> {
    if let Some(col) = ident_after(s, "unique constraint failed:") {
        return Some(col);
    }
    let lower = s.to_ascii_lowercase();
    for marker in ["for key ", "index: "] {
        if let Some(i) = lower.find(marker) {
            if let Some(name) = first_quoted(&s[i + marker.len()..]) {
                return Some(name);
            }
        }
    }
    None
}

fn looks_unique(lower: &str) -> bool {
    lower.contains("duplicate key")
        || lower.contains("duplicate entry")
        || lower.contains("unique constraint")
        || lower.contains("already exists")
        || lower.contains("e11000")
}

fn looks_fk_parent(lower: &str) -> bool {
    lower.contains("is still referenced") || lower.contains("cannot delete or update a parent row")
}

fn looks_fk_child(lower: &str) -> bool {
    lower.contains("violates foreign key")
        || lower.contains("foreign key constraint")
        || lower.contains("cannot add or update a child row")
}

fn looks_too_long(lower: &str) -> bool {
    lower.contains("value too long")
        || lower.contains("data too long")
        || lower.contains("right truncation")
}

fn looks_type(lower: &str) -> bool {
    lower.contains("invalid input syntax")
        || lower.contains("incorrect") && lower.contains("value")
        || lower.contains("wrong data type")
}

fn looks_check(lower: &str) -> bool {
    lower.contains("violates check constraint") || lower.contains("check constraint failed")
}

fn quoted_after(s: &str, markers: &[&str]) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    for marker in markers {
        if let Some(i) = lower.find(marker) {
            let rest = &s[i + marker.len()..];
            if let Some(name) = first_quoted(rest) {
                return Some(name);
            }
        }
    }
    first_quoted(s)
}

fn first_quoted(s: &str) -> Option<String> {
    for (open, close) in [('"', '"'), ('\'', '\''), ('`', '`')] {
        if let Some(a) = s.find(open) {
            let rest = &s[a + 1..];
            if let Some(b) = rest.find(close) {
                let name = rest[..b].trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// `table.col` or `col` after a sqlite-style prefix.
fn ident_after(s: &str, prefix: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let i = lower.find(prefix)?;
    let rest = s[i + prefix.len()..].trim();
    let token = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == '(')
        .next()?
        .trim_matches(|c| c == '"' || c == '\'' || c == '`');
    let col = token.rsplit('.').next().unwrap_or(token);
    if col.is_empty() {
        None
    } else {
        Some(col.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::prettify_sql_error;

    #[test]
    fn postgres_not_null_drops_failing_row() {
        let raw = "null value in column \"content\" of relation \"session_messages\" \
            violates not-null constraint. Failing row contains (1, 1, abc, null).";
        assert_eq!(prettify_sql_error(raw), "\"content\" cannot be empty.");
    }

    #[test]
    fn mysql_not_null() {
        assert_eq!(
            prettify_sql_error("Column 'email' cannot be null"),
            "\"email\" cannot be empty."
        );
    }

    #[test]
    fn sqlite_not_null() {
        assert_eq!(
            prettify_sql_error("NOT NULL constraint failed: users.email"),
            "\"email\" cannot be empty."
        );
    }

    #[test]
    fn postgres_unique() {
        assert_eq!(
            prettify_sql_error(
                "duplicate key value violates unique constraint \"users_email_key\""
            ),
            "A row with this value already exists."
        );
    }

    #[test]
    fn mysql_duplicate() {
        assert_eq!(
            prettify_sql_error("Duplicate entry 'a@b.c' for key 'users.email'"),
            "A row with this \"users.email\" already exists."
        );
    }

    #[test]
    fn sqlite_unique() {
        assert_eq!(
            prettify_sql_error("UNIQUE constraint failed: users.email"),
            "A row with this \"email\" already exists."
        );
    }
}
