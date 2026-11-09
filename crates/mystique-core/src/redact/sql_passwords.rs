//! Mask passwords written into account statements before SQL is stored.
//!
//! Query history keeps SQL so it can be run again, so this is narrow on
//! purpose: `UPDATE users SET password = 'x'` and `WHERE password = 'x'` are
//! data and stay. What is masked is the secret of an account or connection:
//!
//! - `CREATE|ALTER USER|ROLE … [ENCRYPTED] PASSWORD 'x'` (Postgres)
//! - `… IDENTIFIED BY 'x'`, `IDENTIFIED WITH plugin BY|AS 'x'`, `IDENTIFIED BY
//!   PASSWORD 'hash'`, and the `REPLACE 'old'` that may follow (MySQL / MariaDB)
//! - `IDENTIFIED BY x` with an unquoted password (Oracle style)
//! - `SET PASSWORD [FOR u] = 'x'` and `PASSWORD('x')`
//! - `OPTIONS (… password 'x')` (foreign servers and user mappings)
//! - `MASTER_PASSWORD = 'x'`, `SOURCE_PASSWORD = 'x'`, `PASSWORD = 'x'` in a
//!   statement that starts with `SET`, `CREATE`, `ALTER`, `GRANT`, `CHANGE`
//!   or `START`
//! - Mongo `createUser` / `updateUser` `pwd: "x"` and
//!   `changeUserPassword("u", "x")`
//!
//! A quoted value keeps its quote: `'x'` becomes `'***'`.

use super::MASK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    /// Quoted string. `quote` is the opening character (`'`, `"`, or `$`).
    Literal {
        quote: u8,
    },
    Punct(u8),
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Words, string literals and punctuation. Comments and blanks are dropped.
fn tokenize(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        let start = i;
        match byte {
            b if b.is_ascii_whitespace() => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                while bytes.get(i).is_some_and(|&b| b != b'\n') {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'\'' | b'"' | b'`' => {
                i = quoted_end(bytes, i, byte);
                if byte == b'`' {
                    // A quoted identifier, not a value.
                    out.push(Token {
                        kind: Kind::Word,
                        start,
                        end: i,
                    });
                } else {
                    out.push(Token {
                        kind: Kind::Literal { quote: byte },
                        start,
                        end: i,
                    });
                }
            }
            b'$' => match dollar_end(text, i) {
                Some(end) => {
                    i = end;
                    out.push(Token {
                        kind: Kind::Literal { quote: b'$' },
                        start,
                        end,
                    });
                }
                None => {
                    i += 1;
                    out.push(Token {
                        kind: Kind::Punct(b'$'),
                        start,
                        end: i,
                    });
                }
            },
            b if b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80 => {
                // `E'…'` / `N'…'` / `_utf8'…'` prefixes belong to the literal.
                while bytes
                    .get(i)
                    .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80)
                {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'\'') {
                    i = quoted_end(bytes, i, b'\'');
                    out.push(Token {
                        kind: Kind::Literal { quote: b'\'' },
                        start,
                        end: i,
                    });
                } else {
                    out.push(Token {
                        kind: Kind::Word,
                        start,
                        end: i,
                    });
                }
            }
            other => {
                i += 1;
                out.push(Token {
                    kind: Kind::Punct(other),
                    start,
                    end: i,
                });
            }
        }
    }
    out
}

/// End (exclusive) of a quoted run. A doubled quote and a backslash escape
/// stay inside. An unterminated quote runs to the end, which over-masks.
fn quoted_end(bytes: &[u8], open: usize, quote: u8) -> usize {
    let mut i = open + 1;
    while let Some(&byte) = bytes.get(i) {
        if byte == b'\\' && quote != b'`' {
            i += 2;
            continue;
        }
        if byte == quote {
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

/// `$tag$ … $tag$` (Postgres). `None` when `$` does not open one (`$1`).
fn dollar_end(text: &str, open: usize) -> Option<usize> {
    let rest = text.get(open + 1..)?;
    let tag_len = rest.find('$')?;
    let tag = rest.get(..tag_len)?;
    if !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || tag.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return None;
    }
    let delimiter = format!("${tag}$");
    let body_start = open + delimiter.len();
    let close = text.get(body_start..)?.find(&delimiter)?;
    Some(body_start + close + delimiter.len())
}

fn word<'a>(text: &'a str, token: &Token) -> Option<&'a str> {
    (token.kind == Kind::Word)
        .then(|| text.get(token.start..token.end))
        .flatten()
}

fn is_word(text: &str, token: Option<&Token>, want: &str) -> bool {
    token
        .and_then(|token| word(text, token))
        .is_some_and(|got| got.eq_ignore_ascii_case(want))
}

fn is_literal(token: Option<&Token>) -> bool {
    token.is_some_and(|token| matches!(token.kind, Kind::Literal { .. }))
}

fn is_punct(token: Option<&Token>, want: u8) -> bool {
    token.is_some_and(|token| token.kind == Kind::Punct(want))
}

/// Statements whose `PASSWORD = 'x'` sets an account or connection secret:
/// `SET PASSWORD`, replication (`CHANGE …`, `START REPLICA …`), and
/// `CREATE|ALTER USER|ROLE|LOGIN|SERVER`. `CREATE VIEW … WHERE password = 'x'`
/// is data and is kept.
fn sets_a_secret(first: &str, second: Option<&str>) -> bool {
    let is = |word: &str, kws: &[&str]| kws.iter().any(|kw| word.eq_ignore_ascii_case(kw));
    if is(first, &["set", "change", "start", "grant"]) {
        return true;
    }
    is(first, &["create", "alter"])
        && second.is_some_and(|second| is(second, &["user", "role", "login", "server"]))
}

/// Mask passwords in account and connection statements. Other SQL is unchanged.
pub fn redact_sql_passwords(text: &str) -> String {
    let tokens = tokenize(text);
    let mut masks: Vec<Token> = Vec::new();
    let mongo_account = {
        let lower = text.to_ascii_lowercase();
        lower.contains("createuser") || lower.contains("updateuser")
    };
    let mut first_word: Option<&str> = None;
    let mut second_word: Option<&str> = None;
    let mut identified = false;
    for (index, token) in tokens.iter().enumerate() {
        let at = |offset: usize| tokens.get(index + offset);
        if is_punct(Some(token), b';') {
            first_word = None;
            second_word = None;
            identified = false;
            continue;
        }
        let Some(current) = word(text, token) else {
            // Mongo: `"pwd": "x"` in a createUser / updateUser document.
            if mongo_account
                && is_literal(Some(token))
                && text
                    .get(token.start + 1..token.end.saturating_sub(1))
                    .is_some_and(|key| key.eq_ignore_ascii_case("pwd"))
                && is_punct(at(1), b':')
                && is_literal(at(2))
            {
                masks.extend(at(2));
            }
            continue;
        };
        let first = match first_word {
            Some(first) => {
                if second_word.is_none() {
                    second_word = Some(current);
                }
                first
            }
            None => *first_word.insert(current),
        };
        let lower = current.to_ascii_lowercase();

        if lower == "password" {
            // `PASSWORD 'x'` and `PASSWORD ('x')`.
            if is_literal(at(1)) {
                masks.extend(at(1));
            } else if is_punct(at(1), b'(') && is_literal(at(2)) {
                masks.extend(at(2));
            }
        }
        if lower.ends_with("password") && sets_a_secret(first, second_word) {
            // `SET PASSWORD = 'x'`, `MASTER_PASSWORD = 'x'`, and
            // `SET PASSWORD FOR 'u'@'h' = 'x'`: the value after the next `=`.
            let mut ahead = 1;
            while let Some(next) = at(ahead) {
                if is_punct(Some(next), b';') || ahead > 8 {
                    break;
                }
                if is_punct(Some(next), b'=') {
                    if is_literal(at(ahead + 1)) {
                        masks.extend(at(ahead + 1));
                    } else if is_word(text, at(ahead + 1), "password")
                        && is_punct(at(ahead + 2), b'(')
                        && is_literal(at(ahead + 3))
                    {
                        masks.extend(at(ahead + 3));
                    }
                    break;
                }
                ahead += 1;
            }
        }
        if lower == "identified" {
            identified = true;
            // IDENTIFIED [WITH plugin] BY|AS [PASSWORD] secret
            let mut ahead = 1;
            if is_word(text, at(ahead), "with") {
                ahead += 2;
            }
            if is_word(text, at(ahead), "by") || is_word(text, at(ahead), "as") {
                ahead += 1;
                if is_word(text, at(ahead), "password") {
                    ahead += 1;
                }
                if let Some(secret) = at(ahead) {
                    let unquoted = word(text, secret).is_some_and(|w| {
                        !["random", "values", "externally", "globally"]
                            .iter()
                            .any(|kw| w.eq_ignore_ascii_case(kw))
                    });
                    if is_literal(Some(secret)) || unquoted {
                        masks.push(*secret);
                    }
                }
            }
        }
        if lower == "replace" && identified && is_literal(at(1)) {
            masks.extend(at(1));
        }
        if mongo_account && lower == "pwd" && is_punct(at(1), b':') && is_literal(at(2)) {
            masks.extend(at(2));
        }
        if lower == "changeuserpassword"
            && is_punct(at(1), b'(')
            && is_literal(at(2))
            && is_punct(at(3), b',')
            && is_literal(at(4))
        {
            masks.extend(at(4));
        }
    }
    apply_masks(text, masks)
}

fn apply_masks(text: &str, mut masks: Vec<Token>) -> String {
    if masks.is_empty() {
        return text.to_string();
    }
    masks.sort_by_key(|token| token.start);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for token in masks {
        if token.start < copied {
            continue;
        }
        out.push_str(text.get(copied..token.start).unwrap_or_default());
        match token.kind {
            Kind::Literal { quote: b'$' } => {
                out.push('\'');
                out.push_str(MASK);
                out.push('\'');
            }
            Kind::Literal { quote } => {
                // Keep a prefix such as `E` or `_utf8`, and the quote.
                let body = text.get(token.start..token.end).unwrap_or_default();
                let open = body.find(char::from(quote)).unwrap_or(0);
                out.push_str(body.get(..=open).unwrap_or_default());
                out.push_str(MASK);
                out.push(char::from(quote));
            }
            _ => out.push_str(MASK),
        }
        copied = token.end;
    }
    out.push_str(text.get(copied..).unwrap_or_default());
    out
}

#[cfg(test)]
mod tests {
    use super::redact_sql_passwords as redact;

    #[test]
    fn postgres_role_passwords_are_masked() {
        assert_eq!(
            redact("ALTER USER app WITH PASSWORD 's3cret'"),
            "ALTER USER app WITH PASSWORD '***'"
        );
        assert_eq!(
            redact("CREATE ROLE r LOGIN ENCRYPTED PASSWORD 'md5abc' VALID UNTIL 'infinity'"),
            "CREATE ROLE r LOGIN ENCRYPTED PASSWORD '***' VALID UNTIL 'infinity'"
        );
        assert_eq!(
            redact("alter role r password $p$it's$p$"),
            "alter role r password '***'"
        );
        assert_eq!(
            redact("CREATE USER u PASSWORD E'a\\'b'; SELECT 1"),
            "CREATE USER u PASSWORD E'***'; SELECT 1"
        );
    }

    #[test]
    fn mysql_identified_by_and_set_password_are_masked() {
        assert_eq!(
            redact("CREATE USER 'u'@'%' IDENTIFIED BY 'pw'"),
            "CREATE USER 'u'@'%' IDENTIFIED BY '***'"
        );
        assert_eq!(
            redact("ALTER USER u IDENTIFIED WITH caching_sha2_password BY \"pw\" REPLACE 'old' RETAIN CURRENT PASSWORD"),
            "ALTER USER u IDENTIFIED WITH caching_sha2_password BY \"***\" REPLACE '***' RETAIN CURRENT PASSWORD"
        );
        assert_eq!(
            redact("ALTER USER u IDENTIFIED WITH mysql_native_password AS '*HASH'"),
            "ALTER USER u IDENTIFIED WITH mysql_native_password AS '***'"
        );
        assert_eq!(
            redact("GRANT ALL ON *.* TO u IDENTIFIED BY PASSWORD '*HASH'"),
            "GRANT ALL ON *.* TO u IDENTIFIED BY PASSWORD '***'"
        );
        assert_eq!(redact("SET PASSWORD = 'pw'"), "SET PASSWORD = '***'");
        assert_eq!(
            redact("SET PASSWORD FOR 'u'@'localhost' = PASSWORD('pw')"),
            "SET PASSWORD FOR 'u'@'localhost' = PASSWORD('***')"
        );
        assert_eq!(
            redact("CHANGE REPLICATION SOURCE TO SOURCE_USER='r', SOURCE_PASSWORD='pw'"),
            "CHANGE REPLICATION SOURCE TO SOURCE_USER='r', SOURCE_PASSWORD='***'"
        );
        assert_eq!(
            redact("CREATE USER scott IDENTIFIED BY tiger"),
            "CREATE USER scott IDENTIFIED BY ***"
        );
        assert_eq!(
            redact("ALTER USER u IDENTIFIED BY RANDOM PASSWORD"),
            "ALTER USER u IDENTIFIED BY RANDOM PASSWORD"
        );
    }

    #[test]
    fn sql_server_logins_are_masked() {
        assert_eq!(
            redact("CREATE LOGIN app WITH PASSWORD = 'pw', CHECK_POLICY = OFF"),
            "CREATE LOGIN app WITH PASSWORD = '***', CHECK_POLICY = OFF"
        );
    }

    #[test]
    fn foreign_server_options_are_masked() {
        assert_eq!(
            redact("CREATE USER MAPPING FOR app SERVER s OPTIONS (user 'r', password 'pw')"),
            "CREATE USER MAPPING FOR app SERVER s OPTIONS (user 'r', password '***')"
        );
    }

    #[test]
    fn mongo_account_commands_are_masked() {
        assert_eq!(
            redact(r#"db.createUser({ user: "u", pwd: "pw", roles: [] })"#),
            r#"db.createUser({ user: "u", pwd: "***", roles: [] })"#
        );
        assert_eq!(
            redact(r#"db.runCommand({ "updateUser": "u", "pwd": 'pw' })"#),
            r#"db.runCommand({ "updateUser": "u", "pwd": '***' })"#
        );
        assert_eq!(
            redact(r#"db.changeUserPassword("u", "pw")"#),
            r#"db.changeUserPassword("u", "***")"#
        );
    }

    #[test]
    fn data_that_happens_to_be_a_password_column_is_kept() {
        for sql in [
            "UPDATE users SET password = 'x' WHERE id = 1",
            "SELECT * FROM users WHERE password = 'x'",
            "INSERT INTO users (name, password) VALUES ('a', 'x')",
            "CREATE TABLE t (password text DEFAULT 'x')",
            "CREATE VIEW v AS SELECT * FROM users WHERE password = 'x'",
            "SELECT 'IDENTIFIED BY' AS label",
            "-- ALTER USER u PASSWORD 'not a statement'\nSELECT 1",
            r#"db.users.find({ pwd: "x" })"#,
        ] {
            assert_eq!(redact(sql), sql);
        }
    }

    #[test]
    fn statements_after_the_first_are_checked_too() {
        assert_eq!(
            redact("SELECT 1; ALTER USER app PASSWORD 'pw'"),
            "SELECT 1; ALTER USER app PASSWORD '***'"
        );
        assert_eq!(
            redact("UPDATE t SET password = 'kept'; SET PASSWORD = 'gone'"),
            "UPDATE t SET password = 'kept'; SET PASSWORD = '***'"
        );
    }
}
