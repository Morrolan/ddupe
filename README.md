# ddupe

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

## ✨ Features

- 🔍 **Fast recursive scanning**
- 🧠 **Content-based deduplication (SHA-256)**
- 🎨 **Colourised output for clarity**
- 📊 **Progress bar while hashing**
- 🧮 **Reports how much disk space can be freed**
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
