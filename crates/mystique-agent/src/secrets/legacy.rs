use std::collections::HashMap;
use std::fs;
use std::path::Path;

use mystique_core::{AppError, AppResult};

pub(super) fn read(path: &Path) -> HashMap<String, String> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

pub(super) fn write(path: &Path, map: &HashMap<String, String>) -> AppResult<()> {
    if map.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| AppError::Internal(format!("secrets dir: {e}")))?;
    }
    let json =
        serde_json::to_string(map).map_err(|e| AppError::Internal(format!("secrets json: {e}")))?;
    fs::write(path, json).map_err(|e| AppError::Internal(format!("secrets write: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}
