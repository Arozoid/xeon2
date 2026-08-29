// host architecture / platform detection
//
// used so xeon can report (and, when installing prebuilt binaries, require)
// the architecture of the machine it is running on. arch names match the
// `<arch>-<platform>` filename convention used by the pkgxeo build site.

use std::process::Command;

/// normalized CPU architecture: `x86_64`, `arm64`, or the raw `uname -m` value
pub fn host_arch() -> String {
    let raw = uname_m().unwrap_or_else(|| "unknown".to_string());
    normalize_arch(&raw)
}

/// normalized operating system: `linux`, `windows`, `macos`, or `unknown`
pub fn host_platform() -> String {
    match std::env::consts::OS {
        "linux" => "linux".to_string(),
        "windows" => "windows".to_string(),
        "macos" => "macos".to_string(),
        other => other.to_string(),
    }
}

/// rust target triple for the host (e.g. `x86_64-unknown-linux-gnu`)
pub fn host_triple() -> String {
    let arch = host_arch();
    let os = std::env::consts::OS;
    let env = std::env::consts::ARCH;
    format!("{arch}-{os}-{env}")
}

/// the `<arch>-<platform>` key used in prebuilt binary filenames
pub fn arch_platform() -> String {
    format!("{}-{}", host_arch(), host_platform())
}

/// `x86_64` -> `x86_64`, `aarch64`/`arm64` -> `arm64`
fn normalize_arch(m: &str) -> String {
    match m {
        "x86_64" | "amd64" => "x86_64".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

fn uname_m() -> Option<String> {
    let out = Command::new("uname").arg("-m").output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_known_arches() {
        assert_eq!(normalize_arch("x86_64"), "x86_64");
        assert_eq!(normalize_arch("amd64"), "x86_64");
        assert_eq!(normalize_arch("aarch64"), "arm64");
        assert_eq!(normalize_arch("arm64"), "arm64");
    }

    #[test]
    fn platform_is_a_known_value() {
        let p = host_platform();
        assert!(
            matches!(p.as_str(), "linux" | "windows" | "macos" | "unknown" | "freebsd" | "netbsd" | "openbsd" | "android" | "ios"),
            "unexpected platform {p}"
        );
    }
}
