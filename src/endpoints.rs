/// endpoint registry (`endpoints.toml`) and package discovery
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::home::{Home, XResult};
use crate::model::Package;
use crate::paths::{cache_name, looks_like_http_url};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Endpoint {
    Local { name: String, path: PathBuf },
    Http { name: String, url: String },
}

impl Endpoint {
    pub fn name(&self) -> &str {
        match self {
            Endpoint::Local { name, .. } | Endpoint::Http { name, .. } => name,
        }
    }

    pub fn is_http(&self) -> bool {
        matches!(self, Endpoint::Http { .. })
    }

    /// for a local endpoint: the on-disk tree root. for an http endpoint: the
    /// cache directory where its pkg/ catalog is (or will be) stored.
    pub fn root(&self, home: &Home) -> PathBuf {
        match self {
            Endpoint::Local { path, .. } => path.clone(),
            Endpoint::Http { name, .. } => home.cache_dir().join(cache_name(name)),
        }
    }

    /// the `pkg/` directory holding this endpoint's catalog
    pub fn pkg_dir(&self, home: &Home) -> PathBuf {
        self.root(home).join(crate::home::PKG_DIR)
    }

    /// fetch/hook this endpoint's catalog (pkg/*) so it is usable locally.
    ///
    /// - local: verify the tree and ensure a repo.db catalog exists
    /// - http:  fetch only `pkg/` (repo.db + package tomls) into cache, never
    ///          lib/ or bin/. this is only called when installing/updating.
    pub fn hook(&self, home: &Home) -> XResult<PathBuf> {
        match self {
            Endpoint::Local { path, .. } => {
                if !path.is_dir() {
                    return Err(format!(
                        "local endpoint {} not found: {}",
                        self.name(),
                        path.display()
                    ));
                }
                let pkg_dir = path.join(crate::home::PKG_DIR);
                crate::repo::ensure(&pkg_dir)?;
                Ok(pkg_dir)
            }
            Endpoint::Http { name, url } => {
                let cached = home.cache_dir().join(cache_name(name));
                fs::create_dir_all(&cached)
                    .map_err(|e| format!("failed to create {}: {}", cached.display(), e))?;
                clone_pkg_only(url, &cached)?;
                let pkg_dir = cached.join(crate::home::PKG_DIR);
                crate::repo::ensure(&pkg_dir)?;
                Ok(pkg_dir)
            }
        }
    }

    /// the packages this endpoint currently lists, without any network access.
    /// http endpoints only read what is already in the cache (if hooked).
    pub fn catalog(&self, home: &Home) -> XResult<Vec<(PathBuf, Package)>> {
        let pkg_dir = self.pkg_dir(home);
        if !pkg_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for (toml, pkg) in scan_dir(&pkg_dir)? {
            out.push((toml, pkg));
        }
        Ok(out)
    }

    /// ensure the cached catalog is usable, fetching it if a local git tree is
    /// already present but stale. used by `xeon update`.
    pub fn refresh(&self, home: &Home) -> XResult<PathBuf> {
        match self {
            Endpoint::Local { .. } => {
                let pkg_dir = self.pkg_dir(home);
                crate::repo::ensure(&pkg_dir)?;
                Ok(pkg_dir)
            }
            Endpoint::Http { name, url } => {
                let cached = home.cache_dir().join(cache_name(name));
                fs::create_dir_all(&cached)
                    .map_err(|e| format!("failed to create {}: {}", cached.display(), e))?;
                pull_pkg_only(url, &cached)?;
                let pkg_dir = cached.join(crate::home::PKG_DIR);
                crate::repo::ensure(&pkg_dir)?;
                Ok(pkg_dir)
            }
        }
    }
}

/// build an ad-hoc endpoint (not stored in the registry) from a location string
pub fn adhoc_endpoint(name: &str, location: &str) -> XResult<Endpoint> {
    if looks_like_http_url(location) {
        Ok(Endpoint::Http {
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

/// scan a `pkg/` directory for `*.toml` manifests (the catalog source of truth)
pub fn scan_dir(pkg_dir: &Path) -> XResult<Vec<(PathBuf, Package)>> {
    if !pkg_dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(pkg_dir)
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

/// scan a package tree root for `pkg/*.toml` manifests
pub fn scan_root(root: &Path) -> XResult<Vec<(PathBuf, Package)>> {
    scan_dir(&root.join(crate::home::PKG_DIR))
}

/// locate a package across every configured endpoint, without network access.
/// http endpoints only consult the metadata already in the cache; run `hook`
/// (during install/update) to fetch remote metadata.
pub fn find_package(
    home: &Home,
    reg: &EndpointRegistry,
    name: &str,
) -> XResult<Vec<(String, PathBuf, Package)>> {
    let mut hits: Vec<(String, PathBuf, Package)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for ep in &reg.endpoint {
        let pkg_dir = ep.pkg_dir(home);
        let toml = pkg_dir.join(format!("{}.toml", name));
        if !toml.is_file() {
            continue;
        }
        if let Ok(pkg) = Package::read(&toml) {
            hits.push((ep.name().to_string(), pkg_dir, pkg));
        } else {
            warnings.push(format!("endpoint '{}' has an invalid catalog entry", ep.name()));
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

/// clone only the `pkg/` tree of an http git endpoint into `dest`, leaving
/// lib/ and bin/ untouched. the working tree materializes package tomls and
/// repo.db as blobs are fetched.
fn clone_pkg_only(url: &str, dest: &Path) -> XResult<()> {
    if dest.join(".git").is_dir() {
        return pull_pkg_only(url, dest);
    }
    if let Some(parent) = dest.parent()
        && !parent.is_dir()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }
    let status = Command::new("git")
        .args(["clone", "--depth", "1", "--filter=blob:none", "--sparse", url])
        .arg(dest)
        .status()
        .map_err(|e| format!("git is not available ({})", e))?;
    if !status.success() {
        return Err(format!("failed to fetch pkg/ from {}", url));
    }
    materialize_pkg(dest)
}

/// refresh an already-hooked http endpoint: reset pkg/ to origin and
/// materialize the catalog.
fn pull_pkg_only(_url: &str, dest: &Path) -> XResult<()> {
    cmd_output("git", &["-C", dest.to_str().unwrap_or("."), "fetch", "origin"])?;
    cmd_output(
        "git",
        &["-C", dest.to_str().unwrap_or("."), "reset", "--hard", "origin/HEAD"],
    )?;
    materialize_pkg(dest)
}

/// make sure the sparse checkout covers pkg/, then materialize its files
/// (asynchronous on-demand blob fetching handled by git's partial clone).
fn materialize_pkg(dest: &Path) -> XResult<()> {
    cmd_output(
        "git",
        &[
            "-C",
            dest.to_str().unwrap_or("."),
            "sparse-checkout",
            "set",
            crate::home::PKG_DIR,
        ],
    )?;
    cmd_output(
        "git",
        &["-C", dest.to_str().unwrap_or("."), "checkout-index", "-a", "-f"],
    )?;
    Ok(())
}

/// stage the actual `lib/` and `bin/` files of a single package out of an http
/// endpoint so `place_package` can copy them. returns the staging root (a tree
/// with `lib/` and `bin/`). only used at install time.
pub fn fetch_package_files(
    home: &Home,
    ep: &Endpoint,
    pkg: &Package,
) -> XResult<PathBuf> {
    let Endpoint::Http { url, .. } = ep else {
        return Ok(ep.pkg_dir(home).parent().unwrap_or_else(|| Path::new(".")).to_path_buf());
    };
    let root = ep.root(home);
    let work = root.join("work").join(&pkg.name);
    fs::create_dir_all(&work)
        .map_err(|e| format!("failed to create {}: {}", work.display(), e))?;
    if work.join(".git").is_dir() {
        pull_pkg_files(url, &work, pkg)?;
    } else {
        let status = Command::new("git")
            .args(["clone", "--depth", "1", "--filter=blob:none", "--sparse", url])
            .arg(&work)
            .status()
            .map_err(|e| format!("git is not available ({})", e))?;
        if !status.success() {
            return Err(format!("failed to fetch package files from {}", url));
        }
        materialize_pkg_files(&work, pkg)?;
    }
    Ok(work)
}

fn pull_pkg_files(_url: &str, work: &Path, pkg: &Package) -> XResult<()> {
    cmd_output("git", &["-C", work.to_str().unwrap_or("."), "fetch", "origin"])?;
    cmd_output(
        "git",
        &["-C", work.to_str().unwrap_or("."), "reset", "--hard", "origin/HEAD"],
    )?;
    materialize_pkg_files(work, pkg)
}

/// sparse-checkout the `lib/` and `bin/` trees for a package and materialize them
fn materialize_pkg_files(work: &Path, pkg: &Package) -> XResult<()> {
    cmd_output(
        "git",
        &["-C", work.to_str().unwrap_or("."), "sparse-checkout", "set", "lib", "bin"],
    )?;
    cmd_output(
        "git",
        &["-C", work.to_str().unwrap_or("."), "checkout-index", "-a", "-f"],
    )?;
    for (dir, file) in pkg.owned_files() {
        let path = work.join(dir).join(file);
        if !path.is_file() {
            return Err(format!(
                "missing {} in http package '{}'",
                path.display(),
                pkg.name
            ));
        }
    }
    Ok(())
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
        reg.add(Endpoint::Http {
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
            matches!(loaded.get("hub"), Some(Endpoint::Http { url, .. }) if url == "https://github.com/u/r.git")
        );
        assert!(
            matches!(loaded.get("local"), Some(Endpoint::Local { path, .. }) if path.as_path() == Path::new("/home/u/pkgs"))
        );

        let _ = fs::remove_dir_all(home.root());
    }

    #[test]
    fn add_rejects_duplicates() {
        let mut reg = EndpointRegistry::default();
        reg.add(Endpoint::Http {
            name: "hub".into(),
            url: "a".into(),
        })
        .unwrap();
        assert!(
            reg.add(Endpoint::Http {
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
                .is_http()
        );
        assert!(!adhoc_endpoint("x", "/tmp/somewhere").unwrap().is_http());
    }
}
