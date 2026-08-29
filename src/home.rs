use std::fs;
use std::path::{Path, PathBuf};

/// generic result alias used across xeon commands
pub type XResult<T> = Result<T, String>;

pub const PKG_DIR: &str = "pkg";
pub const LIB_DIR: &str = "lib";
pub const BIN_DIR: &str = "bin";
pub const CACHE_DIR: &str = "cache";
pub const ENDPOINTS_FILE: &str = "endpoints.toml";

/// `~/.xeon/` (or `$XEON_HOME`) — the install tree.
///
/// the same three-way layout (`lib/`, `bin/`, `pkg/`) is used both on the
/// filesystem and inside every endpoint, so installing a package is just
/// merging files from an endpoint into this tree.
#[derive(Debug, Clone)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// resolve the xeon home from `$XEON_HOME` or `~/.xeon`
    pub fn resolve() -> Home {
        let root = match std::env::var_os("XEON_HOME") {
            Some(path) => PathBuf::from(path),
            None => home::home_dir()
                .map(|p| p.join(".xeon"))
                .unwrap_or_else(|| PathBuf::from(".xeon")),
        };
        Home { root }
    }

    /// construct a home explicitly (used by tests)
    #[cfg(test)]
    pub fn from_root(root: PathBuf) -> Home {
        Home { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn lib_dir(&self) -> PathBuf {
        self.root.join(LIB_DIR)
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join(BIN_DIR)
    }

    pub fn pkg_dir(&self) -> PathBuf {
        self.root.join(PKG_DIR)
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join(CACHE_DIR)
    }

    pub fn endpoints_file(&self) -> PathBuf {
        self.root.join(ENDPOINTS_FILE)
    }

    pub fn is_initialized(&self) -> bool {
        self.pkg_dir().is_dir() && self.endpoints_file().is_file()
    }

    /// create the skeleton of a xeon home if it does not exist yet
    pub fn init(&self) -> XResult<()> {
        if self.is_initialized() {
            return Ok(());
        }
        for dir in [
            self.lib_dir(),
            self.bin_dir(),
            self.pkg_dir(),
            self.cache_dir(),
        ] {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
        }
        if !self.endpoints_file().exists() {
            fs::write(self.endpoints_file(), ENDPOINTS_TEMPLATE).map_err(|e| {
                format!("failed to write {}: {}", self.endpoints_file().display(), e)
            })?;
        }
        Ok(())
    }

    /// idempotently make sure the install dirs exist (never seeds config)
    pub fn ensure(&self) -> XResult<()> {
        for dir in [
            self.lib_dir(),
            self.bin_dir(),
            self.pkg_dir(),
            self.cache_dir(),
        ] {
            if !dir.is_dir() {
                fs::create_dir_all(&dir)
                    .map_err(|e| format!("failed to create {}: {}", dir.display(), e))?;
            }
        }
        Ok(())
    }
}

const ENDPOINTS_TEMPLATE: &str = "# xeon endpoints — where packages are installed from
#
# every endpoint is a tree that mirrors the xeon home layout:
#
#   <root>/
#   ├── lib/<name>.xeo        library modules (loaded with `use`)
#   ├── bin/<tool>            extension executables (run with `ext`/`extc`)
#   └── pkg/<name>.toml       package metadata
#
# local endpoints point straight at a directory on disk.
#   kind = \"local\"  path = \"/absolute/path\"
# git endpoints are cloned into ~/.xeon/cache/<name> and refreshed on update.
#   kind = \"git\"  url = \"https://github.com/user/xeo-pkgs.git\"
#
# [[endpoint]]
# name = \"hub\"
# kind = \"git\"
# url = \"https://github.com/rustle/xeo-hub.git\"
#
# [[endpoint]]
# name = \"local\"
# kind = \"local\"
# path = \"/home/rustle/pkgs\"
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_respects_xeon_home() {
        // SAFETY: test-only manipulation of env vars
        unsafe { std::env::set_var("XEON_HOME", "/tmp/xeon-home-test") };
        let home = Home::resolve();
        assert_eq!(home.root(), Path::new("/tmp/xeon-home-test"));
        unsafe { std::env::remove_var("XEON_HOME") };
    }

    #[test]
    fn layout_paths() {
        let home = Home::from_root(PathBuf::from("/tmp/.xeon"));
        assert_eq!(home.lib_dir(), PathBuf::from("/tmp/.xeon/lib"));
        assert_eq!(home.bin_dir(), PathBuf::from("/tmp/.xeon/bin"));
        assert_eq!(home.pkg_dir(), PathBuf::from("/tmp/.xeon/pkg"));
        assert_eq!(
            home.endpoints_file(),
            PathBuf::from("/tmp/.xeon/endpoints.toml")
        );
    }
}
