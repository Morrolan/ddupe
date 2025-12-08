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
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

/// Command-line arguments for the `ddupe` tool.
#[derive(Parser)]
#[command(
    name = "ddupe",
    author = "Morrolan",
    version = "v1.0.0",
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

    /// Interactive deletion: review each duplicate one by one
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    /// Write analysis to a JSON file (implies dry-run; never deletes)
    #[arg(long = "json-output", value_name = "FILE")]
    json_output: Option<PathBuf>,
}

/// Data structure for JSON output.
#[derive(Serialize)]
struct JsonGroup {
    files: Vec<String>,
}

#[derive(Serialize)]
struct JsonReport {
    roots: Vec<String>,
    duplicate_groups: Vec<JsonGroup>,
    removable_count: usize,
    savings_bytes: u64,
    dry_run: bool,
    interactive: bool,
    mode: &'static str,
}

/// Ask the user a yes/no question. Returns `true` for "y"/"yes" (case-insensitive).
fn ask_yes_no(prompt: &str) -> bool {
    print!("{prompt} ");
    io::stdout().flush().ok();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let answer = input.trim();
            answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
        }
        Err(e) => {
            eprintln!("{} {}", "Failed to read input:".red(), e);
            false
        }
    }
}

/// Ask the user whether they want to proceed with deletion.
///
/// Returns `true` if the user explicitly answers "y" or "yes" (case-insensitive),
/// otherwise returns `false`.
fn ask_user_to_confirm() -> bool {
    let prompt = format!(
        "{}",
        "Delete the [DUPE] files and keep the [KEEP] ones? [y/N]:"
            .bright_red()
            .bold()
    );
    ask_yes_no(&prompt)
}

/// Prompt the user to select an index in the inclusive range [1, max],
/// or choose to keep all copies. Empty input defaults to 1.
fn prompt_for_selection(max: usize) -> Option<usize> {
    loop {
        print!(
            "{}\n{} ",
            "Enter a number to keep that file or 'a' to keep all copies.".bright_red(),
            format!("Which file should be kept? Enter 1-{} (default 1):", max)
                .bright_red()
                .bold()
        );
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            eprintln!("{}", "Failed to read input, defaulting to 1.".yellow());
            return Some(1);
        }
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Some(1);
        }
        if trimmed.eq_ignore_ascii_case("a") || trimmed.eq_ignore_ascii_case("all") {
            return None;
        }
        if let Ok(num) = trimmed.parse::<usize>()
            && num >= 1
            && num <= max
        {
            return Some(num);
        }
        eprintln!(
            "{}",
            format!("Please enter a number between 1 and {}.", max)
                .yellow()
                .bold()
        );
    }
}

/// Delete a single file path, returning the number of bytes freed if successful.
fn delete_path(path: &Path) -> Option<u64> {
    match fs::metadata(path) {
        Ok(meta) => {
            let size = meta.len();
            match fs::remove_file(path) {
                Ok(_) => {
                    println!("{} {}", "[DELETED]".red().bold(), path.display());
                    Some(size)
                }
                Err(e) => {
                    eprintln!(
                        "{} {}: {}",
                        "[FAILED]".red().bold(),
                        path.display(),
                        e.to_string().red()
                    );
                    None
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
            None
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
        if let Some(size) = delete_path(path) {
            deleted_count += 1;
            deleted_bytes += size;
        }
    }

    (deleted_count, deleted_bytes)
}

/// Interactively ask the user about each duplicate before deleting it.
///
/// Returns the same tuple as `delete_files`.
fn delete_files_interactively(groups: &[ddupe::DuplicateGroup]) -> (u64, u64) {
    println!(
        "{}",
        "Interactive mode: decide for each duplicate individually."
            .red()
            .bold()
    );

    let mut deleted_count = 0u64;
    let mut deleted_bytes = 0u64;

    for (idx, group) in groups.iter().enumerate() {
        let mut candidates = Vec::new();
        candidates.push(group.keep.clone());
        candidates.extend(group.dupes.iter().cloned());

        println!(
            "\n{} {} {}",
            "---".bright_yellow(),
            "Duplicate Group".bright_yellow().bold(),
            (idx + 1).to_string().bright_yellow()
        );

        for (i, path) in candidates.iter().enumerate() {
            let default_hint = if i == 0 { " (default)" } else { "" };
            println!(
                "  [{}] {}{}",
                (i + 1).to_string().bright_yellow(),
                path.display().to_string().cyan(),
                default_hint
            );
        }
        println!(
            "  [{}] {}",
            "A".bright_yellow(),
            "Keep all copies (skip deletion)".cyan()
        );

        let selection = prompt_for_selection(candidates.len());
        if selection.is_none() {
            println!(
                "{}",
                "[KEEPING ALL] Chose to keep every file in this group."
                    .green()
                    .bold()
            );
            continue;
        }
        let keep_idx = selection.unwrap() - 1;

        println!(
            "{} {}",
            "[KEEPING]".green().bold(),
            candidates[keep_idx].display().to_string().cyan()
        );

        for (i, path) in candidates.iter().enumerate() {
            if i == keep_idx {
                continue;
            }
            if let Some(size) = delete_path(path) {
                deleted_count += 1;
                deleted_bytes += size;
            }
        }
    }

    (deleted_count, deleted_bytes)
}

/// Write a JSON report to disk without deleting or prompting.
fn write_json_report(
    output_path: &Path,
    roots: &[PathBuf],
    analysis: &ddupe::DuplicateAnalysis,
    interactive: bool,
) -> io::Result<()> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let groups = analysis
        .groups
        .iter()
        .map(|g| JsonGroup {
            files: std::iter::once(&g.keep)
                .chain(g.dupes.iter())
                .map(|p| p.display().to_string())
                .collect(),
        })
        .collect();

    let report = JsonReport {
        roots: roots.iter().map(|r| r.display().to_string()).collect(),
        duplicate_groups: groups,
        removable_count: analysis.total_dupes(),
        savings_bytes: analysis.total_saving_bytes,
        dry_run: true,
        interactive,
        mode: "json",
    };

    let mut file = std::fs::File::create(output_path)?;
    serde_json::to_writer_pretty(&mut file, &report).map_err(io::Error::other)
}

fn main() {
    // Parse command-line arguments using clap.
    let args = Args::parse();
    let root = args.path;
    let roots = vec![root.clone()];
    let json_mode = args.json_output.is_some();

    println!(
        "{}\nLicense: LGPL-3.0-or-later\nSource: https://github.com/Morrolan/ddupe\nDocs:   https://morrolan.github.io/ddupe\n------------------------------------------------------------",
        "ddupe — Duplicate File Cleaner".bold()
    );

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
        if !json_mode {
            println!("{}", "No files found.".yellow());
        }
        return;
    }

    // Step 2: Build a hash map with a progress bar.
    let total_files = files.len() as u64;

    let multi = MultiProgress::new();

    let bar = multi.add(ProgressBar::new(total_files));
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let current = multi.add(ProgressBar::new_spinner());
    current.set_style(
        ProgressStyle::with_template("{spinner:.green} Hashing: {msg}")
            .unwrap()
            .tick_chars("/-\\| "),
    );
    current.enable_steady_tick(Duration::from_millis(100));

    // Build the map manually so we can update the bar as we go, but delegate
    // the actual hashing logic to the library.
    let mut map = std::collections::HashMap::new();
    for path in &files {
        current.set_message(path.display().to_string());
        if let Ok(hash) = ddupe::hash_file(path) {
            map.entry(hash).or_insert_with(Vec::new).push(path.clone());
        }
        bar.inc(1);
    }

    bar.finish_with_message("Hashing complete");
    current.finish_with_message("Hashing complete");

    // Step 3: Analyse duplicates using library logic.
    let analysis = analyse_duplicates(map);

    if json_mode {
        if let Some(output_path) = args.json_output.as_ref() {
            if let Err(e) = write_json_report(output_path, &roots, &analysis, args.interactive) {
                eprintln!(
                    "{} {}",
                    "Failed to write JSON report:".red().bold(),
                    e.to_string().red()
                );
                std::process::exit(1);
            }
            println!(
                "{} {}",
                "JSON report written to:".blue().bold(),
                output_path.display().to_string().cyan()
            );
        }
        return;
    }

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
        format_bytes(analysis.total_saving_bytes)
            .bright_green()
            .bold()
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
            "no files were deleted. Use without --dry-run to delete duplicates.".yellow()
        );
        return;
    }

    // Interactive deletion flow: decide per duplicate.
    if args.interactive {
        let (deleted_count, deleted_bytes) = delete_files_interactively(&analysis.groups);

        println!(
            "\n{} Deleted {} file(s), freeing approximately {}.",
            "Done:".green().bold(),
            deleted_count.to_string().bright_yellow(),
            format_bytes(deleted_bytes).bright_green().bold()
        );
    } else {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(contents).unwrap();
        path
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
}
