use keyring::Entry;

const KEYRING_SERVICE: &str = "djdrop";

pub fn is_op_ref(s: &str) -> bool {
    s.starts_with("op://")
}

pub fn resolve_direct(s: &str) -> String {
    s.to_string()
}

/// Resolve a username_ref + password_ref to actual (username, password).
/// If refs start with "op://", call `op read`. Otherwise treat as literal values.
pub async fn resolve(username_ref: &str, password_ref: &str) -> Result<(String, String), String> {
    let username = if is_op_ref(username_ref) {
        op_read(username_ref).await?
    } else if username_ref.is_empty() {
        // Fall back to keychain for the service named by the ref key
        String::new()
    } else {
        username_ref.to_string()
    };

    let password = if is_op_ref(password_ref) {
        op_read(password_ref).await?
    } else {
        password_ref.to_string()
    };

    Ok((username, password))
}

async fn op_read(reference: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("op")
        .args(["read", reference])
        .output()
        .await
        .map_err(|_| "1Password CLI (`op`) not found — install 1Password desktop app".to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("op read failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Store credentials directly in OS keychain (fallback when op not available).
pub fn store_in_keychain(pool: &str, username: &str, password: &str) -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:username"))
        .map_err(|e| e.to_string())?;
    entry.set_password(username).map_err(|e| e.to_string())?;

    let entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:password"))
        .map_err(|e| e.to_string())?;
    entry.set_password(password).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn read_from_keychain(pool: &str) -> Result<(String, String), String> {
    let u_entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:username"))
        .map_err(|e| e.to_string())?;
    let username = u_entry.get_password().map_err(|e| e.to_string())?;

    let p_entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:password"))
        .map_err(|e| e.to_string())?;
    let password = p_entry.get_password().map_err(|e| e.to_string())?;

    Ok((username, password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_op_reference() {
        assert!(is_op_ref("op://Personal/BPM Supreme/username"));
        assert!(!is_op_ref("myusername@email.com"));
        assert!(!is_op_ref(""));
    }

    #[test]
    fn empty_ref_returns_empty_string() {
        assert_eq!(resolve_direct("myuser"), "myuser");
        assert_eq!(resolve_direct(""), "");
    }
}
