use anyhow::{Result, bail};
use vedit_core::commit::Author;

pub fn resolve_authors() -> Result<Vec<Author>> {
    let name = std::env::var("VEDIT_AUTHOR_NAME")
        .ok()
        .or_else(git_config_value("user.name"))
        .unwrap_or_else(|| "unknown".to_string());
    let email = std::env::var("VEDIT_AUTHOR_EMAIL")
        .ok()
        .or_else(git_config_value("user.email"))
        .unwrap_or_else(|| "unknown@local".to_string());
    let mut authors = vec![Author { name, email }];
    if let Ok(raw) = std::env::var("VEDIT_CO_AUTHORS") {
        authors.extend(parse_author_list(&raw)?);
    }
    Ok(authors)
}

fn parse_author_list(raw: &str) -> Result<Vec<Author>> {
    raw.split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_author)
        .collect()
}

fn parse_author(raw: &str) -> Result<Author> {
    let Some((name, rest)) = raw.split_once('<') else {
        bail!("invalid co-author {raw:?}: expected `Name <email>`");
    };
    let Some((email, trailing)) = rest.split_once('>') else {
        bail!("invalid co-author {raw:?}: expected `Name <email>`");
    };
    if !trailing.trim().is_empty() || name.trim().is_empty() || email.trim().is_empty() {
        bail!("invalid co-author {raw:?}: expected `Name <email>`");
    }
    Ok(Author {
        name: name.trim().to_string(),
        email: email.trim().to_string(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_semicolon_separated_co_author_list() {
        let authors =
            parse_author_list("Alice Example <alice@example.com>; Vedit Agent <agent@vedit.local>")
                .unwrap();

        assert_eq!(
            authors,
            vec![
                Author {
                    name: "Alice Example".to_string(),
                    email: "alice@example.com".to_string(),
                },
                Author {
                    name: "Vedit Agent".to_string(),
                    email: "agent@vedit.local".to_string(),
                }
            ]
        );
    }

    #[test]
    fn rejects_invalid_co_author_entry() {
        let err = parse_author_list("not an author").unwrap_err();
        assert!(err.to_string().contains("expected `Name <email>`"));
    }
}
