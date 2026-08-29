/// endpoint registry (`endpoints.toml`) and package discovery
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::home::{Home, XResult};
use crate::model::Package;
use crate::paths::{cache_name, looks_like_git_url};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Endpoint {
    Local { name: String, path: PathBuf },
    Git { name: String, url: String },
}

impl Endpoint {
    pub fn name(&self) -> &str {
        match self {
            Endpoint::Local { name, .. } | Endpoint::Git { name, .. } => name,
        }
    }

    pub fn is_git(&self) -> bool {
        matches!(self, Endpoint::Git { .. })
    }

    /// ensure this endpoint's tree is present locally (clone git, verify local)
    pub fn ensure(&self, home: &Home) -> XResult<PathBuf> {
        match self {
            Endpoint::Local { path, .. } => {
                if !path.is_dir() {
                    return Err(format!(
                        "local endpoint {} not found: {}",
                        self.name(),
                        path.display()
                    ));
                }
                Ok(path.clone())
            }
            Endpoint::Git { name, url } => {
                let cached = home.cache_dir().join(cache_name(name));
                if cached.is_dir() {
                    return Ok(cached);
                }
                clone_git(url, &cached)
            }
        }
    }

    /// pull the latest snapshot of a git endpoint (no-op for local)
    pub fn refresh(&self, home: &Home) -> XResult<PathBuf> {
        let root = self.ensure(home)?;
        if let Endpoint::Git { .. } = self
            && cmd_output(
                "git",
                &["-C", root.to_str().unwrap_or("."), "pull", "--ff-only"],
            )
            .is_err()
        {
            // shallow clone left in a detached state — just reset to origin head
            cmd_output(
                "git",
                &["-C", root.to_str().unwrap_or("."), "fetch", "origin"],
            )?;
            cmd_output(
                "git",
                &[
                    "-C",
                    root.to_str().unwrap_or("."),
                    "reset",
                    "--hard",
                    "origin/HEAD",
                ],
            )?;
        }
        Ok(root)
    }
}

/// build an ad-hoc endpoint (not stored in the registry) from a location string
pub fn adhoc_endpoint(name: &str, location: &str) -> XResult<Endpoint> {
    if looks_like_git_url(location) {
        Ok(Endpoint::Git {
            name: name.to_string(),
            url: location.to_string(),
        })
    } else {
        Ok(Endpoint::Local {
            name: name.to_string(),
            path: PathBuf::from(location),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EndpointRegistry {
    pub endpoint: Vec<Endpoint>,
}

impl EndpointRegistry {
    pub fn load(home: &Home) -> XResult<EndpointRegistry> {
        let path = home.endpoints_file();
        if !path.is_file() {
            return Ok(EndpointRegistry::default());
        }
        let source = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        let mut reg: EndpointRegistry =
            toml::from_str(&source).map_err(|e| format!("invalid {}: {}", path.display(), e))?;
        reg.endpoint.retain(|ep| !ep.name().is_empty());
        reg.endpoint.dedup_by(|a, b| a.name() == b.name());
        Ok(reg)
    }

    pub fn save(&self, home: &Home) -> XResult<()> {
        home.init()?;
        let rendered = toml::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize endpoints: {}", e))?;
        fs::write(home.endpoints_file(), rendered)
            .map_err(|e| format!("failed to write {}: {}", home.endpoints_file().display(), e))
    }

    pub fn get(&self, name: &str) -> Option<&Endpoint> {
        self.endpoint.iter().find(|ep| ep.name() == name)
    }

    pub fn add(&mut self, endpoint: Endpoint) -> XResult<()> {
        if self.get(endpoint.name()).is_some() {
            return Err(format!("endpoint '{}' already exists", endpoint.name()));
        }
        self.endpoint.push(endpoint);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.endpoint.len();
        self.endpoint.retain(|ep| ep.name() != name);
        before != self.endpoint.len()
    }
}

/// scan a package tree root for `pkg/*.toml` manifests
pub fn scan_root(root: &Path) -> XResult<Vec<(PathBuf, Package)>> {
    let pkg_dir = root.join(crate::home::PKG_DIR);
    if !pkg_dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&pkg_dir)
        .map_err(|e| format!("failed to read {}: {}", pkg_dir.display(), e))?;
    let mut found: Vec<(PathBuf, Package)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        if let Ok(pkg) = Package::read(&path) {
            found.push((path, pkg));
        }
    }
    found.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    Ok(found)
}

/// locate a package across every configured endpoint
pub fn find_package(
    home: &Home,
    reg: &EndpointRegistry,
    name: &str,
) -> XResult<Vec<(String, PathBuf, Package)>> {
    let mut hits: Vec<(String, PathBuf, Package)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for ep in &reg.endpoint {
        match ep.ensure(home) {
            Ok(root) => {
                let toml = root
                    .join(crate::home::PKG_DIR)
                    .join(format!("{}.toml", name));
                if !toml.is_file() {
                    continue;
                }
                if let Ok(pkg) = Package::read(&toml) {
                    hits.push((ep.name().to_string(), root, pkg));
                }
            }
            Err(_) => warnings.push(format!("endpoint '{}' unavailable (skipped)", ep.name())),
        }
    }

    if hits.is_empty() {
        let msg = format!("package '{}' not found in any endpoint", name);
        if warnings.is_empty() {
            return Err(msg);
        }
        return Err(format!("{}; {}", msg, warnings.join("; ")));
    }
    Ok(hits)
}

fn clone_git(url: &str, dest: &Path) -> XResult<PathBuf> {
    if let Some(parent) = dest.parent()
        && !parent.is_dir()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let status = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(dest)
        .status()
        .map_err(|e| format!("git is not available ({})", e))?;
    if !status.success() {
        return Err(format!("failed to clone {} into {}", url, dest.display()));
    }
    Ok(dest.to_path_buf())
}

fn cmd_output(cmd: &str, args: &[&str]) -> XResult<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed: {}", cmd, e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_roundtrip() {
        let mut reg = EndpointRegistry::default();
        reg.add(Endpoint::Git {
            name: "hub".into(),
            url: "https://github.com/u/r.git".into(),
        })
        .unwrap();
        reg.add(Endpoint::Local {
            name: "local".into(),
            path: PathBuf::from("/home/u/pkgs"),
        })
        .unwrap();

        let test_root = std::env::temp_dir().join(format!(
            "xeon-registry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = Home::from_root(test_root);
        reg.save(&home).unwrap();

        let loaded = EndpointRegistry::load(&home).unwrap();
        assert_eq!(loaded.endpoint.len(), 2);
        assert!(
            matches!(loaded.get("hub"), Some(Endpoint::Git { url, .. }) if url == "https://github.com/u/r.git")
        );
        assert!(
            matches!(loaded.get("local"), Some(Endpoint::Local { path, .. }) if path.as_path() == Path::new("/home/u/pkgs"))
        );

        let _ = fs::remove_dir_all(home.root());
    }

    #[test]
    fn add_rejects_duplicates() {
        let mut reg = EndpointRegistry::default();
        reg.add(Endpoint::Git {
            name: "hub".into(),
            url: "a".into(),
        })
        .unwrap();
        assert!(
            reg.add(Endpoint::Git {
                name: "hub".into(),
                url: "b".into()
            })
            .is_err()
        );
    }

    #[test]
    fn adhoc_kind_detection() {
        assert!(
            adhoc_endpoint("x", "https://github.com/u/r.git")
                .unwrap()
                .is_git()
        );
        assert!(!adhoc_endpoint("x", "/tmp/somewhere").unwrap().is_git());
    }
}
