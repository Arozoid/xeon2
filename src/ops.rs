/// package manager operations: install, remove, list, search, info,
/// update, upgrade, new, build, bootstrap, doctor
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::archive;
use crate::endpoints::{self, Endpoint, EndpointRegistry};
use crate::home::{Home, XResult};
use crate::model::{Package, is_valid_name};
use crate::paths::{cache_name, child_path};
use crate::ui;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// a resolved source tree plus the packages it carries
#[derive(Debug)]
pub struct Resolved {
    pub source_label: String,
    pub source_root: PathBuf,
    pub pkg: Package,
}

//============================================================//
//------------------------- install --------------------------//
//============================================================//

pub fn install(
    home: &Home,
    reg: &EndpointRegistry,
    spec: &str,
    force: bool,
    dry_run: bool,
) -> XResult<()> {
    let resolved = resolve_spec(home, reg, spec)?;
    let mut stack: Vec<String> = Vec::new();
    let mut report: Vec<String> = Vec::new();
    for r in resolved {
        install_resolved(home, reg, r, force, dry_run, &mut stack, &mut report)?;
    }
    for line in report {
        ui::ok(line);
    }
    Ok(())
}

fn resolve_spec(home: &Home, reg: &EndpointRegistry, spec: &str) -> XResult<Vec<Resolved>> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return Err("no package given".into());
    }

    // 1. local path / archive on disk
    let as_path = PathBuf::from(trimmed);
    if as_path.exists() {
        if as_path.is_dir() {
            let abs = fs::canonicalize(&as_path).unwrap_or(as_path);
            return packages_from_root(&abs, &format!("path:{}", abs.display()), None);
        }
        if archive::is_archive(&as_path) {
            let abs = fs::canonicalize(&as_path).unwrap_or(as_path);
            let dest = dl_dir(home, &format!("path:{}", abs.display()));
            if !dest.is_dir() {
                archive::extract(&abs, &dest)?;
            }
            return packages_from_root(&dest, &format!("path:{}", abs.display()), None);
        }
        return Err(format!(
            "'{}' exists but is not a package directory or archive",
            trimmed
        ));
    }

    // 2. bare http url, with optional #pkg selector — treat as an ad-hoc
    //    http endpoint, hook its pkg/ catalog, then install from it.
    let (url_part, selector) = match trimmed.split_once('#') {
        Some((url, sel)) => (url, Some(sel.to_string())),
        None => (trimmed, None),
    };
    if crate::paths::looks_like_http_url(url_part) {
        let ep = endpoints::adhoc_endpoint(url_part, url_part)?;
        return resolve_from_endpoint(home, &ep, selector.as_deref());
    }

    // 3. <endpoint>/<package>
    if let Some((ep_name, pkg_name)) = trimmed.split_once('/') {
        if let Some(ep) = reg.get(ep_name) {
            return resolve_from_endpoint(home, ep, Some(pkg_name));
        }
        return Err(format!("unknown endpoint or local path: '{}'", trimmed));
    }

    // 4. bare package name — search every endpoint, hooking http ones for the
    //    catalog so their tomls are available at install time.
    let mut hits: Vec<Resolved> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for ep in &reg.endpoint {
        match resolve_from_endpoint(home, ep, Some(trimmed)) {
            Ok(mut resolved) => hits.append(&mut resolved),
            Err(e) => {
                if ep.is_http() {
                    warnings.push(format!(
                        "endpoint '{}' unavailable ({})",
                        ep.name(),
                        e
                    ));
                }
            }
        }
    }
    if hits.is_empty() {
        let mut msg = format!("package '{}' not found in any endpoint", trimmed);
        if !warnings.is_empty() {
            msg.push_str(&format!("; {}", warnings.join("; ")));
        }
        msg.push_str("\n  tip: add an endpoint with `xeon endpoint add <name> <path-or-url>`");
        return Err(msg);
    }
    if hits.len() > 1 {
        ui::warn(format!(
            "package '{}' found in multiple endpoints; using '{}'",
            trimmed, hits[0].source_label
        ));
    }
    Ok(vec![hits.remove(0)])
}

/// hook an endpoint (fetching its catalog if needed), locate one package, and
/// produce a Resolved with a usable source_root (full tree for local, staged
/// lib/bin tree fetched at install time for http).
fn resolve_from_endpoint(
    home: &Home,
    ep: &Endpoint,
    selector: Option<&str>,
) -> XResult<Vec<Resolved>> {
    let pkg_dir = ep.hook(home)?;

    // if no selector, return every package in the endpoint's catalog
    if selector.is_none() {
        let mut out = Vec::new();
        for (_toml, pkg) in endpoints::scan_dir(&pkg_dir)? {
            let source_root = source_root_for(home, ep, &pkg)?;
            out.push(Resolved {
                source_label: ep.name().to_string(),
                source_root,
                pkg,
            });
        }
        if out.is_empty() {
            return Err(format!("no packages found in endpoint '{}'", ep.name()));
        }
        return Ok(out);
    }

    let name = selector.unwrap();
    let toml = pkg_dir.join(format!("{name}.toml"));
    if !toml.is_file() {
        return Err(format!("package '{}' not found in endpoint '{}'", name, ep.name()));
    }
    let pkg = Package::read(&toml)?;
    let source_root = source_root_for(home, ep, &pkg)?;
    Ok(vec![Resolved {
        source_label: ep.name().to_string(),
        source_root,
        pkg,
    }])
}

/// the file tree to copy from when installing `pkg` from `ep`: the whole
/// endpoint tree for local endpoints, a staged lib/bin tree for http ones.
fn source_root_for(home: &Home, ep: &Endpoint, pkg: &Package) -> XResult<PathBuf> {
    if ep.is_http() {
        endpoints::fetch_package_files(home, ep, pkg)
    } else {
        Ok(ep.pkg_dir(home).parent().unwrap_or_else(|| Path::new(".")).to_path_buf())
    }
}

fn packages_from_root(root: &Path, label: &str, selector: Option<&str>) -> XResult<Vec<Resolved>> {
    let found = endpoints::scan_root(root)?;
    let mut out: Vec<Resolved> = Vec::new();
    for (_toml, pkg) in found {
        if let Some(sel) = selector
            && pkg.name != sel
        {
            continue;
        }
        out.push(Resolved {
            source_label: label.to_string(),
            source_root: root.to_path_buf(),
            pkg,
        });
    }
    if out.is_empty() {
        match selector {
            Some(sel) => return Err(format!("package '{}' not found in {}", sel, label)),
            None => return Err(format!("no packages found in {}", label)),
        }
    }
    Ok(out)
}

fn install_resolved(
    home: &Home,
    reg: &EndpointRegistry,
    r: Resolved,
    force: bool,
    dry_run: bool,
    stack: &mut Vec<String>,
    report: &mut Vec<String>,
) -> XResult<()> {
    let name = r.pkg.name.clone();
    let installed = home.pkg_dir().join(format!("{name}.toml"));
    if installed.exists() && !force {
        if let Ok(prev) = Package::read(&installed)
            && prev.version == r.pkg.version
            && prev.origin == Some(r.source_label.clone())
        {
            report.push(format!(
                "{} {} already installed (v{})",
                "∙".cyan(),
                name.cyan(),
                prev.version
            ));
            return Ok(());
        }
        return Err(format!(
            "'{}' is already installed (use `xeon upgrade {}` or `xeon install {} --force`)",
            name, name, name
        ));
    }

    install_dependencies(home, reg, &r.pkg.depends, force, dry_run, stack, report)?;

    let (libs, bins) = place_package(home, &r.pkg, &r.source_root, &r.source_label, dry_run)?;
    if !dry_run {
        crate::repo::add(&home.pkg_dir(), &name)?;
    }
    let action = if dry_run {
        "would install".yellow().to_string()
    } else {
        "installed".green().to_string()
    };
    report.push(format!(
        "{} {} {} ({} lib, {} bin) from {}",
        action,
        r.pkg.name.cyan(),
        r.pkg.version,
        libs,
        bins,
        r.source_label
    ));
    Ok(())
}

fn install_dependencies(
    home: &Home,
    reg: &EndpointRegistry,
    deps: &[String],
    force: bool,
    dry_run: bool,
    stack: &mut Vec<String>,
    report: &mut Vec<String>,
) -> XResult<()> {
    for dep in deps {
        let dep_toml = home.pkg_dir().join(format!("{dep}.toml"));
        if dep_toml.is_file() {
            continue; // already satisfied
        }
        if stack.contains(dep) {
            return Err(format!("dependency cycle involving '{}'", dep));
        }
        stack.push(dep.clone());

        let mut found: Option<Resolved> = None;
        for ep in &reg.endpoint {
            match resolve_from_endpoint(home, ep, Some(dep)) {
                Ok(mut resolved) if !resolved.is_empty() => {
                    found = resolved.pop();
                    break;
                }
                _ => {}
            }
        }
        let mut r = match found {
            Some(r) => r,
            None => return Err(format!("dependency '{}' not found in any endpoint", dep)),
        };
        r.pkg.origin = Some(format!("endpoint:{}", r.source_label));
        install_resolved(home, reg, r, force, dry_run, stack, report)?;
        stack.pop();
    }
    Ok(())
}

/// copy a package's owned files into the install tree and write its manifest
fn place_package(
    home: &Home,
    pkg: &Package,
    source_root: &Path,
    origin_label: &str,
    dry_run: bool,
) -> XResult<(usize, usize)> {
    for (dir, file) in pkg.owned_files() {
        let container = source_root.join(dir);
        let from = child_path(&container, file)?;
        let to_container = home_dir_for(home, dir);
        let to = child_path(&to_container, file)?;
        if dry_run {
            ui::info(format!("copy {} -> {}", from.display(), to.display()));
            continue;
        }
        if !from.is_file() {
            return Err(format!(
                "missing file in package '{}': {}",
                pkg.name,
                from.display()
            ));
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
        fs::copy(&from, &to).map_err(|e| {
            format!(
                "failed to copy {} -> {}: {}",
                from.display(),
                to.display(),
                e
            )
        })?;
        if dir == "bin" {
            make_executable(&to)?;
        }
    }

    let mut manifest = pkg.clone();
    manifest.origin = Some(origin_label.to_string());
    if !dry_run {
        let toml_path = home.pkg_dir().join(format!("{}.toml", pkg.name));
        if let Some(parent) = toml_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
        manifest.write(&toml_path)?;
    }
    Ok((pkg.lib.len(), pkg.bin.len()))
}

fn home_dir_for(home: &Home, dir: &str) -> PathBuf {
    match dir {
        "bin" => home.bin_dir(),
        _ => home.lib_dir(),
    }
}

#[cfg(unix)]
pub fn make_executable(path: &Path) -> XResult<()> {
    let mut perms = fs::metadata(path)
        .map_err(|e| format!("failed to stat {}: {}", path.display(), e))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("failed to chmod {}: {}", path.display(), e))
}

//============================================================//
//------------------------- remove ---------------------------//
//============================================================//

pub fn remove(home: &Home, name: &str, yes: bool) -> XResult<()> {
    let toml = home.pkg_dir().join(format!("{}.toml", name));
    if !toml.is_file() {
        return Err(format!("package '{}' is not installed", name));
    }
    let pkg = Package::read(&toml)?;

    let owned = pkg.owned_files();
    if !yes && !ui::confirm(&format!("remove {} ({} files)?", name, owned.len())) {
        ui::warn("aborted");
        return Ok(());
    }

    let mut removed = 0;
    for (dir, file) in &owned {
        let base = home_dir_for(home, dir);
        let path = child_path(&base, file)?;
        if path.is_file() {
            fs::remove_file(&path)
                .map_err(|e| format!("failed to remove {}: {}", path.display(), e))?;
            ui::info(format!("removed {}", path.display()));
            removed += 1;
        }
    }
    fs::remove_file(&toml).map_err(|e| format!("failed to remove {}: {}", toml.display(), e))?;
    crate::repo::remove(&home.pkg_dir(), name)?;
    ui::ok(format!(
        "removed {} {} ({} files)",
        name, pkg.version, removed
    ));
    Ok(())
}

//============================================================//
//-------------------------- list ----------------------------//
//============================================================//

pub fn list(home: &Home) -> XResult<()> {
    let found = endpoints::scan_root(home.root())?;
    if found.is_empty() {
        ui::info("no packages installed yet — try `xeon install <name>`");
        return Ok(());
    }
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (_toml, pkg) in found {
        let deps = if pkg.depends.is_empty() {
            "".to_string()
        } else {
            pkg.depends.join(", ")
        };
        rows.push(vec![
            pkg.name,
            pkg.version,
            pkg.lib.len().to_string(),
            pkg.bin.len().to_string(),
            deps,
            pkg.description,
        ]);
    }
    ui::table(
        &["package", "version", "lib", "bin", "depends", "description"],
        &rows,
    );
    Ok(())
}

//============================================================//
//------------------------- search ---------------------------//
//============================================================//

pub fn search(home: &Home, reg: &EndpointRegistry, query: &str) -> XResult<()> {
    if reg.endpoint.is_empty() {
        return Err(
            "no endpoints configured — run `xeon endpoint add <name> <path-or-url>` first".into(),
        );
    }
    let q = query.to_ascii_lowercase();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for ep in &reg.endpoint {
        let pkgs = match ep.catalog(home) {
            Ok(list) => list,
            Err(e) => {
                ui::warn(format!("endpoint '{}' unavailable ({})", ep.name(), e));
                continue;
            }
        };
        for (_toml, pkg) in pkgs {
            let haystack = format!(
                "{} {}",
                pkg.name.to_ascii_lowercase(),
                pkg.description.to_ascii_lowercase()
            );
            if haystack.contains(&q) {
                rows.push(vec![
                    pkg.name,
                    pkg.version,
                    ep.name().to_string(),
                    pkg.description,
                ]);
            }
        }
    }
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    if rows.is_empty() {
        ui::info(format!("no packages match '{}'", query));
        return Ok(());
    }
    ui::table(&["package", "version", "endpoint", "description"], &rows);
    Ok(())
}

//============================================================//
//-------------------------- info ----------------------------//
//============================================================//

pub fn info(home: &Home, reg: &EndpointRegistry, name: &str) -> XResult<()> {
    let mut label = "install tree".to_string();
    let installed = home.pkg_dir().join(format!("{name}.toml"));
    let pkg = if installed.is_file() {
        Package::read(&installed)?
    } else {
        let hits = crate::endpoints::find_package(home, reg, name)?;
        label = hits[0].0.clone();
        hits[0].2.clone()
    };

    println!("{} {}", "package".cyan().bold(), pkg.name.cyan());
    println!("  {}: {}", "version".bold(), pkg.version);
    println!("  {}: {}", "description".bold(), pkg.description);
    println!("  {}: {}", "source".bold(), label);
    if !pkg.depends.is_empty() {
        println!("  {}: {}", "depends".bold(), pkg.depends.join(", "));
    }
    if let Some(origin) = &pkg.origin {
        println!("  {}: {}", "origin".bold(), origin);
    }
    if !pkg.lib.is_empty() {
        println!("  {}:", "lib".bold());
        for f in &pkg.lib {
            println!("    {}", f.cyan());
        }
    }
    if !pkg.bin.is_empty() {
        println!("  {}:", "bin".bold());
        for f in &pkg.bin {
            println!("    {}", f.cyan());
        }
    }
    Ok(())
}

//============================================================//
//------------------------- update ---------------------------//
//============================================================//

pub fn update(home: &Home, reg: &EndpointRegistry) -> XResult<()> {
    if reg.endpoint.is_empty() {
        ui::warn("no endpoints configured — nothing to refresh");
        return Ok(());
    }
    let mut total = 0;
    for ep in &reg.endpoint {
        match ep.refresh(home) {
            Ok(pkg_dir) => {
                let count = endpoints::scan_dir(&pkg_dir)?.len();
                total += count;
                ui::ok(format!(
                    "refreshed endpoint '{}' ({} packages)",
                    ep.name().cyan(),
                    count
                ));
            }
            Err(e) => {
                ui::err(format!("failed to refresh endpoint '{}': {}", ep.name(), e));
            }
        }
    }
    ui::info(format!("{} packages available across endpoints", total));
    Ok(())
}

//============================================================//
//------------------------- upgrade --------------------------//
//============================================================//

pub fn upgrade(
    home: &Home,
    reg: &EndpointRegistry,
    only: Option<Vec<String>>,
    force: bool,
    dry_run: bool,
) -> XResult<()> {
    let installed = endpoints::scan_root(home.root())?;
    if installed.is_empty() {
        ui::info("nothing installed — nothing to upgrade");
        return Ok(());
    }

    let mut done = 0;
    let mut skipped = 0;
    for (_toml, pkg) in installed {
        if let Some(only) = &only
            && !only.contains(&pkg.name)
        {
            continue;
        }

        let origin = match &pkg.origin {
            Some(o) if !o.is_empty() => o.clone(),
            _ => {
                ui::warn(format!(
                    "{} has no recorded origin — skipping",
                    pkg.name.cyan()
                ));
                skipped += 1;
                continue;
            }
        };

        let (label, catalog_dir, http_url) = match origin_to_source(home, reg, &origin) {
            Ok(Some(src)) => src,
            Ok(None) => {
                ui::warn(format!(
                    "{} origin '{}' is unreachable — skipping",
                    pkg.name.cyan(),
                    origin
                ));
                skipped += 1;
                continue;
            }
            Err(e) => {
                ui::err(format!("{}: {}", pkg.name, e));
                skipped += 1;
                continue;
            }
        };

        let candidate_toml = catalog_dir.join(format!("{}.toml", pkg.name));
        let candidate = match Package::read(&candidate_toml) {
            Ok(c) => c,
            Err(_) => {
                ui::warn(format!(
                    "{} not found at origin '{}' — skipping",
                    pkg.name.cyan(),
                    origin
                ));
                skipped += 1;
                continue;
            }
        };

        if candidate.version == pkg.version && !force {
            ui::info(format!("{} {} is up to date", pkg.name.cyan(), pkg.version));
            continue;
        }

        let source_root = match http_url {
            Some(url) => {
                let ep = endpoints::adhoc_endpoint(&origin, &url)?;
                endpoints::fetch_package_files(home, &ep, &candidate)?
            }
            None => catalog_dir
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf(),
        };

        let (libs, bins) = place_package(home, &candidate, &source_root, &label, dry_run)?;
        if dry_run {
            ui::info(format!(
                "would upgrade {} {} -> {} ({} lib, {} bin)",
                pkg.name.cyan(),
                pkg.version,
                candidate.version,
                libs,
                bins
            ));
        } else {
            ui::ok(format!(
                "upgraded {} {} -> {} ({} lib, {} bin)",
                pkg.name.cyan(),
                pkg.version,
                candidate.version,
                libs,
                bins
            ));
        }
        done += 1;
    }

    if done == 0 && skipped > 0 {
        ui::info(format!(
            "{} packages skipped (unreachable origins)",
            skipped
        ));
    }
    Ok(())
}

/// turn a recorded origin string into (label, catalog_dir, http_url_or_none) —
/// when the origin is an http endpoint the files are staged at upgrade time
/// from `http_url`.
fn origin_to_source(
    home: &Home,
    reg: &EndpointRegistry,
    origin: &str,
) -> XResult<Option<(String, PathBuf, Option<String>)>> {
    if let Some(ep) = reg.get(origin) {
        let pkg_dir = ep.hook(home)?;
        let url = match ep {
            endpoints::Endpoint::Http { url, .. } => Some(url.clone()),
            endpoints::Endpoint::Local { .. } => None,
        };
        return Ok(Some((origin.to_string(), pkg_dir, url)));
    }
    if let Some(rest) = origin.strip_prefix("path:") {
        let path = PathBuf::from(rest);
        if archive::is_archive(&path) {
            let dest = dl_dir(home, &format!("path:{}", rest));
            if !dest.is_dir() {
                archive::extract(&path, &dest)?;
            }
            return Ok(Some((
                format!("path:{}", rest),
                dest.join(crate::home::PKG_DIR),
                None,
            )));
        }
        if path.is_dir() {
            return Ok(Some((
                format!("path:{}", rest),
                path.join(crate::home::PKG_DIR),
                None,
            )));
        }
        return Ok(None);
    }
    if let Some(url) = origin.strip_prefix("http:") {
        let ep = endpoints::adhoc_endpoint(origin, url)?;
        let pkg_dir = ep.hook(home)?;
        return Ok(Some((origin.to_string(), pkg_dir, Some(url.to_string()))));
    }
    Ok(None)
}

//============================================================//
//-------------------------- new -----------------------------//
//============================================================//

pub fn new_package(name: &str, dir: Option<&Path>) -> XResult<()> {
    if !is_valid_name(name) {
        return Err(format!("'{}' is not a valid package name", name));
    }
    let base = match dir {
        Some(d) => d.join(name),
        None => PathBuf::from(name),
    };
    if base.exists() {
        return Err(format!("{} already exists", base.display()));
    }

    for sub in ["pkg", "lib", "bin"] {
        fs::create_dir_all(base.join(sub))
            .map_err(|e| format!("failed to create {}: {}", base.join(sub).display(), e))?;
    }

    let pkg = Package {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: format!("a {} package for .xeo", name),
        depends: Vec::new(),
        lib: vec![format!("{}.xeo", name)],
        bin: Vec::new(),
        origin: None,
    };
    pkg.write(&base.join("pkg").join(format!("{name}.toml")))?;

    let lib_template = format!(
        "#!/usr/bin/env xeo\n-- {name} — a .xeo library\n-- load with: use {name}\n\nfunc {name}_version\n    print \"{name} {}\"\nend\n",
        pkg.version
    );
    fs::write(base.join("lib").join(format!("{name}.xeo")), lib_template)
        .map_err(|e| format!("failed to write library: {}", e))?;

    ui::ok(format!(
        "created package '{}' at {}",
        name.cyan(),
        base.display()
    ));
    ui::info("tree");
    ui::info(format!("  {}/pkg/{}.toml", base.display(), name));
    ui::info(format!("  {}/lib/{}.xeo", base.display(), name));
    ui::info(format!("  {}/bin/", base.display()));
    ui::info(format!("install it: xeon install {}", base.display()));
    Ok(())
}

//============================================================//
//-------------------------- build ---------------------------//
//============================================================//

pub fn build_package(_home: &Home, dir: &Path, out: Option<&Path>) -> XResult<()> {
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let found = endpoints::scan_root(dir)?;
    if found.is_empty() {
        return Err(format!(
            "no packages found in {} (expected {}/pkg/*.toml)",
            dir.display(),
            dir.display()
        ));
    }

    // validate every package references existing files
    for (toml, pkg) in &found {
        for (kind, file) in pkg.owned_files() {
            let from = child_path(&dir.join(kind), file)?;
            if !from.is_file() {
                return Err(format!(
                    "{} declares {} but {} is missing",
                    toml.display(),
                    file,
                    from.display()
                ));
            }
        }
    }

    let (name, version) = {
        let p = &found[0].1;
        (p.name.clone(), p.version.clone())
    };
    let archive_path = match out {
        Some(p) => {
            if p.is_dir() {
                p.join(format!("{}-{}.tar.gz", name, version))
            } else {
                p.to_path_buf()
            }
        }
        None => PathBuf::from(format!("{}-{}.tar.gz", name, version)),
    };

    archive::build(dir, &archive_path)?;
    ui::ok(format!(
        "built {} {} -> {} ({} packages)",
        name.cyan(),
        version,
        archive_path.display(),
        found.len()
    ));
    ui::info(format!(
        "install it: xeon install {}",
        archive_path.display()
    ));
    ui::info(format!(
        "share it:  xeon endpoint add <name> {}",
        archive_path.display()
    ));
    Ok(())
}

//============================================================//
//------------------------- bootstrap ------------------------//
//============================================================//

pub fn bootstrap(home: &Home, source: &Path) -> XResult<()> {
    if !source.is_file() {
        return Err(format!(
            "cannot find interpreter binary: {}",
            source.display()
        ));
    }
    home.ensure()?;
    let dest = home.bin_dir().join("xeo");
    fs::copy(source, &dest).map_err(|e| {
        format!(
            "failed to copy {} -> {}: {}",
            source.display(),
            dest.display(),
            e
        )
    })?;
    #[cfg(unix)]
    make_executable(&dest)?;
    ui::ok(format!(
        "installed xeo interpreter at {} ({} bytes)",
        dest.display(),
        fs::metadata(&dest).map(|m| m.len()).unwrap_or(0)
    ));
    Ok(())
}

//============================================================//
//------------------------- doctor ---------------------------//
//============================================================//

pub fn doctor(home: &Home, reg: &EndpointRegistry) -> XResult<()> {
    let mut healthy = true;

    println!("{}", "xeon doctor".cyan().bold());
    println!("  {}: {}", "home".bold(), home.root().display());

    if home.is_initialized() {
        ui::ok("xeon home initialized");
    } else {
        healthy = false;
        ui::err("xeon home not initialized — run `xeon init`");
    }

    for dir in [home.lib_dir(), home.bin_dir(), home.pkg_dir()] {
        if dir.is_dir() {
            ui::ok(format!("{} exists", dir.display()));
        } else {
            healthy = false;
            ui::err(format!("{} missing", dir.display()));
        }
    }

    match Command::new("git").arg("--version").output() {
        Ok(_) => ui::ok("git available"),
        Err(_) => ui::warn("git not found — http endpoints will not work"),
    }

    let xeo_bin = home.bin_dir().join("xeo");
    if xeo_bin.is_file() {
        let size = fs::metadata(&xeo_bin).map(|m| m.len()).unwrap_or(0);
        ui::ok(format!("xeo interpreter present ({} bytes)", size));
    } else {
        ui::warn("xeo interpreter not installed — run `xeon bootstrap <path-to-xeo-binary>`");
    }

    if reg.endpoint.is_empty() {
        ui::warn(
            "no endpoints configured — install from paths/urls directly or run `xeon endpoint add`",
        );
    } else {
        ui::ok(format!("{} endpoints configured", reg.endpoint.len()));
        for ep in &reg.endpoint {
            ui::info(format!("  {} ({})", ep.name(), endpoint_kind(ep)));
        }
    }

    let installed = endpoints::scan_root(home.root())?;
    ui::info(format!("{} packages installed", installed.len()));
    let mut missing = 0;
    for (_toml, pkg) in &installed {
        for (dir, file) in pkg.owned_files() {
            let dest = child_path(&home_dir_for(home, dir), file)?;
            if !dest.is_file() {
                missing += 1;
                ui::err(format!("  {}: {} missing", pkg.name, dest.display()));
            }
        }
    }
    if missing == 0 {
        ui::ok("all installed package files present");
    } else {
        healthy = false;
        ui::err(format!(
            "{} installed files missing — reinstall affected packages",
            missing
        ));
    }

    if !healthy {
        return Err("xeon doctor found problems".into());
    }
    ui::ok("environment looks good");
    Ok(())
}

fn endpoint_kind(ep: &Endpoint) -> &'static str {
    if ep.is_http() { "http" } else { "local" }
}

//============================================================//
//-------------------------- init ----------------------------//
//============================================================//

pub fn init(home: &Home) -> XResult<()> {
    if home.is_initialized() {
        ui::warn("xeon is already initialized");
        return Ok(());
    }
    home.init()?;
    ui::ok(format!("initialized xeon at {}", home.root().display()));
    ui::info("layout:");
    ui::info(format!(
        "  {}/lib/   .xeo library modules",
        home.root().display()
    ));
    ui::info(format!(
        "  {}/bin/   extension executables",
        home.root().display()
    ));
    ui::info(format!(
        "  {}/pkg/   package metadata",
        home.root().display()
    ));
    ui::info("add a source: xeon endpoint add <name> <path-or-url>");
    Ok(())
}

//============================================================//
//------------------------- clean ----------------------------//
//============================================================//

pub fn clean(home: &Home) -> XResult<()> {
    let dl = dl_root(home);
    if !dl.is_dir() {
        ui::info("download cache is already empty");
        return Ok(());
    }
    let mut freed = 0;
    if let Ok(read) = fs::read_dir(&dl) {
        for entry in read.flatten() {
            let path = entry.path();
            if let Ok(meta) = fs::metadata(&path) {
                freed += meta.len();
            }
            let _ = fs::remove_dir_all(&path);
            let _ = fs::remove_file(&path);
        }
    }
    let _ = fs::remove_dir_all(&dl);
    ui::ok(format!(
        "emptied download cache {} (~{} freed)",
        dl.display(),
        human_bytes(freed)
    ));
    Ok(())
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= MB {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes as f64 >= KB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{} B", bytes)
    }
}

//============================================================//
//------------------------- internals ------------------------//
//============================================================//

/// root of the download cache (`~/.xeon/cache/dl`)
fn dl_root(home: &Home) -> PathBuf {
    home.cache_dir().join("dl")
}

/// cache dir for downloaded (url/archive) sources
fn dl_dir(home: &Home, key: &str) -> PathBuf {
    dl_root(home).join(cache_name(key))
}
