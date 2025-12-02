//! Core logic for ddupe.
//!
//! This module contains pure functionality for:
//! - hashing files
//! - collecting files from a directory tree
//! - analysing duplicates and computing potential space savings
//!
//! The CLI, progress bars, colouring and user interaction live in `src/main.rs`.

use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
};

/// Hash a single file using SHA-256 and return the hex-encoded digest.
///
/// This reads the file in chunks to avoid loading large files entirely
/// into memory.
pub fn hash_file(path: &Path) -> io::Result<String> {
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

/// Collect all files under a root directory (recursively).
///
/// Returns a flat list of file paths. Directories are ignored.
pub fn collect_files(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

/// Human-readable byte formatting (KB, MB, GB).
pub fn format_bytes(bytes: u64) -> String {
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

/// A single duplicate group: one "keep" file and zero or more "dupe" files.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// The file we keep in this group.
    pub keep: PathBuf,
    /// Files that are considered duplicates of `keep`.
    pub dupes: Vec<PathBuf>,
}

/// Full analysis result of a scan.
#[derive(Debug, Clone)]
pub struct DuplicateAnalysis {
    /// All groups that contain at least one duplicate.
    pub groups: Vec<DuplicateGroup>,
    /// All files that are candidates for deletion (all dupes in all groups).
    pub removable_files: Vec<PathBuf>,
    /// Total number of bytes that would be freed by deleting all dupes.
    pub total_saving_bytes: u64,
}

impl DuplicateAnalysis {
    /// Total number of duplicate files (i.e. potential deletions).
    pub fn total_dupes(&self) -> usize {
        self.removable_files.len()
    }
}

/// Given a mapping from content-hash -> list of files, build a `DuplicateAnalysis`.
///
/// Any hash that only has a single file is ignored (not a duplicate).
pub fn analyse_duplicates(hash_map: HashMap<String, Vec<PathBuf>>) -> DuplicateAnalysis {
    let mut groups = Vec::new();
    let mut removable_files = Vec::new();
    let mut total_saving_bytes: u64 = 0;

    for (_hash, mut files) in hash_map {
        if files.len() <= 1 {
            continue;
        }

        // Deterministic order: sort paths so that "keep" selection is stable.
        files.sort();

        let keep = files[0].clone();
        let dupes = files[1..].to_vec();

        for dupe in &dupes {
            if let Ok(meta) = fs::metadata(dupe) {
                total_saving_bytes += meta.len();
            }
        }

        removable_files.extend(dupes.clone());

        groups.push(DuplicateGroup { keep, dupes });
    }

    DuplicateAnalysis {
        groups,
        removable_files,
        total_saving_bytes,
    }
}

/// Build a hash map: SHA-256 hash -> list of files with that hash.
///
/// This version does **not** handle any UI/progress, so it is easy to test.
/// The CLI wrapper in `main.rs` can add progress bars while calling `hash_file`
/// if desired.
pub fn build_hash_map(files: &[PathBuf]) -> HashMap<String, Vec<PathBuf>> {
    let mut map: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for path in files {
        if let Ok(hash) = hash_file(path) {
            map.entry(hash).or_default().push(path.clone());
        }
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(contents).unwrap();
        path
    }

    #[test]
    fn hash_file_produces_expected_sha256() {
        let dir = TempDir::new().unwrap();
        let path = write_file(&dir, "sample.txt", b"hello world");

        let hash = hash_file(&path).unwrap();

        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn collect_files_recurses_and_ignores_directories() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let _a = write_file(&dir, "a.txt", b"a");
        let _b = write_file(&dir, "nested/b.txt", b"b");

        let files = collect_files(dir.path());
        let names: HashSet<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        let expected: HashSet<String> = ["a.txt", "b.txt"].into_iter().map(String::from).collect();

        assert_eq!(names, expected);
    }

    #[test]
    fn format_bytes_handles_common_boundaries() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024u64), "3.00 GB");
    }

    #[test]
    fn analyse_duplicates_builds_groups_and_savings() {
        let dir = TempDir::new().unwrap();
        let keep = write_file(&dir, "a.txt", b"x");
        let dupe_one = write_file(&dir, "b.txt", b"x");
        let dupe_two = write_file(&dir, "c.txt", b"x");
        let unique = write_file(&dir, "unique.txt", b"zzz");

        let mut map = HashMap::new();
        map.insert(
            "dup".to_string(),
            vec![dupe_two.clone(), keep.clone(), dupe_one.clone()],
        );
        map.insert("unique".to_string(), vec![unique.clone()]);

        let analysis = analyse_duplicates(map);

        assert_eq!(analysis.groups.len(), 1);
        let group = &analysis.groups[0];
        assert_eq!(group.keep, keep);

        let mut dupes = group.dupes.clone();
        dupes.sort();
        assert_eq!(dupes, vec![dupe_one, dupe_two]);
        assert_eq!(analysis.total_dupes(), 2);
        assert_eq!(analysis.total_saving_bytes, 2);
        assert!(
            analysis
                .removable_files
                .iter()
                .any(|p| p.ends_with("b.txt"))
        );
    }

    #[test]
    fn build_hash_map_groups_identical_content() {
        let dir = TempDir::new().unwrap();
        let first = write_file(&dir, "first.txt", b"same content");
        let second = write_file(&dir, "second.txt", b"same content");
        let unique = write_file(&dir, "unique.txt", b"different");

        let files = vec![first.clone(), second.clone(), unique.clone()];

        let map = build_hash_map(&files);
        let dup_hash = hash_file(&first).unwrap();
        let unique_hash = hash_file(&unique).unwrap();

        let mut dupes = map.get(&dup_hash).unwrap().clone();
        dupes.sort();
        assert_eq!(dupes, vec![first, second]);

        assert_eq!(map.get(&unique_hash).unwrap(), &vec![unique]);
    }
}
