mod archive;
mod endpoints;
mod home;
mod model;
mod ops;
mod paths;
mod ui;

use clap::{Parser, Subcommand};
use colored::*;
use std::path::PathBuf;

use home::Home;
use ui::err;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const ABOUT: &str = "xeon — the 'modern' package manager for .xeo";

#[derive(Parser)]
#[command(
    name = "xeon",
    version = VERSION,
    about = ABOUT,
    arg_required_else_help = true,
    after_help = "endpoints are trees sharing the xeon layout (lib/, bin/, pkg/).\ninstall from a local path, a .tar.gz archive, a git url, a named\nendpoint (endpoint/pkg), or a bare package name searched across endpoints."
)]
struct Cli {
    #[command(subcommand)]
    command: UserCommands,
}

#[derive(Subcommand)]
enum UserCommands {
    /// scaffold the ~/.xeon install tree and endpoints file
    Init,
    /// install a package from a path, archive, git url, endpoint, or name
    #[command(visible_alias = "add")]
    Install {
        /// package spec: name | endpoint/name | path | archive | git-url[#name]
        pkg: String,
        /// overwrite an already-installed package
        #[arg(short, long)]
        force: bool,
        /// show what would happen without touching the filesystem
        #[arg(short, long)]
        dry_run: bool,
    },
    /// remove an installed package and its files
    #[command(visible_alias = "rm")]
    Remove {
        pkg: String,
        /// skip the confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// list installed packages
    #[command(visible_alias = "ls")]
    List,
    /// search every endpoint for packages matching a query
    Search { query: String },
    /// show metadata for an installed package (or any endpoint package)
    Info { pkg: String },
    /// refresh git endpoints from their origin
    Update,
    /// upgrade installed packages from their recorded origins
    Upgrade {
        /// upgrade only this package (default: all)
        pkg: Option<String>,
        /// reinstall even when versions match
        #[arg(short, long)]
        force: bool,
        /// show what would happen without touching the filesystem
        #[arg(short, long)]
        dry_run: bool,
    },
    /// manage package endpoints
    Endpoint {
        #[command(subcommand)]
        cmd: EndpointCmd,
    },
    /// scaffold a new package tree that can be installed or shared
    New {
        /// package name
        name: String,
        /// parent directory for the new package (default: current dir)
        #[arg(short = 'd', long)]
        dir: Option<PathBuf>,
    },
    /// package a package tree (lib/bin/pkg) into a <name>-<version>.tar.gz
    Build {
        /// the package directory to build
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// output location (file, or directory to put <name>-<version>.tar.gz in)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// install the xeo interpreter binary into ~/.xeon/bin/xeo
    Bootstrap {
        /// path to a built xeo binary
        source: PathBuf,
    },
    /// basic diagnostics for this machine
    Doctor,
    /// empty the download cache (~/.xeon/cache/dl)
    Clean,
    /// print version
    Version,
}

#[derive(Subcommand)]
enum EndpointCmd {
    /// add an endpoint (git:// or path)
    Add { name: String, location: String },
    /// remove an endpoint by name
    #[command(visible_alias = "rm")]
    Remove { name: String },
    /// list configured endpoints
    List,
    /// show where a named endpoint resolves on disk
    Path { name: String },
}

fn main() {
    let cli = Cli::parse();

    let home = Home::resolve();
    let registry = match endpoints::EndpointRegistry::load(&home) {
        Ok(reg) => reg,
        Err(e) => {
            err(e);
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        UserCommands::Init => ops::init(&home),
        UserCommands::Install {
            pkg,
            force,
            dry_run,
        } => ops::install(&home, &registry, &pkg, force, dry_run),
        UserCommands::Remove { pkg, yes } => ops::remove(&home, &pkg, yes),
        UserCommands::List => ops::list(&home),
        UserCommands::Search { query } => ops::search(&home, &registry, &query),
        UserCommands::Info { pkg } => ops::info(&home, &registry, &pkg),
        UserCommands::Update => ops::update(&home, &registry),
        UserCommands::Upgrade {
            pkg,
            force,
            dry_run,
        } => ops::upgrade(&home, &registry, pkg.map(|p| vec![p]), force, dry_run),
        UserCommands::Endpoint { cmd } => handle_endpoint(&home, &registry, cmd),
        UserCommands::New { name, dir } => ops::new_package(&name, dir.as_deref()),
        UserCommands::Build { dir, out } => ops::build_package(&home, &dir, out.as_deref()),
        UserCommands::Bootstrap { source } => ops::bootstrap(&home, &source),
        UserCommands::Doctor => match ops::doctor(&home, &registry) {
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        },
        UserCommands::Clean => ops::clean(&home),
        UserCommands::Version => {
            println!("{}", "xeon — the .xeo package manager".green());
            println!("v{}", VERSION);
            Ok(())
        }
    };

    if let Err(e) = result {
        err(e);
        std::process::exit(1);
    }
}

fn handle_endpoint(
    home: &Home,
    registry: &endpoints::EndpointRegistry,
    cmd: EndpointCmd,
) -> home::XResult<()> {
    let mut reg = registry.clone();
    match cmd {
        EndpointCmd::Add { name, location } => {
            let ep = endpoints::adhoc_endpoint(&name, &location)?;
            reg.add(ep.clone())?;
            reg.save(home)?;
            let root = ep.ensure(home)?;
            let count = endpoints::scan_root(&root)?.len();
            let kind = if ep.is_git() { "git" } else { "local" };
            ui::ok(format!(
                "added {} endpoint '{}' at {} ({} packages)",
                kind,
                name.cyan(),
                root.display(),
                count
            ));
            Ok(())
        }
        EndpointCmd::Remove { name } => {
            if !reg.remove(&name) {
                return Err(format!("endpoint '{}' not found", name));
            }
            reg.save(home)?;
            ui::ok(format!("removed endpoint '{}'", name.cyan()));
            Ok(())
        }
        EndpointCmd::List => {
            if reg.endpoint.is_empty() {
                ui::info("no endpoints configured");
                return Ok(());
            }
            let mut rows: Vec<Vec<String>> = Vec::new();
            for ep in &reg.endpoint {
                let kind = if ep.is_git() { "git" } else { "local" };
                let loc = match ep {
                    endpoints::Endpoint::Local { path, .. } => path.display().to_string(),
                    endpoints::Endpoint::Git { url, .. } => url.clone(),
                };
                let root = ep
                    .ensure(home)
                    .ok()
                    .map(|r| r.display().to_string())
                    .unwrap_or_default();
                let count = ep
                    .ensure(home)
                    .ok()
                    .and_then(|r| endpoints::scan_root(&r).ok())
                    .map(|v| v.len())
                    .unwrap_or(0);
                rows.push(vec![
                    ep.name().to_string(),
                    kind.to_string(),
                    loc,
                    root,
                    count.to_string(),
                ]);
            }
            ui::table(&["name", "kind", "location", "resolves to", "pkgs"], &rows);
            Ok(())
        }
        EndpointCmd::Path { name } => match reg.get(&name) {
            Some(ep) => {
                let root = ep.ensure(home)?;
                println!("{}", root.display());
                Ok(())
            }
            None => Err(format!("endpoint '{}' not found", name)),
        },
    }
}
