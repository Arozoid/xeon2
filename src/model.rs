/// package metadata — the `pkg/<name>.toml` manifest
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::home::XResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    /// other package names needed before this one
    pub depends: Vec<String>,
    /// library modules under `lib/` that belong to this package
    pub lib: Vec<String>,
    /// executables under `bin/` that belong to this package
    pub bin: Vec<String>,
    /// where this package was installed from (`<endpoint>` / `path:<path>` / `git:<url>`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

impl Default for Package {
    fn default() -> Self {
        Package {
            name: String::new(),
            version: "0.1.0".to_string(),
            description: String::new(),
            depends: Vec::new(),
            lib: Vec::new(),
            bin: Vec::new(),
            origin: None,
        }
    }
}

impl Package {
    pub fn parse(source: &str) -> XResult<Package> {
        let pkg: Package =
            toml::from_str(source).map_err(|e| format!("invalid package metadata: {}", e))?;
        pkg.validate()?;
        Ok(pkg)
    }

    pub fn render(&self) -> XResult<String> {
        toml::to_string_pretty(self).map_err(|e| format!("failed to serialize metadata: {}", e))
    }

    pub fn read(path: &Path) -> XResult<Package> {
        let source = fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        Self::parse(&source)
    }

    pub fn write(&self, path: &Path) -> XResult<()> {
        let rendered = self.render()?;
        fs::write(path, rendered).map_err(|e| format!("failed to write {}: {}", path.display(), e))
    }

    pub fn validate(&self) -> XResult<()> {
        if !is_valid_name(&self.name) {
            return Err(format!("invalid package name: {:?}", self.name));
        }
        if self.version.trim().is_empty() {
            return Err(format!("package {:?} is missing a version", self.name));
        }
        Ok(())
    }

    /// the relative files (dir, filename) this package owns inside the install tree
    pub fn owned_files(&self) -> Vec<(&'static str, &str)> {
        let mut files: Vec<(&'static str, &str)> =
            self.lib.iter().map(|f| ("lib", f.as_str())).collect();
        files.extend(self.bin.iter().map(|f| ("bin", f.as_str())));
        files
    }
}

/// a valid package name is a single path segment, no whitespace
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Package {
        Package {
            name: "printc".into(),
            version: "0.1.0".into(),
            description: "ANSI color printing for .xeo".into(),
            depends: vec!["json".into()],
            lib: vec!["printc.xeo".into()],
            bin: vec!["printc".into()],
            origin: Some("hub".into()),
        }
    }

    #[test]
    fn render_parse_roundtrip() {
        let pkg = sample();
        let text = pkg.render().unwrap();
        let back = Package::parse(&text).unwrap();
        assert_eq!(back.name, "printc");
        assert_eq!(back.version, "0.1.0");
        assert_eq!(back.depends, vec!["json"]);
        assert_eq!(back.lib, vec!["printc.xeo"]);
        assert_eq!(back.origin.as_deref(), Some("hub"));
    }

    #[test]
    fn missing_fields_default() {
        let pkg = Package::parse("name = \"lonely\"\n").unwrap();
        assert_eq!(pkg.name, "lonely");
        assert_eq!(pkg.version, "0.1.0");
        assert!(pkg.lib.is_empty());
        assert!(pkg.origin.is_none());
    }

    #[test]
    fn reject_bad_names() {
        assert!(Package::parse("name = \"a/b\"\n").is_err());
        assert!(Package::parse("name = \"\"\n").is_err());
    }

    #[test]
    fn owned_files_lists_both_kinds() {
        let pkg = sample();
        let owned = pkg.owned_files();
        assert!(owned.contains(&("lib", "printc.xeo")));
        assert!(owned.contains(&("bin", "printc")));
    }
}
