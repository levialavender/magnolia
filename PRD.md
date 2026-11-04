# PRODUCT REQUIREMENTS DOCUMENT (PRD) - Magnolia

## 1. Executive Summary & Background

### 1.1 Product Description
**Magnolia** is an ultra-fast, local desktop search utility built in pure **Rust** and powered by the GPU hardware-accelerated **egui/eframe** GUI framework. Designed with a strict *zero-browser / zero-webview runtime* architecture, Magnolia delivers an exceptionally lightweight memory footprint, instantaneous cold startup, and real-time keystroke search responsiveness without UI stuttering.

### 1.2 Problems Solved
Default operating system search tools (such as Windows Search Indexer) frequently suffer from severe user experience bottlenecks:
1. **Excessive Resource Consumption:** High CPU usage, heavy background disk I/O, and bloated RAM consumption caused by invasive full-text document parsing, OCR, and perpetual background indexing services.
2. **Slow Initial & Ongoing Indexing:** Traditional recursive folder crawling can take hours or even days to index modern multi-terabyte drives with millions of files.
3. **Sluggish, Non-Instantaneous Results:** Search UI latency, unwanted web search redirection (e.g. Bing integration), and failure to find low-level developer or system files immediately.

Magnolia solves these challenges by adopting an industry-proven in-memory architecture inspired by high-performance indexers (such as Voidtools Everything):
* **Exclusive Metadata Indexing:** Focuses purely on file and directory names, hierarchy, file sizes, and timestamps—completely bypassing expensive file content parsing.
* **Direct NTFS Master File Table (MFT) Enumeration:** Enumerates entire NTFS disk volumes (hundreds of thousands of files) in 1–2 seconds via low-level Win32 IOCTL calls.
* **Zero-Allocation Search Engine:** Keystroke search queries execute against in-memory slices without allocating heap strings per frame.
* **Seamless Native Windows Shell Integration:** Incorporates genuine Windows Explorer context menus (`IContextMenu`), Windows shell icons via GDI, and native explorer folder navigation.

---

## 2. User Personas & Roles

### 2.1 Target Personas
1. **Software Developers & Engineers:** Need immediate, sub-millisecond retrieval of source code files, git repositories, compiler build artifacts, dependencies, and configuration files across multiple drives.
2. **System Administrators & Power Users:** Require advanced query operators including negation (`!term`), wildcard matching (`*.rs`), and regular expressions, alongside native shell actions (Run as Administrator, 7-Zip, command line tools).
3. **General Desktop Users:** Want a clean, bloat-free, instant search bar that immediately locates downloaded files, documents, pictures, and media without disk thrashing.

### 2.2 Roles and Access Privileges
Magnolia operates as a local desktop executable with two operational privilege tiers:

| Role / Mode | System Privilege Level | System Capabilities & Behaviors |
|---|---|---|
| **Standard User (Non-Elevated)** | Standard Windows user privileges (no UAC prompt required) | • Uses concurrent multi-drive DFS crawler for file indexing.<br>• Real-time monitoring of user download directory (`%USERPROFILE%\Downloads`).<br>• Full search query execution, inline in-place file renaming, path copying, and default file launch. |
| **Administrator (Elevated)** | Elevated Administrator privileges (Run as Administrator / UAC) | • Opens raw volume handles (`\\.\C:`, `\\.\D:`, etc.).<br>• Executes **Direct NTFS MFT Enumeration** (indexing ~1.5 million files in 1–2 seconds).<br>• Connects to the **NTFS USN Change Journal** for real-time tracking of file creations, modifications, deletions, and renames across all NTFS drives at 0.0% idle CPU usage. |

---

## 3. Functional Requirements

### 3.1 Indexing & Storage Engine
* **Source Modules:** `src/db.rs`, `src/ntfs.rs`, `src/scanner.rs`
* **Description:**
  Manages in-memory storage of all indexed files and directories with atomic disk persistence. Employs a compact *parent-id hierarchy model* where directory paths are deduplicated into a unique folder table (`folder_paths`), allowing millions of file records to store only a 4-byte `parent_id: u32` rather than duplicate full path strings.
* **Technical Specifications:**
  - Binary persistence via `bincode` to `%APPDATA%\Magnolia\magnolia.db` using atomic temp-file replacement (`.db.tmp` $\to$ remove existing $\to$ rename).
  - NTFS MFT enumeration using Win32 `FSCTL_ENUM_USN_DATA` and `MFT_ENUM_DATA_V0`.
  - Multi-threaded fallback directory crawler using worker threads (`scan_drives_parallel`).

### 3.2 Instant Query Engine
* **Source Module:** `src/query.rs`
* **Description:**
  Multi-threaded in-memory search pipeline distributed across 4 concurrent worker threads via `std::thread::scope`.
* **Query Features:**
  - **Substring Matching:** Fast case-insensitive search using byte-slice window comparison (`contains_ignore_case`) with zero heap allocations.
  - **Negation Filtering:** Excludes terms prefixed with an exclamation mark (e.g. `document !draft`).
  - **Wildcard Search:** Supports wildcard asterisk `*` (zero or more characters) and question mark `?` (exactly one character) without regex compile overhead.
  - **Regular Expressions (Regex):** Dynamic regex matching powered by the `regex` crate.
  - **Match Case Toggle:** Exact character case comparison.
  - **Match Path Toggle:** Expands evaluation to include the full directory path string.

### 3.3 Graphical User Interface & Table Presentation
* **Source Modules:** `src/main.rs`, `src/icon.rs`
* **Description:**
  Native dark-mode desktop window operating at display refresh rate. Renders results in an interactive virtualized scroll table.
* **UI Features:**
  - **Non-Animated Instant Scrolling:** Smooth scrolling animations and inertia easing are strictly disabled (`ScrollArea::vertical().animated(false)` and `animation_time = 0.0`), eliminating frame interpolation overhead.
  - **Interactive Column Resizing:** 6-pixel draggable dividers with native `ResizeColumn` mouse cursor for dynamic width adjustment of Name, Path, Size, and Date Modified columns.
  - **Pixel-Perfect Left Alignment:** Column headers and table cell content are aligned to the left with clipped text rendering.
  - **Native Windows Shell Icons:** Extracts official 16×16 system icons for folders and file types via Win32 `SHGetFileInfoW` and GDI bitmap conversion.
  - **Single-Click Selection:** Highlights the active row without opening files.
  - **Double-Click Direct Launch:** Instantly launches selected files or directories using registered system handlers via Win32 `ShellExecuteW`.
  - **Native Open in Explorer:** Opens the parent folder in Windows File Explorer and automatically highlights/scrolls to the target file using Win32 `SHOpenFolderAndSelectItems`.

### 3.4 Shell Context Menu & In-Place Inline Rename
* **Source Modules:** `src/context_menu.rs`, `src/main.rs`
* **Description:**
  Deep integration with the Windows Shell context menu subsystem and inline file editing.
* **Features:**
  - **Native Windows Explorer IContextMenu:** Right-clicking any item summons the authentic Windows Explorer context menu (Open, Edit, 7-Zip, WinRAR, Delete, Properties, etc.) via a dedicated STA thread hosting a custom hidden message window with message pump forwarding (`IContextMenu2` / `IContextMenu3`). Includes `CMF_CANRENAME` to expose the native "Rename" action.
  - **Inline Cell Renaming:** Triggered via keyboard <kbd>F2</kbd> or selecting "Rename" from the context menu. Transforms the Name cell into an inline text input with stem selection (filename base selected, extension protected). Pressing <kbd>Enter</kbd> commits `std::fs::rename`; pressing <kbd>Escape</kbd> or clicking another row immediately cancels/commits and closes the form.

### 3.5 Real-Time Filesystem Watcher
* **Source Module:** `src/ntfs.rs`
* **Description:**
  Continuously synchronizes the in-memory index when files are created, renamed, or deleted externally.
* **Features:**
  - **NTFS USN Change Journal Poller:** Background loop polling `FSCTL_READ_USN_JOURNAL` every 500ms on elevated NTFS volumes (0.0% CPU overhead).
  - **Downloads Folder Monitor:** Real-time directory polling for `%USERPROFILE%\Downloads` to instantly register newly completed browser downloads without requiring manual rescanning.

---

## 4. User Flow

### 4.1 First Launch & Indexing Flow
1. User executes `magnolia.exe`.
2. Application checks for existing cache at `%APPDATA%\Magnolia\magnolia.db`.
3. **Cache Present:** Database loads into RAM in milliseconds; status bar shows *"Instant ready: N items loaded from cache"*.
4. **Cache Missing (First Run):**
   - Application queries all logical drives (`C:\`, `D:\`, etc.).
   - Attempts instant NTFS MFT enumeration; falls back to parallel multi-drive directory crawler if non-elevated.
   - Saves index atomically to `magnolia.db`.

### 4.2 Search Flow
1. User types query string into the top search bar (e.g. `*.jpg !backup`).
2. Event listener triggers `update_search()`.
3. Search engine distributes evaluations across worker threads using zero-allocation byte matchers.
4. Top matched results (up to 2,000 items) sort according to active column and render instantly into the virtualized scroll view.

### 4.3 Interaction & In-Place Renaming Flow
1. **File Launch:** User double-clicks a row $\to$ Win32 `ShellExecuteW` opens the item in its default application.
2. **Context Menu:** User right-clicks a row $\to$ Genuine Windows Explorer context menu displays at cursor coordinates.
3. **Inline Renaming:**
   - User presses <kbd>F2</kbd> or clicks "Rename" in the context menu.
   - Name column transforms into an inline input box with filename stem highlighted.
   - User types new name and presses <kbd>Enter</kbd> (or clicks away) $\to$ `std::fs::rename` executes, database updates, and input closes. Pressing <kbd>Escape</kbd> cancels.

---

## 5. Non-Functional & Technical Specifications

### 5.1 Technology Stack
* **Language:** Rust (2021 Edition, rustc 1.75+)
* **GUI Framework:** `eframe` / `egui 0.29.1` (Hardware-accelerated native rendering via OpenGL/Glow)
* **Windows API Bindings:** Crate `windows 0.58`:
  - `Win32_Foundation`, `Win32_Graphics_Gdi`
  - `Win32_Storage_FileSystem`, `Win32_System_IO`, `Win32_System_Ioctl`
  - `Win32_UI_Shell`, `Win32_UI_Shell_Common`, `Win32_UI_WindowsAndMessaging`
  - `Win32_System_Com`, `Win32_System_Ole`, `Win32_Security`
* **Serialization & Storage:** `bincode 1.3`, `serde 1.0` (Derive)
* **Utilities:** `chrono 0.4` (Local time formatting), `regex 1.10` (Regular expressions), `rfd 0.15` (Native folder dialog), `image 0.25` (PNG decoding), `winres 0.1` (Windows PE resource compilation)
* **Release Compiler Profile (`Cargo.toml`):**
  - `opt-level = 3` (Maximum optimizations)
  - `lto = true` (Link-Time Optimization)
  - `codegen-units = 1` (Maximal inlining and CPU vectorization)

### 5.2 Security & System Integrity
* **Authorization:** Standalone native desktop utility without remote web endpoints. Access adheres to Windows User Account Control (UAC) and NTFS Discretionary Access Control Lists (DACL).
* **Crash-Resilient Persistence:** Database serialization writes to a temporary file (`.db.tmp`) before replacing the primary database file to prevent database corruption during sudden process termination.
* **Isolated STA Threads:** COM/OLE operations execute on dedicated Single-Threaded Apartment (STA) threads, ensuring proper life-cycle destruction of Windows Shell objects.

---

## 6. System Scope & Limitations

### 6.1 Current Scope
1. **Operating System:** Exclusively **Microsoft Windows** (Windows 10 / Windows 11 x86_64).
2. **Metadata Scope:** Strictly file and directory metadata (Name, Full Path, Size, Date Modified).
3. **Display Limit:** Caps rendering to the first 2,000 matches per query to maintain 60+ FPS UI refresh rates.
4. **MFT Indexing:** Direct MFT enumeration requires NTFS formatted volumes with Administrator privileges; non-NTFS volumes (FAT32, exFAT) automatically utilize parallel directory crawling.

### 6.2 Out of Scope
1. **Full-Text Content Search:** Magnolia does not parse text content inside documents (PDF, DOCX, XLSX) or databases.
2. **Cross-Platform Support:** Non-Windows platforms (Linux, macOS) are not supported due to architectural coupling with Windows Shell APIs.
3. **Remote & Cloud Storage:** Network shares (SMB/NFS) and cloud-only virtual placeholders (e.g. OneDrive hydration) are not indexed in real time.
