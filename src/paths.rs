/// path handling helpers shared across xeon
use std::path::{Path, PathBuf};

/// join `entry` onto `base`, rejecting absolute or `..`-escaping entries
pub fn child_path(base: &Path, entry: &str) -> crate::home::XResult<PathBuf> {
    if entry.starts_with('/')
        || entry.starts_with('\\')
        || entry.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(format!("unsafe path in package: {}", entry));
    }
    Ok(base.join(entry))
}

/// is this a git-style endpoint location (http(s) url, ssh url, scp syntax)?
pub fn looks_like_git_url(location: &str) -> bool {
    let lower = location.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
        || lower.starts_with("git+http:")
        || lower.starts_with("git+https:")
        || lower.starts_with("file://")
}

/// quickly sanitize a string into a filesystem-safe cache name
pub fn cache_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_path_accepts_nested() {
        let base = Path::new("/root/lib");
        assert_eq!(
            child_path(base, "printc.xeo").unwrap(),
            PathBuf::from("/root/lib/printc.xeo")
        );
        assert_eq!(
            child_path(base, "sub/dir/thing.xeo").unwrap(),
            PathBuf::from("/root/lib/sub/dir/thing.xeo")
        );
    }

    #[test]
    fn child_path_rejects_escape() {
        let base = Path::new("/root/lib");
        assert!(child_path(base, "../evil.xeo").is_err());
        assert!(child_path(base, "a/../../evil.xeo").is_err());
        assert!(child_path(base, "/etc/passwd").is_err());
    }

    #[test]
    fn git_url_detection() {
        assert!(looks_like_git_url("https://github.com/user/repo.git"));
        assert!(looks_like_git_url("git@github.com:user/repo.git"));
        assert!(looks_like_git_url("ssh://git@host/repo.git"));
        assert!(!looks_like_git_url("/home/user/pkgs"));
        assert!(!looks_like_git_url("printc"));
    }

    #[test]
    fn cache_name_sanitizes() {
        assert_eq!(cache_name("hub"), "hub");
        assert_eq!(
            cache_name("https://github.com/user/repo.git"),
            "https___github.com_user_repo.git"
        );
    }
}
