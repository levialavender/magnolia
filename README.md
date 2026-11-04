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

**Magnolia** is designed as a modern, bloat-free local desktop search engine inspired by the proven architecture of Voidtools *Everything*. Unlike built-in OS search tools that index heavy document contents and cause background disk thrashing, Magnolia focuses exclusively on file and folder metadata (names, paths, sizes, and timestamps).

By reading the **NTFS Master File Table (MFT)** directly and maintaining a compact in-memory database, Magnolia indexes entire storage drives containing millions of files in mere seconds and provides instantaneous, zero-latency keystroke search results.

---

## Key Features

### ⚡ Blazing-Fast Indexing & Low Memory Footprint
- **Direct NTFS MFT Enumeration:** Leverages low-level Win32 IOCTL calls (`FSCTL_ENUM_USN_DATA`) to enumerate full NTFS drive volumes (~1.5+ million files) in **1–2 seconds** when running elevated.
- **High-Speed Parallel Crawler Fallback:** Automatically switches to a concurrent multi-threaded DFS directory crawler across all physical drives when running without administrator privileges.
- **Compact Parent-ID Architecture:** Saves up to **80% RAM** compared to traditional indexers by deduplicating directory strings into a unified folder dictionary (`folder_paths`), storing only 4-byte `parent_id` references per file.
- **Atomic Disk Persistence:** Serializes database state to `%APPDATA%\Magnolia\magnolia.db` via `bincode` with atomic swap safety (`.db.tmp` swap) for instantaneous cold startup.

### 🔍 Advanced Instant Search Engine
- **Zero-Allocation Keystroke Search:** Uses ASCII byte-slice matching (`contains_ignore_case`) executed in parallel across 4 worker threads—delivering sub-15ms query evaluations without heap memory churn.
- **Negation Filtering:** Prefix any keyword with `!` to exclude unwanted terms (e.g., `project !node_modules` or `document !backup`).
- **Wildcard Matching:** Full native support for wildcard asterisk `*` (zero or more characters) and question mark `?` (single character) without regex overhead (e.g., `*.rs` or `report_202?.*`).
- **Regular Expressions (Regex):** One-click toggle for full PCRE-compliant regex matching.
- **Match Case & Match Path Switches:** Easily filter case-sensitively or expand queries across full directory path hierarchies.

### 🪟 Authentic Windows Shell Integration
- **Native Windows Explorer Context Menu (`IContextMenu`):** Right-clicking any item summons the genuine Windows Explorer context menu (Open, Edit, 7-Zip, WinRAR, Delete, Properties, etc.) via a dedicated STA thread hosting a custom hidden message pump window supporting `IContextMenu2` and `IContextMenu3`.
- **Inline In-Place Renaming:** Press <kbd>F2</kbd> or choose **Rename** from the native context menu to edit file names directly within the table row. Automatically highlights the filename stem while preserving the extension. Press <kbd>Enter</kbd> or click outside to commit; press <kbd>Escape</kbd> to cancel.
- **Native "Open in Explorer":** Automatically opens File Explorer, scrolls to the file, and selects/highlights it natively using Win32 `SHOpenFolderAndSelectItems`.
- **Direct File Launch:** Double-click any row to open the file or folder immediately with its registered system default handler via Win32 `ShellExecuteW`.
- **Genuine Windows Shell Icons:** Automatically renders authentic 16×16 system icons for folders and file types extracted via Win32 `SHGetFileInfoW` and GDI.

### 🖥️ Native GPU-Accelerated UI
- **Zero-Browser Overhead:** Pure native Windows binary compiled with `#![windows_subsystem = "windows"]` (zero console window flash, zero Electron/Chromium bloat).
- **Interactive Resizable Columns:** Drag 6px column dividers with native `ResizeColumn` cursors to customize Name, Path, Size, and Date Modified widths in real time.
- **Pixel-Perfect Alignment:** Consistent, clean left-aligned presentation across all column headers and cells.
- **Instant Non-Animated Scrolling:** Easing and smooth scroll animations are strictly disabled (`.animated(false)`, `animation_time = 0.0`) for crisp, instant scrollwheel response.
- **Real-Time Filesystem Monitoring:**
  - **NTFS USN Change Journal:** Background poller tracking file additions, renames, and deletions in real time at 0.0% CPU overhead.
  - **Downloads Watcher:** Automatically monitors `%USERPROFILE%\Downloads` to display completed browser downloads instantly.

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
