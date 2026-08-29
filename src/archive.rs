/// archive support: packages can be distributed as `<name>-<version>.tar.gz`
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::home::XResult;

pub fn is_archive(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("tgz") => true,
        Some("tar") => true,
        Some("gz") => path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".tar.gz")),
        _ => false,
    }
}

/// tar.gz a package tree (its `lib/`, `bin/`, `pkg/` members only) into `out`
pub fn build(pkg_dir: &Path, out: &Path) -> XResult<()> {
    let file =
        File::create(out).map_err(|e| format!("failed to create {}: {}", out.display(), e))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(encoder);

    for member in ["lib", "bin", crate::home::PKG_DIR] {
        let member_path = pkg_dir.join(member);
        if member_path.is_dir() {
            tar.append_dir_all(member, &member_path)
                .map_err(|e| format!("failed to archive {}: {}", member_path.display(), e))?;
        }
    }
    let encoder = tar
        .into_inner()
        .map_err(|e| format!("failed to finish archive: {}", e))?;
    let mut file = encoder
        .finish()
        .map_err(|e| format!("failed to finish archive: {}", e))?;
    let _ = file.flush();
    Ok(())
}

/// extract a `.tar.gz` / `.tgz` / `.tar` archive into `dest`
pub fn extract(archive: &Path, dest: &Path) -> XResult<()> {
    let file =
        File::open(archive).map_err(|e| format!("failed to open {}: {}", archive.display(), e))?;

    let reader: Box<dyn Read> = match archive.extension().and_then(|e| e.to_str()) {
        Some("tgz") => Box::new(GzDecoder::new(file)),
        Some("gz") => Box::new(GzDecoder::new(file)),
        _ => Box::new(file),
    };

    let mut tar = tar::Archive::new(reader);
    tar.set_preserve_permissions(true);
    tar.unpack(dest)
        .map_err(|e| format!("failed to extract {}: {}", archive.display(), e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (temp::TempDir, std::path::PathBuf) {
        let dir = temp::TempDir::new().unwrap();
        let root = dir.path().join("pkg");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("pkg")).unwrap();
        fs::write(root.join("lib").join("printc.xeo"), "use printc\n").unwrap();
        fs::write(root.join("bin").join("printc"), "#!/bin/sh\n").unwrap();
        fs::write(
            root.join("pkg").join("printc.toml"),
            "name = \"printc\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        (dir, root)
    }

    #[test]
    fn build_and_extract_roundtrip() {
        let (_keep, root) = fixture();
        let out = std::env::temp_dir().join(format!("xeon-test-{}.tar.gz", std::process::id()));
        build(&root, &out).unwrap();

        let dest = std::env::temp_dir().join(format!("xeon-extract-{}", std::process::id()));
        extract(&out, &dest).unwrap();

        assert!(dest.join("lib").join("printc.xeo").exists());
        assert!(dest.join("bin").join("printc").exists());
        assert!(dest.join("pkg").join("printc.toml").exists());

        let _ = fs::remove_file(&out);
        let _ = fs::remove_dir_all(&dest);
    }

    mod temp {
        pub struct TempDir(std::path::PathBuf);
        impl TempDir {
            pub fn new() -> std::io::Result<Self> {
                let p = std::env::temp_dir().join(format!(
                    "xeon-temp-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
                std::fs::create_dir_all(&p)?;
                Ok(TempDir(p))
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
