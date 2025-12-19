use clap::{Parser, Subcommand};
use colored::*;
use std::path::PathBuf;
use std::fs;
use std::io::Write;
use std::process::Command;
use std::env;

// statics
static VERSION: &str = "0.0.1";
static ABOUT: &str = "\u{1b}[0;32mthe 'modern' package manager\u{1b}[0m";

// helper functions
fn read_xeo(path: &PathBuf) {
    match fs::read_to_string(path) {
        Ok(content) => {
            println!("{}", "read xeo script");
            handle_xeo(content);
        },
        Err(e) => {
            eprintln!("{} {}", "failed to read xeo script:".red(), e);
        }
    }
}

fn handle_xeo(script: String) {
    println!("{}", "handling xeo script...");
    for line in script.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() { continue; }

        match parts[0] {
            "mkdir" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "xeo: err:".red(), "mkdir requires a directory name");
                    continue;
                }
                let dir_name = parts[1];
                match fs::create_dir_all(dir_name) {
                    Ok(_) => println!("{} {}", "created directory:".green(), dir_name),
                    Err(e) => eprintln!("{} {}: {}", "failed to create directory".red(), dir_name, e),
                }
            },
            "make" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "xeo: err:".red(), "make requires a file name");
                    continue;
                }
                let file_name = parts[1];
                match fs::File::create(file_name) {
                    Ok(_) => println!("{} {}", "created file:".green(), file_name),
                    Err(e) => eprintln!("{} {}: {}", "failed to create file".red(), file_name, e),
                }
            },
            "print" => {
                if parts.len() < 2 {
                    eprintln!("{} {}", "xeo: err:".red(), "print requires a string to print");
                    continue;
                }
                let to_print = parts[1..].join(" ");
                println!("{}", to_print);
            },
            "move" => {
                if parts.len() != 3 {
                    eprintln!("{} {}", "xeo: err:".red(), "move requires source and destination");
                    continue;
                }
                let src = parts[1];
                let dest = parts[2];
                match fs::rename(src, dest) {
                    Ok(_) => println!("{} {} {}", "moved".green(), src, format!("to {}", dest).green()),
                    Err(e) => eprintln!("{} {} to {}: {}", "failed to move".red(), src, dest, e),
                }
            },
            _ => {
                eprintln!("{} {}", "xeo: err:".red(), format!("unknown command '{}'", parts[0]));
            }
        }
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

    match fs::create_dir(&xeon_dir) {
        // goes through successfully
        Ok(_) => println!("{}", "xeon initialized successfully.".green()),
        // no perms or other errors
        Err(e) => eprintln!("{} {}", "failed to initialize xeon:".red(), e),
    }

    // move xeon binary

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
    Xeo { path: PathBuf },
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
        UserCommands::Xeo { path } => {
            println!("{} {:?}", "running xeo script at".green(), path);
            read_xeo(&path);
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