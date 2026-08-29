// plain-HTTP file downloading for static (non-git) http endpoints.
//
// a "type http" endpoint can be either a git repository served over https
// (e.g. https://github.com/user/repo.git) or a plain static file server such
// as GitHub Pages (e.g. https://user.github.io/repo/). git repositories are
// handled through git; this module is the fallback that downloads individual
// files with curl when the url is not a git repository.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::home::XResult;

/// true if `url` responds as a git repository (testable via `git ls-remote`)
pub fn is_git_repo(url: &str) -> bool {
    Command::new("git")
        .args(["ls-remote", url, "HEAD"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// download `url` to `dest`, creating parent dirs. returns an error if the
/// server responds anything other than 2xx.
pub fn download(url: &str, dest: &Path) -> XResult<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let status = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| format!("curl is not available ({})", e))?;
    if !status.success() {
        return Err(format!("failed to download {}", url));
    }
    Ok(())
}

/// join a relative file path onto an http base url, handling a trailing slash
/// on the base and preserving the base's own path prefix.
pub fn join_url(base: &str, rel: &str) -> String {
    let base = base.trim_end_matches('/');
    let rel = rel.trim_start_matches('/');
    format!("{}/{}", base, rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_url_handles_slashes() {
        assert_eq!(
            join_url("https://pkgxeo.github.io/repo", "pkg/fs.toml"),
            "https://pkgxeo.github.io/repo/pkg/fs.toml"
        );
        assert_eq!(
            join_url("https://pkgxeo.github.io/repo/", "lib/fs/fs.xeo"),
            "https://pkgxeo.github.io/repo/lib/fs/fs.xeo"
        );
    }

    #[test]
    fn join_url_preserves_base_path() {
        let url = join_url("https://x.example/base/", "pkg/a.toml");
        assert_eq!(url, "https://x.example/base/pkg/a.toml");
    }
}
