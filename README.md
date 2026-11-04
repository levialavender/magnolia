<div align="center">

# Magnolia

<img src="assets/icon.png" alt="Magnolia Logo" width="128" height="128" />

**An ultra-fast, lightweight, pure native desktop search utility for Windows.**

Built with **Rust** and powered by **egui** (GPU hardware-accelerated, zero-browser runtime, minimal memory footprint).

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE.txt)
[![Platform](https://img.shields.io/badge/Platform-Windows%20x64-0078D6.svg?logo=windows)](https://www.microsoft.com/windows)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-dea584.svg?logo=rust)](https://www.rust-lang.org/)

</div>

---

## Overview

**Magnolia** is a modern, lightweight desktop search utility designed for instant local file discovery on Windows. By focusing purely on file and folder metadata (names, paths, sizes, and timestamps) rather than parsing heavy document contents, Magnolia eliminates background disk thrashing, minimises memory overhead, and delivers search results with sub-millisecond responsiveness.

---

## Key Features

### ⚡ Rapid Indexing & Low Resource Usage
- **High-Speed Drive Indexing:** Rapidly catalogs local storage volumes and millions of files in seconds upon launch.
- **Optimised Memory Footprint:** Efficient in-memory data structures keep RAM consumption low even across large file collections.
- **Persistent Cache:** Safely stores indexed metadata locally for instantaneous ready states on every startup.

### 🔍 Instant Keystroke Search
- **Real-Time Keystroke Response:** Displays matching results instantly as you type with zero noticeable latency.
- **Flexible Query Filters:** Supports substring search, exclusion/negation filters (`!term`), and wildcard pattern matching (`*` and `?`).
- **Search Toggles:** Built-in switches for regular expressions (Regex), case sensitivity, and full-path matching.

### 🪟 Windows Shell Integration
- **Native Explorer Context Menu:** Right-click any item to summon the authentic Windows context menu with all system and third-party options.
- **Inline In-Place Renaming:** Rename files and folders directly inside the results table with automatic file extension preservation.
- **Open in Explorer:** Reveal, select, and automatically scroll to any target file directly inside Windows File Explorer.
- **Direct File Launch:** Double-click any row to open the file or folder immediately in its default registered application.
- **System File Icons:** Automatically renders authentic Windows icons corresponding to each file type and folder.

### 🖥️ Clean Native Interface
- **Lightweight Desktop UI:** Hardware-accelerated native interface with zero webview or browser dependencies.
- **Customisable Columns:** Drag column dividers to adjust Name, Path, Size, and Date Modified widths freely in real time.
- **Instant Scrolling:** Crisp, responsive table navigation without animated easing delays.
- **Live File Tracking:** Automatically detects newly added, renamed, or downloaded files in the background.

---

## Getting Started

### Prerequisites
- Windows 10 or Windows 11 (64-bit)
- [Rust toolchain](https://www.rust-lang.org/) (cargo, rustc 1.75+)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/levialavender/magnolia.git
cd magnolia

# Compile optimized release binary
cargo build --release
```

The resulting binary will be placed at:
```text
target/release/magnolia.exe
```

### Running Tests

```bash
cargo test
```

---

## Architecture Overview

```text
magnolia/
├── assets/             # Branding icons (icon.ico, icon.png)
├── build.rs            # Windows PE resource compilation (embeds icon into .exe)
├── Cargo.toml          # Rust dependencies & release profile optimizations
├── PRD.md              # Complete Product Requirements Document
├── src/
│   ├── context_menu.rs # Native Windows IContextMenu integration & message host
│   ├── db.rs           # Compact parent-id database model & atomic persistence
│   ├── icon.rs         # Win32 SHGetFileInfoW GDI icon extraction & caching
│   ├── lib.rs          # Library module exports
│   ├── main.rs         # egui/eframe GUI, event loop & table rendering
│   ├── ntfs.rs         # Direct MFT enumeration & USN Change Journal watcher
│   ├── query.rs        # Zero-allocation search engine (wildcard, negation, regex)
│   └── scanner.rs      # High-speed parallel multi-drive directory crawler
└── README.md
```

For full architectural specifications, user flows, and technical requirements, refer to **[`PRD.md`](PRD.md)**.

---

## Keyboard & Mouse Shortcuts

| Action | Input |
|---|---|
| **Select Row** | Left-Click (1×) |
| **Open File / Folder** | Double Left-Click (2×) |
| **Open Native Context Menu** | Right-Click |
| **Inline Rename** | Press <kbd>F2</kbd> (or click **Rename** in Context Menu) |
| **Commit Rename** | Press <kbd>Enter</kbd> (or click outside) |
| **Cancel Rename** | Press <kbd>Escape</kbd> |
| **Resize Column** | Click & Drag Column Divider |
| **Sort by Column** | Click Column Header Button (`Name`, `Path`, `Size`, `Date`) |

---

## License

This project is licensed under the **MIT License** — see the [LICENSE.txt](LICENSE.txt) file for details.
