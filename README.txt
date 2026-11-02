================================================================================
                                MAGNOLIA
                    Ultra-Fast Desktop Search Utility
================================================================================

Version : v0.3.0
Platform: Windows (x64)
License : MIT License (see LICENSE.txt)
Author  : levia (https://github.com/levialavender/magnolia)

--------------------------------------------------------------------------------
1. ABOUT MAGNOLIA
--------------------------------------------------------------------------------
Magnolia is a blazing-fast, lightweight desktop file and folder search utility 
built with pure Rust and accelerated natively using egui. Designed with zero 
browser/webview overhead, Magnolia uses minimal system resources and provides 
instantaneous, real-time search across your storage drives.

--------------------------------------------------------------------------------
2. KEY FEATURES
--------------------------------------------------------------------------------
- Instant Ready State & Persistent Index:
  On its first run, Magnolia automatically indexes all connected system drives 
  (C:\, D:\, etc.) in the background. The catalog is safely stored in an atomic 
  disk cache (%APPDATA%\Magnolia\magnolia.db). On every subsequent launch, millions 
  of records are ready instantly with 0-second startup latency.

- Advanced Query Engine:
  * Instant search-as-you-type response.
  * Negation Filtering: Use the exclamation mark (!) prefix to exclude terms.
    Example: "project !node_modules" or "document !backup"
  * Wildcard Matching: Full support for * and ? pattern matching.
    Example: "*.rs" or "financial_202?.*"
  * Regular Expressions (Regex): Toggle Regex mode for complex patterns.
  * Case-sensitive and full-path matching switches.

- Pure Native Desktop UI:
  * Clean standalone GUI window with no background console/terminal tab.
  * Multi-column sorting: Click column headers (Name, Path, Size, Date Modified) 
    for instant sorting.
- Windows Shell Integration:
  * Left-Click: Instantly opens the file or folder using its registered system default handler.
  * Right-Click: Summons the full native Windows Explorer context menu directly (Open, 
    Edit, 7-Zip/WinRAR archive actions, Cut/Copy, Properties, etc.).

--------------------------------------------------------------------------------
3. HOW TO USE
--------------------------------------------------------------------------------
1. Extract "magnolia.exe" and accompanying files to any folder.
2. Launch "magnolia.exe" (double-click).
3. If running for the first time, allow the background indexer a brief moment 
   to catalog your drives. You can begin typing queries immediately.
4. Type your query into the top search bar.
5. Left-click any file in the results list to open it directly with its default app.
6. Right-click any file to bring up the full native Windows Explorer context menu.

--------------------------------------------------------------------------------
4. WHAT'S NEW IN v0.3.0
--------------------------------------------------------------------------------
- Full Native Windows Explorer Context Menu on right-click (Shell IContextMenu integration).
- Left-click directly launches the file with its associated system application.
- High-Definition logo branding: Replaced application icon with high-resolution vector-rendered art.
- License file converted to plain text format (`LICENSE.txt`) for immediate readability on Windows.
================================================================================
