use std::path::Path;

use rand::Rng;

/// Load the management API bearer token from disk, generating and
/// persisting a fresh random one on first run (FR26). The CLI reads the
/// same file to authenticate, which is sufficient for the single-operator,
/// localhost-by-default v1 threat model (see PRD "Open" questions).
pub fn load_or_create(token_file: &Path) -> anyhow::Result<String> {
    if let Ok(existing) = std::fs::read_to_string(token_file) {
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    let token: String = {
        let mut rng = rand::thread_rng();
        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..16);
                std::char::from_digit(idx, 16).unwrap()
            })
            .collect()
    };

    std::fs::write(token_file, &token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(token_file, perms)?;
    }

    Ok(token)
}
