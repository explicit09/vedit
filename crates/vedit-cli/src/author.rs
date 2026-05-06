use anyhow::Result;
use vedit_core::commit::Author;

/// Resolve commit authorship from env vars first, then a git config sniff,
/// then a default. We deliberately do not read .vedit/config yet; that
/// arrives when there's a repo-level config story.
pub fn resolve() -> Result<Author> {
    let name = std::env::var("VEDIT_AUTHOR_NAME")
        .ok()
        .or_else(git_config_value("user.name"))
        .unwrap_or_else(|| "unknown".to_string());
    let email = std::env::var("VEDIT_AUTHOR_EMAIL")
        .ok()
        .or_else(git_config_value("user.email"))
        .unwrap_or_else(|| "unknown@local".to_string());
    Ok(Author { name, email })
}

fn git_config_value(key: &'static str) -> impl FnOnce() -> Option<String> {
    move || {
        let out = std::process::Command::new("git")
            .args(["config", "--get", key])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8(out.stdout).ok()?;
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}
