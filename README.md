# ddupe

[![CI](https://github.com/Morrolan/ddupe/actions/workflows/ci.yml/badge.svg)](https://github.com/Morrolan/ddupe/actions/workflows/ci.yml)
![Coverage](https://morrolan.github.io/ddupe/coverage-badge.svg)
[![Release](https://github.com/Morrolan/ddupe/actions/workflows/release.yml/badge.svg)](https://github.com/Morrolan/ddupe/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
![Rust Version](https://img.shields.io/badge/rust-stable-orange.svg)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-blue.svg)


**ddupe** is a fast, safe, cross-platform command-line tool for finding and removing duplicate files.  
Duplicates are detected by hashing file contents using **SHA-256**, so files are matched by **content**, not name or location.

Useful for cleaning up:

- Photo collections  
- Download folders  
- Backups / archives  
- Source trees  
- Any directory full of near-identical files  

ddupe works recursively and will scan an entire directory tree.

---

## 📘 Summary

| Item                | Details                                                                 |
|---------------------|-------------------------------------------------------------------------|
| **Name**            | `ddupe`                                                                 |
| **Description**     | Fast, safe, cross-platform duplicate-file detector & cleaner            |
| **Languages**       | Rust (CLI, async-ready, cross-platform)                                 |
| **Supported OS**    | Linux · macOS · Windows                                                 |
| **Duplicate Method**| SHA-256 content hashing (filename/location independent)                 |
| **Key Features**    | Recursive scan · Colourised output · Progress bar · Dry-run mode        |
| **Safety**          | Interactive confirmation step before deletion                           |
| **Binary Releases** | Available for Linux/macOS/Windows (built via GitHub Actions)            |
| **License**         | MIT                                                                     |
| **CI**              | rustfmt · clippy (warnings = errors) · unit + integration tests         |

## 🚀 Capabilities at a Glance

- 🔎 **Deep recursive scanning** of entire directory trees  
- 🧠 **Content-based detection** using SHA-256 hashes  
- 🎨 **Readable output** with colours and group markers  
- 📊 **Progress bars** for large scans  
- 🧪 **Dry-run mode** for safe previews  
- 🗑️ **Optional deletion** with clear KEEP/DUPE status  
- 💾 **Total space estimate** before removing anything  
- 🧰 **Pure Rust library** + thin CLI wrapper  
- 🌍 **Cross-platform binaries** via GitHub Actions  

## ❓ Why ddupe?

Most duplicate cleaners rely on filenames, heuristics, or metadata.  
ddupe instead:

- hashes the actual **file content**
- groups truly identical files regardless of name or location  
- avoids false matches or “smart guessing”  
- works the same on Linux, macOS, and Windows  
- is scriptable, testable, and written with safe Rust practices  

## ✨ Features

- 🔍 **Fast recursive scanning**
- 🧠 **Content-based deduplication (SHA-256)**
- 🎨 **Colourised output for clarity**
- 📊 **Progress bar while hashing**
- 🧮 **Reports how much disk space can be freed**
- 👟 **Interactive mode (`-i`) to accept/reject each duplicate**
- 🔒 **Safe by default** – always asks before deleting
- 🧪 **Dry-run mode** (`--dry-run`) to preview deletions
- 💥 Optional Windows/Linux/macOS binaries (cross-compiled)

---

## 📦 Installation

### 🔧 Build from source (recommended)

```bash
git clone https://github.com/yourname/ddupe.git
cd ddupe
cargo build --release
