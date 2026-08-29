/// terminal helpers: colors, tables, confirmations
use colored::*;
use std::io::{IsTerminal, Write};

pub fn ok(msg: impl AsRef<str>) {
    println!("{} {}", "✔".green().bold(), msg.as_ref().green());
}

pub fn info(msg: impl AsRef<str>) {
    println!("{} {}", "·".cyan().bold(), msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    println!("{} {}", "!".yellow().bold(), msg.as_ref().yellow());
}

pub fn err(msg: impl AsRef<str>) {
    eprintln!("{} {}", "✘".red().bold(), msg.as_ref().red());
}

/// print a left-aligned padded table; accent the first column
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        println!("{}", "(none)".yellow());
        return;
    }
    let mut widths: Vec<usize> = headers.iter().map(|h| display_width(h)).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(display_width(cell));
            }
        }
    }

    let mut line = String::new();
    for (i, h) in headers.iter().enumerate() {
        line.push_str(&format!("{:<width$}  ", h, width = widths[i]));
    }
    println!("{}", line.trim_end().bold());

    for row in rows {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            if i == 0 {
                line.push_str(&format!("{:<width$}  ", cell.cyan(), width = widths[i]));
            } else {
                line.push_str(&format!("{:<width$}  ", cell, width = widths[i]));
            }
        }
        println!("{}", line.trim_end());
    }
}

/// ask yes/no when stdin is an interactive terminal (auto-yes otherwise)
pub fn confirm(prompt: &str) -> bool {
    if !std::io::stdin().is_terminal() {
        return false;
    }
    print!("{} [y/N] ", prompt);
    std::io::stdout().flush().ok();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let answer = input.trim().to_ascii_lowercase();
        return answer == "y" || answer == "yes";
    }
    false
}

fn display_width(s: &str) -> usize {
    s.chars().map(|c| if c.is_control() { 0 } else { 1 }).sum()
}
