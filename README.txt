================================================================================
                                MAGNOLIA
                    Ultra-Fast Desktop Search Utility
================================================================================

Version : v0.4.0
Platform: Windows (x64)
License : MIT License (see LICENSE.txt)
Author  : levia (https://github.com/levialavender/magnolia)

--------------------------------------------------------------------------------
1. ABOUT MAGNOLIA
--------------------------------------------------------------------------------
Magnolia is a blazing-fast, lightweight desktop file and folder search utility 
built with pure Rust and accelerated natively using egui. Designed with zero 
browser/webview overhead, Magnolia uses minimal system resources and provides 
instantaneous, real-time search across all your storage drives.

--------------------------------------------------------------------------------
2. KEY FEATURES
--------------------------------------------------------------------------------
- Direct NTFS MFT Enumeration:
  Reads the Master File Table directly via Win32 IOCTL (FSCTL_ENUM_USN_DATA), 
  indexing entire multi-terabyte drives (~1.5+ million files) in 1-2 seconds.
  Automatically falls back to parallel multi-drive crawling if un-elevated.

- Compact Parent-ID Memory Architecture:
  Saves up to 80% RAM by storing folder paths once in a unique folder dictionary 
  and referencing them by a 4-byte parent ID per file record.

- Instant Ready State & Persistent Index:
  Retains the database in an atomic disk cache (%APPDATA%\Magnolia\magnolia.db) 
  so subsequent launches start with 0-second latency.

- Zero-Allocation Advanced Query Engine:
  * Substring search using parallel CPU byte matching (sub-15ms response).
  * Negation Filtering: Use the exclamation mark (!) prefix to exclude terms.
    Example: "project !node_modules" or "document !backup"
  * Wildcard Matching: Full native support for * and ? pattern matching.
    Example: "*.rs" or "financial_202?.*"
  * Regular Expressions (Regex): Toggle Regex mode for complex patterns.
  * Case-sensitive and full-path matching switches.

- Pure Native Desktop UI:
  * Hardware-accelerated GPU rendering with zero background console window.
  * Non-animated instant scrolling for immediate scrollwheel response.
  * Interactive resizable columns with draggable dividers (ResizeColumn cursor).
  * Pixel-perfect left-aligned table layout.
  * Genuine Windows Shell icons extracted natively via GDI.

- Windows Shell Integration:
  * Single-Click: Highlights and selects a row.
  * Double-Click: Launches the file directly with its default system handler.
  * Right-Click: Summons the authentic native Windows Explorer context menu 
    (Open, Edit, 7-Zip, WinRAR, Delete, Properties, etc.) via dedicated STA host.
  * Inline In-Place Renaming: Press F2 or click Rename in the context menu to 
    edit file names directly in the Name column with automatic stem selection.
  * Open in Explorer: Automatically highlights and scrolls to the file in 
    File Explorer via SHOpenFolderAndSelectItems.

- Real-Time Filesystem Watcher:
  * Monitors the NTFS USN Change Journal in the background (0.0% CPU usage).
  * Live-watches the Downloads directory (%USERPROFILE%\Downloads) for new files.

--------------------------------------------------------------------------------
3. HOW TO USE
--------------------------------------------------------------------------------
1. Launch "magnolia.exe" (double-click).
2. Type your query into the top search bar.
3. Left-click once to select an item.
4. Double-click to open any file or folder directly.
5. Right-click to access the full native Windows Explorer context menu.
6. Press F2 or select Rename to rename files inline. Press Enter to save.

================================================================================
