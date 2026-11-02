# Magnolia

A blazing-fast, pure native desktop search utility built in **Rust** and powered by **egui** (GPU hardware-accelerated, zero-browser runtime, minimal memory footprint).

## Features

- **Instant Startup & Persistent Cache**:
  - Automatically indexes all available logical drives (`C:\`, `D:\`, etc.) in the background upon first launch.
  - Retains index state across restarts in `%APPDATA%\Magnolia\magnolia.db` with atomic write safety (`.db.tmp` swap).
  - Subsequent launches achieve instantaneous ready state with zero delay.

- **Fast In-Memory Search Engine**:
  - Substring search with instant keystroke response.
  - Negation filtering via `!term` (e.g. `document !backup`).
  - Wildcard pattern matching (`*` and `?`).
  - Regular expression (Regex) search toggle.
  - Case-sensitivity and path-matching switches.

- **Clean Native UI**:
  - Powered by `egui`/`eframe` running at full display refresh rate.
  - Clickable file entries that open and highlight directly inside Windows Explorer.
  - Instant sorting by Name, Path, File Size, and Date Modified.
  - Manual directory addition and whole-drive re-indexing capabilities.

## Getting Started

### Prerequisites
- [Rust toolchain](https://www.rust-lang.org/) (cargo, rustc 1.75+)

### Build from Source
```bash
cargo build --release
```

The compiled binary will be placed at:
```text
target/release/magnolia.exe
```

### Running Tests
```bash
cargo test
```

## Architecture

- `src/db.rs`: Core database models, sorting logic, and atomic serialization.
- `src/query.rs`: Token parsing, wildcard matching, and evaluation pipeline.
- `src/scanner.rs`: Multi-threaded asynchronous directory traversal.
- `src/ntfs.rs`: Low-level Windows filesystem and drive management interfaces.
- `src/main.rs`: Native desktop interface and event orchestration.

## License
MIT
