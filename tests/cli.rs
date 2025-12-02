use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::TempDir;

fn write_file(dir: &TempDir, name: &str, contents: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(contents).unwrap();
    path
}

fn binary_cmd() -> Command {
    #[allow(deprecated)]
    Command::cargo_bin("ddupe").unwrap()
}

#[test]
fn cli_reports_no_duplicates_for_unique_files() {
    let dir = TempDir::new().unwrap();
    let _ = write_file(&dir, "unique.txt", b"unique content");

    binary_cmd()
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

    let output = binary_cmd()
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
    assert!(stdout.contains("Dry run:"), "stdout was: {}", stdout);

    assert!(keep.exists());
    assert!(dupe.exists());
}

#[test]
fn interactive_mode_prompts_and_respects_choices() {
    let dir = TempDir::new().unwrap();
    let keep = write_file(&dir, "01-keep.txt", b"dupe");
    let delete_me = write_file(&dir, "02-delete-me.txt", b"dupe");
    let keep_me = write_file(&dir, "03-keep-me.txt", b"dupe");

    binary_cmd()
        .env("NO_COLOR", "1")
        .arg("-i")
        .arg(dir.path())
        // Choose the third file as the one to keep.
        .write_stdin("3\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Interactive mode"));

    assert!(
        !keep.exists() && !delete_me.exists(),
        "Expected non-selected files to be deleted"
    );
    assert!(keep_me.exists(), "Expected selected file to be kept");
}

#[test]
fn confirmation_decline_skips_deletion() {
    let dir = TempDir::new().unwrap();
    let keep = write_file(&dir, "keep.txt", b"dupe");
    let dupe = write_file(&dir, "dupe.txt", b"dupe");

    let output = binary_cmd()
        .env("NO_COLOR", "1")
        .arg(dir.path())
        // Decline deletion.
        .write_stdin("n\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Delete the [DUPE]"),
        "Expected confirmation prompt in stdout"
    );
    assert!(
        stdout.contains("Aborted. No files were deleted."),
        "Expected abort message in stdout"
    );

    assert!(keep.exists(), "Keep file should remain");
    assert!(dupe.exists(), "Dupe should not be removed when declined");
}

#[test]
fn confirmation_accepts_and_deletes_duplicates() {
    let dir = TempDir::new().unwrap();
    let keep = write_file(&dir, "keep.txt", b"dupe");
    let dupe_one = write_file(&dir, "dupe-one.txt", b"dupe");
    let dupe_two = write_file(&dir, "dupe-two.txt", b"dupe");

    let output = binary_cmd()
        .env("NO_COLOR", "1")
        .arg(dir.path())
        // Accept deletion.
        .write_stdin("y\n")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Deleted 2 file(s)"),
        "Expected deletion summary in stdout"
    );

    let survivors: Vec<_> = [keep.clone(), dupe_one.clone(), dupe_two.clone()]
        .into_iter()
        .filter(|path| path.exists())
        .collect();

    assert_eq!(survivors.len(), 1, "Only one file should remain");
    assert!(
        survivors[0] == keep || survivors[0] == dupe_one || survivors[0] == dupe_two,
        "Remaining file should be one of the original paths"
    );
}

#[test]
fn empty_directory_reports_and_exits_cleanly() {
    let dir = TempDir::new().unwrap();

    binary_cmd()
        .env("NO_COLOR", "1")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No files found"));
}
