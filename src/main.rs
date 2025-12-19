use clap::{Parser, Subcommand};
use colored::*;
use std::path::PathBuf;
use std::fs;
use std::io::Write;
use std::process::Command;
use std::process;
use std::env;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// statics
static VERSION: &str = "0.0.1";
static ABOUT: &str = "\u{1b}[0;32mthe 'modern' package manager\u{1b}[0m";

// helper functions
fn get_current_path() -> String {
    env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "Unknown".to_string())
}

fn change_path(target: &PathBuf) -> Result<PathBuf, std::io::Error> {
    env::set_current_dir(target)?;
    // Return the absolute version so you can log exactly where you are
    env::current_dir()
}

fn read_xeo(path: &PathBuf, reverse: bool) {
    match fs::read_to_string(path) {
        Ok(content) => {
            println!("{}", "read xeo script");
            if reverse {
                reverse_xeo(content);
                return;
            }
            handle_xeo(content);
        },
        Err(e) => {
            eprintln!("{} {}", "failed to read xeo script:".red(), e);
        }
    }
}

fn reverse_xeo(script: String) {
    println!("reversing xeo script...");

    let initial_pwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let lines: Vec<&str> = script.lines().collect();
    
    let mut path_history: Vec<PathBuf> = Vec::new();
    
    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() { continue; }
        
        if parts[0] == "dir" && parts.len() >= 2 {
            path_history.push(env::current_dir().unwrap());
            if let Err(e) = change_path(&PathBuf::from(parts[1])) {
                eprintln!("{}", format!("[xeo] err: could not simulate path to {}: {}", parts[1], e).red());
            }
        }
    }

    for line in lines.into_iter().rev() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() { continue; }

        match parts[0] {
            "dir" => {
                if let Some(parent_dir) = path_history.pop() {
                    match change_path(&PathBuf::from(&parent_dir)) {
                        Ok(_) => println!("{}", format!("[xeo] moved out to: {}", parent_dir.display()).green()),
                        Err(e) => eprintln!("{}", format!("[xeo] err: failed to move back: {}", e).red()),
                    }
                }
            }
            "mkdir" => {
                if parts.len() >= 2 {
                    let dir_name = parts[1];
                    match fs::remove_dir_all(dir_name) {
                        Ok(_) => println!("{}", format!("[xeo] deleted directory: {}", dir_name).green()),
                        Err(e) => eprintln!("{}", format!("[xeo] err: failed to delete directory {}: {}", dir_name, e).red()),
                    }
                }
            },
            "make" => {
                if parts.len() >= 2 {
                    let file_name = parts[1];
                    match fs::remove_file(file_name) {
                        Ok(_) => println!("{}", format!("[xeo] deleted file: {}", file_name).green()),
                        Err(e) => eprintln!("{}", format!("[xeo] err: failed to delete file {}: {}", file_name, e).red()),
                    }
                }
            },
            "move" => {
                if parts.len() == 3 {
                    let src = parts[1];
                    let dest = parts[2];
                    if let Err(e) = fs::rename(dest, src) {
                        eprintln!("{}", format!("[xeo] err: failed to restore move {} to {}: {}", dest, src, e).red());
                    } else {
                        println!("{}", format!("[xeo] restored move: {} back to {}", dest, src).green());
                    }
                }
            },
            "print" | "chmod" => {
                // No action needed for reversal
            },
            _ => {
                eprintln!("{}", format!("[xeo] err: unknown command '{}'", parts[0]).red());
            }
        }
    }

    let _ = change_path(&initial_pwd);
}

fn handle_xeo(script: String) {
    println!("{}", "handling xeo script...");
    let pwd = PathBuf::from(get_current_path());
    let mut dir = home::home_dir().unwrap().join(".xeon");
    for line in script.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() { continue; }

        match parts[0] {
            "dir" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "[xeo] err: ".red(), "dir requires a directory name")
                }
                dir = PathBuf::from(parts[1]);
                match change_path(&dir) {
                    Ok(abs_path) => {
                        println!("{} {}", "[xeo] moved to:".green(), abs_path.display());
                    },
                    Err(e) => {
                        eprintln!("{} {}", "[xeo] err: failed to change directory:".red(), e);
                    }
                }       
            }
            "mkdir" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "[xeo] err:".red(), "mkdir requires a directory name");
                    continue;
                }
                let dir_name = parts[1];
                match fs::create_dir_all(dir_name) {
                    Ok(_) => println!("{} {}", "[xeo] created directory:".green(), dir_name),
                    Err(e) => eprintln!("{} {}: {}", "[xeo] err: failed to create directory".red(), dir_name, e),
                }
            },
            "make" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "[xeo] err:".red(), "make requires a file name");
                    continue;
                }
                let file_name = parts[1];
                match fs::File::create(file_name) {
                    Ok(_) => println!("{} {}", "[xeo] created file:".green(), file_name),
                    Err(e) => eprintln!("{} {}: {}", "[xeo] err: failed to create file".red(), file_name, e),
                }
            },
            "print" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "[xeo] err:".red(), "print requires a string to print");
                    continue;
                }
                let to_print = parts[1..].join(" ");
                println!("{}", to_print);
            },
            "move" => {
                if parts.len() < 3 {
                    eprintln!("{} {}", "[xeo] err:".red(), "move requires source and destination");
                    continue;
                }
                let src = parts[1];
                let dest = parts[2];
                match fs::rename(src, dest) {
                    Ok(_) => println!("{} {} {}", "[xeo] moved".green(), src, format!("to {}", dest).green()),
                    Err(e) => eprintln!("{} {} to {}: {}", "[xeo] err: failed to move".red(), src, dest, e),
                }
            },
            "chmod" => {
                let mut permissions = fs::metadata(parts[1]).expect("reason").permissions();
                #[cfg(unix)]
                {
                    permissions.set_mode(0o700);
                }

                #[cfg(not(unix))]
                {
                    permissions.set_readonly(false);
                }

                let _ = fs::set_permissions(parts[1], permissions);
                println!("{} {}", "[xeo] set executable permissions for:".green(), parts[1]);
            },
            // "shell" => {
            //     if parts.len() < 2 {
            //         eprintln!("{} {}", "xeo: err:".red(), "shell requires a command to execute");
            //         continue;
            //     }
            //     let cmd = parts[1..].join(" ");
            //     match Command::new("sh").arg("-c").arg(cmd).status() {
            //         Ok(status) => {
            //             if status.success() {
            //                 println!("{} {} {}", "shell command".green(), &cmd, "executed successfully.".green());
            //             } else {
            //                 eprintln!("{} {}", "shell command failed with status:".red(), status);
            //             }
            //         },
            //         Err(e) => eprintln!("{} {}", "failed to execute shell command:".red(), e),
            //     }
            // },
            _ => {
                eprintln!("{} {}", "[xeo] err:".red(), format!("unknown command '{}'", parts[0]));
            }
        }
    }
    match change_path(&pwd) {
        Ok(_) => {},
        Err(e) => {
            eprintln!("{} {}", "[xeo] fatal:".red(), e);
            process::exit(1);
        },
    }
}

// handler functions
fn handle_init() {
    // find home directory
    let home_dir = match home::home_dir() {
        // return if found
        Some(path) => path,
        // if no home directory send error message
        None => {
            eprintln!("{}", "could not determine home directory!".red());
            return;
        }
    };
    
    // if found, then add .xeon
    let xeon_dir = home_dir.join(".xeon");

    if xeon_dir.exists() {
        println!("{}", "xeon is already initialized.".yellow());
        return;
    }

    
}

// cli structs & enums
#[derive(Parser)]
#[command(name = "xeon", about = ABOUT)]
struct Cli {
    #[command(subcommand)]
    command: UserCommands
}

#[derive(Subcommand)]
enum UserCommands {
    /// check xeon version
    Version,
    /// refresh package database
    Update,
    /// adds a package
    Add { pkg: String },
    /// removes a package
    Rm { pkg: String },
    /// adds a new repository
    AddRepo { url: String },
    /// removes a repository
    RmRepo { alias: String },
    /// upgrades a package
    Upgrade { pkg: String },
    /// initializes xeon (if not already initialized)
    Init,
    /// runs custom .xeo file
    Xeo { 
        path: PathBuf,

        #[arg(short = 'r', long = "reverse", default_value_t = false)]
        reverse: bool,
    },
    // interacts with commands in the community repo
    // Dev {
    //     #[command(subcommand)]
    //     cmd: DevCommands,
    // },
}

// #[derive(Subcommand)]
// enum DevCommands {
//     Add,
//     Edit { pkg: String },
//     Rm { pkg: String },
// }

fn main() {
    // grab args
    let cli = Cli::parse();

    // route them
    match cli.command {
        UserCommands::Add { pkg } => {
            println!("{} {}{}", "installing".green(), pkg, "...".green());
        },
        UserCommands::Rm { pkg } => {
            println!("{} {}{}", "removing".green(), pkg, "...".green());
        },
        UserCommands::Update => {
            println!("{}", "refreshing package database...".green());
        },
        UserCommands::Upgrade { pkg } => {
            println!("{} {}{}", "upgrading".green(), pkg, "...");
        },
        UserCommands::Version => {
            println!("{}", "xeon: the 'modern' package manager".green());
            println!("v{}", VERSION);
        },
        UserCommands::AddRepo { url } => {
            println!("{} {}", "adding new repo:".green(), url);
        },
        UserCommands::RmRepo { alias } => {
            println!("{} {} {}", "removing".green(), alias, "repo...".green());
        },
        UserCommands::Init => {
            println!("{}", "initializing xeon...".green());
            handle_init();
        },
        UserCommands::Xeo { path, reverse } => {
            println!("{} {:?}", "running xeo script at".green(), path);
            read_xeo(&path, reverse);
        },
        // UserCommands::Dev { cmd } => {
        //     match cmd {
        //         DevCommands::Add => {
        //             println!("{} {} {}", "adding package to".green(), "community", "repo...".green());
        //         },
        //         DevCommands::Edit { pkg } => {
        //             println!("{} {}", "editing package:".green(), pkg);
        //         },
        //         DevCommands::Rm { pkg } => {
        //             println!("{} {}", "removing package from xeon-dev:".green(), pkg);
        //         },
        //     }
        // }
    }
}