//! ddupe - a simple CLI tool for finding and optionally deleting duplicate files.
//!
//! Duplicates are detected by hashing file contents (SHA-256), so files are
//! considered duplicates if their *contents* match, regardless of filename
//! or directory.

use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufReader, Read, Write},
    path::PathBuf,
};
use walkdir::WalkDir;

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

/// Hash a single file using SHA-256 and return the hex-encoded digest.
///
/// This reads the file in chunks to avoid loading large files entirely
/// into memory.
fn hash_file(path: &PathBuf) -> io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            // End of file
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Convert a byte count into a human-readable string (KB, MB, GB, etc.).
fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;

    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Recursively gather all files under the given root directory.
///
/// Returns a flat list of file paths. Directories are ignored.
fn collect_files(root: &PathBuf) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

/// Build a mapping from `hash -> list of files` with a progress bar.
///
/// This function:
/// - hashes all files
/// - updates a progress bar as it goes
/// - returns a `HashMap` where each key is a content hash and the value is
///   a list of files that share that hash.
fn build_hash_map(files: &[PathBuf]) -> HashMap<String, Vec<PathBuf>> {
    let total_files = files.len() as u64;

    let bar = ProgressBar::new(total_files);
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} files",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let mut map: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for path in files {
        match hash_file(path) {
            Ok(hash) => {
                map.entry(hash).or_default().push(path.clone());
            }
            Err(e) => {
                eprintln!(
                    "{} {}: {}",
                    "Error hashing".red().bold(),
                    path.display().to_string().red(),
                    e.to_string().red()
                );
            }
        }

        bar.inc(1);
    }

    bar.finish_with_message("Hashing complete");
    map
}

/// Print duplicate groups, decide which files to keep/delete, and calculate savings.
///
/// Returns a tuple:
/// - `Vec<PathBuf>`: list of duplicate files that could be safely removed
/// - `u64`: total number of bytes that would be freed by removing them
fn analyse_and_print_duplicates(
    hash_map: HashMap<String, Vec<PathBuf>>,
) -> (Vec<PathBuf>, u64) {
    println!("\n{}", "Duplicate files found:".yellow().bold());

    // Collect only groups that have more than one file (i.e. real duplicates)
    let mut duplicate_groups: Vec<Vec<PathBuf>> = Vec::new();
    for (_hash, files) in hash_map.into_iter() {
        if files.len() > 1 {
            duplicate_groups.push(files);
        }
    }

    if duplicate_groups.is_empty() {
        println!("{}", "No duplicates found 🎉".bright_green().bold());
        return (Vec::new(), 0);
    }

    let mut group_index = 0usize;
    let mut removable_files: Vec<PathBuf> = Vec::new();
    let mut total_saving_bytes: u64 = 0;

    for group in &duplicate_groups {
        group_index += 1;

        println!(
            "\n{} {} {}",
            "---".bright_yellow(),
            "Duplicate Group".bright_yellow().bold(),
            group_index.to_string().bright_yellow()
        );

        // For each group, we arbitrarily keep the first file and mark the rest as duplicates.
        // You could change this strategy later (e.g. keep newest, keep in preferred folder, etc.).
        for (i, f) in group.iter().enumerate() {
            if i == 0 {
                println!(
                    "{} {}",
                    "[KEEP]".green().bold(),
                    f.display().to_string().cyan()
                );
            } else {
                println!(
                    "{} {}",
                    "[DUPE]".red().bold(),
                    f.display().to_string().cyan()
                );

                // Try to get the file size so we can estimate savings.
                if let Ok(meta) = fs::metadata(f) {
                    total_saving_bytes += meta.len();
                    removable_files.push(f.clone());
                }
            }
        }
    }

    println!(
        "\n{} {} duplicate file(s) can be removed, freeing approximately {}.",
        "Summary:".blue().bold(),
        removable_files.len().to_string().bright_yellow(),
        format_bytes(total_saving_bytes).bright_green().bold()
    );

    (removable_files, total_saving_bytes)
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
fn delete_files(paths: Vec<PathBuf>) -> (u64, u64) {
    println!("{}", "Deleting duplicate files...".red().bold());

    let mut deleted_count = 0u64;
    let mut deleted_bytes = 0u64;

    for path in paths {
        match fs::metadata(&path) {
            Ok(meta) => {
                let size = meta.len();
                match fs::remove_file(&path) {
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

    // Step 2: Build a hash map of content hash -> list of files (with a progress bar).
    let hash_map = build_hash_map(&files);

    // Step 3: Print duplicate groups and calculate potential savings.
    let (removable_files, _saving_bytes) = analyse_and_print_duplicates(hash_map);

    // If there are no files to remove, we're done.
    if removable_files.is_empty() {
        return;
    }

    // If dry-run is enabled, we stop before any deletion.
    if args.dry_run {
        println!(
            "\n{} {}",
            "Dry run:".yellow().bold(),
            "no files were deleted. Use without --dry-run to delete duplicates."
                .yellow()
        );
        return;
    }

    // Step 4: Ask the user if they actually want to delete the duplicates.
    if !ask_user_to_confirm() {
        println!("{}", "Aborted. No files were deleted.".yellow());
        return;
    }

    // Step 5: Delete the duplicates and report the result.
    let (deleted_count, deleted_bytes) = delete_files(removable_files);

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
    use std::{collections::HashSet, io::Write};

    fn write_temp_file(dir: &tempfile::TempDir, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = File::create(&path).expect("create temp file");
        file.write_all(contents).expect("write temp file");
        path
    }

    #[test]
    fn format_bytes_formats_units() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.00 KB");
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
    }

    #[test]
    fn hash_file_produces_expected_digest() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp_file(&dir, "sample.txt", b"hello world");
        let expected = format!("{:x}", Sha256::digest(b"hello world"));

        let digest = hash_file(&path).unwrap();
        assert_eq!(digest, expected);
    }

    #[test]
    fn collect_files_returns_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = write_temp_file(&dir, "a.txt", b"a");
        let nested_dir = dir.path().join("nested");
        fs::create_dir(&nested_dir).unwrap();
        let nested_tempdir = tempfile::TempDir::new_in(&nested_dir).unwrap();
        let file_b = write_temp_file(&nested_tempdir, "b.txt", b"b");

        let files = collect_files(&dir.path().to_path_buf());
        let set: HashSet<PathBuf> = files.into_iter().collect();
        let expected: HashSet<PathBuf> = HashSet::from([file_a, file_b]);

        assert_eq!(set, expected);
    }

    #[test]
    fn build_hash_map_groups_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let dup1 = write_temp_file(&dir, "dup1.txt", b"same");
        let dup2 = write_temp_file(&dir, "dup2.txt", b"same");
        let unique = write_temp_file(&dir, "unique.txt", b"different");

        let map = build_hash_map(&vec![dup1.clone(), dup2.clone(), unique.clone()]);
        assert_eq!(map.len(), 2);

        let mut lengths: Vec<usize> = map.values().map(|v| v.len()).collect();
        lengths.sort();
        assert_eq!(lengths, vec![1, 2]);

        let duplicates = map
            .values()
            .find(|v| v.len() == 2)
            .expect("duplicate group missing");
        let set: HashSet<PathBuf> = duplicates.iter().cloned().collect();
        let expected = HashSet::from([dup1, dup2]);
        assert_eq!(set, expected);

        let unique_group = map
            .values()
            .find(|v| v.len() == 1)
            .expect("unique group missing");
        assert_eq!(unique_group[0], unique);
    }

    #[test]
    fn analyse_and_print_duplicates_marks_removable() {
        let dir = tempfile::tempdir().unwrap();
        let keep = write_temp_file(&dir, "keep.txt", b"data");
        let dupe = write_temp_file(&dir, "dupe.txt", b"data");

        let mut map = HashMap::new();
        map.insert("hash".to_string(), vec![keep.clone(), dupe.clone()]);

        let (removable_files, saved_bytes) = analyse_and_print_duplicates(map);
        assert_eq!(removable_files, vec![dupe]);
        assert_eq!(saved_bytes, 4);
    }

    #[test]
    fn delete_files_removes_files_and_reports_stats() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp_file(&dir, "to_delete.txt", b"bye");

        let (deleted_count, deleted_bytes) = delete_files(vec![path.clone()]);

        assert_eq!(deleted_count, 1);
        assert_eq!(deleted_bytes, 3);
        assert!(!path.exists());
    }
}
