/// repo.db — a per-source plain-text catalog of package manifests.
///
/// `repo.db` lives in a source's `pkg/` directory and lists, one per line,
/// the package manifest filenames (e.g. `printc.toml`) that belong to that
/// source. It is kept for every kind of source:
///
/// - the installed tree: `~/.xeon/pkg/repo.db`
/// - a local endpoint tree: `<path>/pkg/repo.db`
/// - an http endpoint: `~/.xeon/cache/<name>/pkg/repo.db`
///
/// This lets xeon know which packages a source lists without having to scan
/// the whole tree.
use std::fs;
use std::path::Path;

use crate::home::XResult;

pub const REPO_DB: &str = "repo.db";

/// read the package manifest filenames listed in a `pkg/` directory's repo.db
pub fn read(pkg_dir: &Path) -> Vec<String> {
    let path = pkg_dir.join(REPO_DB);
    match fs::read_to_string(&path) {
        Ok(source) => source
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// write a repo.db into a `pkg/` directory, one manifest filename per line
pub fn write(pkg_dir: &Path, names: &[String]) -> XResult<()> {
    fs::create_dir_all(pkg_dir)
        .map_err(|e| format!("failed to create {}: {}", pkg_dir.display(), e))?;
    let mut body = String::new();
    for name in names {
        body.push_str(name);
        body.push('\n');
    }
    fs::write(pkg_dir.join(REPO_DB), body)
        .map_err(|e| format!("failed to write {}/repo.db: {}", pkg_dir.display(), e))
}

/// build a repo.db from the on-disk `pkg/*.toml` files in a directory
pub fn index(pkg_dir: &Path) -> XResult<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(pkg_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml")
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// return the listed manifest names, generating repo.db from disk if missing
pub fn ensure(pkg_dir: &Path) -> XResult<Vec<String>> {
    let names = read(pkg_dir);
    if !names.is_empty() {
        return Ok(names);
    }
    let names = index(pkg_dir)?;
    write(pkg_dir, &names)?;
    Ok(names)
}

/// add a manifest filename to a repo.db (without duplicating)
pub fn add(pkg_dir: &Path, name: &str) -> XResult<()> {
    let mut names = read(pkg_dir);
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
        names.sort();
        write(pkg_dir, &names)?;
    }
    Ok(())
}

/// remove a manifest filename from a repo.db
pub fn remove(pkg_dir: &Path, name: &str) -> XResult<()> {
    let names = read(pkg_dir);
    let kept: Vec<String> = names
        .into_iter()
        .filter(|n| n != name)
        .collect();
    write(pkg_dir, &kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir() -> std::path::PathBuf {
        env::temp_dir().join(format!(
            "xeon-repo-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = tmp_dir().join("pkg");
        write(&dir, &["a.toml".into(), "b.toml".into()]).unwrap();
        assert_eq!(read(&dir), vec!["a.toml", "b.toml"]);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn index_finds_tomls_only() {
        let dir = tmp_dir().join("pkg");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("printc.toml"), "name = \"printc\"").unwrap();
        fs::write(dir.join("notes.txt"), "x").unwrap();
        let names = index(&dir).unwrap();
        assert_eq!(names, vec!["printc.toml"]);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn add_and_remove() {
        let dir = tmp_dir().join("pkg");
        add(&dir, "a.toml").unwrap();
        add(&dir, "a.toml").unwrap(); // no dup
        add(&dir, "b.toml").unwrap();
        assert_eq!(read(&dir), vec!["a.toml", "b.toml"]);
        remove(&dir, "a.toml").unwrap();
        assert_eq!(read(&dir), vec!["b.toml"]);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }
}
