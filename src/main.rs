//! ddupe - CLI entrypoint.
//!
//! This module handles:
//! - CLI parsing (clap)
//! - progress bars (indicatif)
//! - coloured output (colored)
//! - confirmation prompts and deletion
//!
//! Core logic for hashing and duplicate analysis lives in `lib.rs`.

use clap::Parser;
use colored::*;
use ddupe::{analyse_duplicates, collect_files, format_bytes};
use indicatif::{ProgressBar, ProgressStyle};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

/// Command-line arguments for the `ddupe` tool.
#[derive(Parser)]
#[command(
    name = "ddupe",
    author,
    version,
    about = "Find and optionally delete duplicate files based on content hashes.",
    long_about = "ddupe recursively scans a directory, hashes file contents using SHA-256,\n\
                  groups files with identical content, and can optionally delete duplicates,\n\
                  keeping one file per group. By default it asks for confirmation before\n\
                  deleting, and with --dry-run it will never delete anything."
)]
struct Args {
    /// Directory to scan recursively for duplicate files
    path: PathBuf,

    /// Dry run: do not delete files, only show what *would* be removed
    #[arg(long)]
    dry_run: bool,
}

/// Ask the user whether they want to proceed with deletion.
///
/// Returns `true` if the user explicitly answers "y" or "yes" (case-insensitive),
/// otherwise returns `false`.
fn ask_user_to_confirm() -> bool {
    print!(
        "{} ",
        "Delete the [DUPE] files and keep the [KEEP] ones? [y/N]:"
            .bright_red()
            .bold()
    );
    // Ensure the prompt is actually written to the terminal before reading input.
    io::stdout().flush().ok();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let answer = input.trim().to_lowercase();
            answer == "y" || answer == "yes"
        }
        Err(e) => {
            eprintln!("{} {}", "Failed to read input:".red(), e);
            false
        }
    }
}

/// Delete the given list of files, reporting progress and total savings.
///
/// Returns:
/// - number of successfully deleted files
/// - total number of bytes freed
fn delete_files(paths: &[PathBuf]) -> (u64, u64) {
    println!("{}", "Deleting duplicate files...".red().bold());

    let mut deleted_count = 0u64;
    let mut deleted_bytes = 0u64;

    for path in paths {
        match fs::metadata(path) {
            Ok(meta) => {
                let size = meta.len();
                match fs::remove_file(path) {
                    Ok(_) => {
                        deleted_count += 1;
                        deleted_bytes += size;
                        println!("{} {}", "[DELETED]".red().bold(), path.display());
                    }
                    Err(e) => {
                        eprintln!(
                            "{} {}: {}",
                            "[FAILED]".red().bold(),
                            path.display(),
                            e.to_string().red()
                        );
                    }
                }
            }
            Err(_) => {
                // File might have been deleted or become inaccessible between scan and this point.
                eprintln!(
                    "{} {}",
                    "[SKIPPED]".yellow().bold(),
                    path.display().to_string().yellow()
                );
            }
        }
    }

    (deleted_count, deleted_bytes)
}

fn main() {
    // Parse command-line arguments using clap.
    let args = Args::parse();
    let root = args.path;

    // Basic sanity check: ensure the directory exists.
    if !root.exists() {
        eprintln!(
            "{} {}",
            "Error:".red().bold(),
            format!("'{}' does not exist.", root.display()).red()
        );
        return;
    }

    println!(
        "{} {}",
        "Scanning:".green().bold(),
        root.display().to_string().bright_green()
    );

    // Step 1: Collect all files under the target directory.
    let files = collect_files(&root);
    if files.is_empty() {
        println!("{}", "No files found.".yellow());
        return;
    }

    // Step 2: Build a hash map with a progress bar.
    let total_files = files.len() as u64;

    let bar = ProgressBar::new(total_files);
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    // Build the map manually so we can update the bar as we go, but delegate
    // the actual hashing logic to the library.
    let mut map = std::collections::HashMap::new();
    for path in &files {
        if let Ok(hash) = ddupe::hash_file(path) {
            map.entry(hash).or_insert_with(Vec::new).push(path.clone());
        }
        bar.inc(1);
    }

    bar.finish_with_message("Hashing complete");

    // Step 3: Analyse duplicates using library logic.
    let analysis = analyse_duplicates(map);

    println!("\n{}", "Duplicate files found:".yellow().bold());

    if analysis.groups.is_empty() {
        println!("{}", "No duplicates found 🎉".bright_green().bold());
        return;
    }

    // Print groups with KEEP/DUPE markers.
    for (idx, group) in analysis.groups.iter().enumerate() {
        println!(
            "\n{} {} {}",
            "---".bright_yellow(),
            "Duplicate Group".bright_yellow().bold(),
            (idx + 1).to_string().bright_yellow()
        );

        println!(
            "{} {}",
            "[KEEP]".green().bold(),
            group.keep.display().to_string().cyan()
        );

        for dupe in &group.dupes {
            println!(
                "{} {}",
                "[DUPE]".red().bold(),
                dupe.display().to_string().cyan()
            );
        }
    }

    println!(
        "\n{} {} duplicate file(s) can be removed, freeing approximately {}.",
        "Summary:".blue().bold(),
        analysis.total_dupes().to_string().bright_yellow(),
        format_bytes(analysis.total_saving_bytes).bright_green().bold()
    );

    // If there are no files to remove (shouldn't happen if groups non-empty), we're done.
    if analysis.removable_files.is_empty() {
        return;
    }

    // Dry-run: show everything but do not delete.
    if args.dry_run {
        println!(
            "\n{} {}",
            "Dry run:".yellow().bold(),
            "no files were deleted. Use without --dry-run to delete duplicates."
                .yellow()
        );
        return;
    }

    // Ask the user if they actually want to delete the duplicates.
    if !ask_user_to_confirm() {
        println!("{}", "Aborted. No files were deleted.".yellow());
        return;
    }

    // Delete the duplicates and report the result.
    let (deleted_count, deleted_bytes) = delete_files(&analysis.removable_files);

    println!(
        "\n{} Deleted {} file(s), freeing approximately {}.",
        "Done:".green().bold(),
        deleted_count.to_string().bright_yellow(),
        format_bytes(deleted_bytes).bright_green().bold()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::fs::File;
    use std::io::Write;
    use std::process::Command;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(contents).unwrap();
        path
    }

    fn binary_path() -> PathBuf {
        if let Some(path) = option_env!("CARGO_BIN_EXE_ddupe") {
            PathBuf::from(path)
        } else {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("target/debug/ddupe");
            path
        }
    }

    #[test]
    fn delete_files_removes_and_counts_bytes() {
        let dir = TempDir::new().unwrap();
        let one = write_file(&dir, "one.txt", b"abc"); // 3 bytes
        let two = write_file(&dir, "two.txt", b"1234"); // 4 bytes

        let (count, bytes) = delete_files(&[one.clone(), two.clone()]);

        assert_eq!(count, 2);
        assert_eq!(bytes, 7);
        assert!(!one.exists());
        assert!(!two.exists());
    }

    #[test]
    fn cli_reports_no_duplicates_for_unique_files() {
        let dir = TempDir::new().unwrap();
        let _ = write_file(&dir, "unique.txt", b"unique content");

        Command::new(binary_path())
            .env("NO_COLOR", "1")
            .arg(dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("No duplicates found"));
    }

    #[test]
    fn cli_dry_run_reports_duplicates_without_deleting() {
        let dir = TempDir::new().unwrap();
        let keep = write_file(&dir, "keep.txt", b"dupe");
        let dupe = write_file(&dir, "dupe.txt", b"dupe");

        let output = Command::new(binary_path())
            .env("NO_COLOR", "1")
            .arg("--dry-run")
            .arg(dir.path())
            .output()
            .unwrap();

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("duplicate file(s) can be removed"),
            "stdout was: {}",
            stdout
        );
        assert!(
            stdout.contains("Dry run:"),
            "stdout was: {}",
            stdout
        );

        assert!(keep.exists());
        assert!(dupe.exists());
    }
}
