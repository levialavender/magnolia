================================================================================
                                MAGNOLIA
                    Ultra-Fast Desktop Search Utility
================================================================================

Version : v0.2.0
Platform: Windows (x64)
License : MIT License (see LICENSE)
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
  * Windows Explorer Integration: Click any file link to immediately open and 
    highlight the item in Windows Explorer.

--------------------------------------------------------------------------------
3. HOW TO USE
--------------------------------------------------------------------------------
1. Extract "magnolia.exe" and accompanying files to any folder.
2. Launch "magnolia.exe" (double-click).
3. If running for the first time, allow the background indexer a brief moment 
   to catalog your drives. You can begin typing queries immediately.
4. Type your query into the top search bar.
5. Click on any file or folder in the results list to reveal and highlight it 
   inside Windows Explorer.

--------------------------------------------------------------------------------
4. WHAT'S NEW IN v0.2.0
--------------------------------------------------------------------------------
- Background terminal/console window has been completely removed (`windows_subsystem`).
- Embedded custom app branding and icon (`.ico` resource & window viewport icon).
- Standardized documentation and MIT license bundled with the release package.
================================================================================
