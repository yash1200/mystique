use uuid::Uuid;

pub(crate) fn ensure_token(existing: Option<&str>) -> String {
    let t = existing.map(str::trim).unwrap_or("");
    if t.len() >= 16 {
        t.to_string()
    } else {
        Uuid::new_v4().simple().to_string()
    }
}
