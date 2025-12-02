# ddupe Documentation

Welcome to the ddupe docs site. This page is designed to be published via GitHub Pages from the `docs/` folder.

## Overview

ddupe is a fast, safe CLI for finding and cleaning duplicate files by hashing their contents (SHA-256). It works recursively, prints progress, and can delete duplicates in bulk or interactively.

## Installation

Build from source:

```bash
git clone https://github.com/Morrolan/ddupe.git
cd ddupe
cargo build --release
# binary will be at target/release/ddupe
```

## Usage

Basic scan (no deletions):

```bash
ddupe /path/to/scan
```

Dry-run (preview without deleting):

```bash
ddupe --dry-run /path/to/scan
```

Interactive deletion (choose which file to keep per duplicate group):

```bash
ddupe -i /path/to/scan
```

Non-interactive delete (asks once for confirmation, then removes all dupes):

```bash
ddupe /path/to/scan
# answer the prompt to proceed with deletion
```

## Features at a Glance

- Recursive scanning of directories
- Content-based duplicate detection using SHA-256
- Progress bar plus current-file display during hashing
- Interactive per-group selection with numbered choices
- Dry-run safety mode
- Clear reporting of space savings

## Exit Codes

- `0` on success
- Non-zero on errors (e.g., unreadable paths, failures during deletion)

## Tips

- Set `NO_COLOR=1` to disable coloured output in CI logs.
- Run `cargo fmt && cargo clippy && cargo test` before opening a PR to match the CI pipeline.

## Links

- Source: https://github.com/Morrolan/ddupe
- Issues: https://github.com/Morrolan/ddupe/issues
