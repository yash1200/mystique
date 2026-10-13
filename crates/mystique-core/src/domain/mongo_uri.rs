//! MongoDB connection-string userinfo. The password never stays in SQLite.

/// Username and password pulled out of a `mongodb://` or `mongodb+srv://` URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MongoUserinfo {
    pub username: String,
    pub password: String,
    /// Same URI with the `user:pass@` authority removed.
    pub uri: String,
}

/// Split userinfo off a Mongo URI.
///
/// `None` when the string is not a Mongo URI or it has no userinfo.
/// The password is percent-decoded (`s%40cret` → `s@cret`).
pub fn strip_mongo_userinfo(uri: &str) -> Option<MongoUserinfo> {
    let uri = uri.trim();
    let (scheme, rest) = uri.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("mongodb") && !scheme.eq_ignore_ascii_case("mongodb+srv") {
        return None;
    }
    let (userinfo, hostpart) = rest.rsplit_once('@')?;
    if userinfo.is_empty() || hostpart.is_empty() {
        return None;
    }
    let (user_enc, pass_enc) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let username = percent_decode(user_enc);
    let password = percent_decode(pass_enc);
    if username.is_empty() && password.is_empty() {
        return None;
    }
    Some(MongoUserinfo {
        username,
        password,
        uri: format!("{scheme}://{hostpart}"),
    })
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_percent_encoded_password() {
        let got =
            strip_mongo_userinfo("mongodb://admin:s%40cret@127.0.0.1:27017/app?authSource=admin")
                .expect("userinfo");
        assert_eq!(got.username, "admin");
        assert_eq!(got.password, "s@cret");
        assert_eq!(got.uri, "mongodb://127.0.0.1:27017/app?authSource=admin");
    }

    #[test]
    fn srv_uri_drops_userinfo() {
        let got = strip_mongo_userinfo("mongodb+srv://user:pw@cluster.example/db").expect("srv");
        assert_eq!(got.username, "user");
        assert_eq!(got.password, "pw");
        assert_eq!(got.uri, "mongodb+srv://cluster.example/db");
    }

    #[test]
    fn uri_without_userinfo_stays_put() {
        assert!(strip_mongo_userinfo("mongodb://127.0.0.1:27017/app").is_none());
    }
}
