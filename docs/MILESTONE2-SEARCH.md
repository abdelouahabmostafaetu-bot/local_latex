# Milestone 2: local project search (work in progress)

Open **Search** in the header, or press **Ctrl/⌘ + Shift + F**. Type text and choose **Regular expression** and **Match case** when needed. Choose a matching line to open the file at that location. The active text editor buffer is searched as it currently appears, including edits not yet saved to disk; other files are read from disk.

Search runs locally using ripgrep's `grep-searcher`, `grep-regex`, `grep-matcher`, and `ignore` libraries under their MIT option. It does not follow symlinks or scan `.git`, `.texcraft`, `node_modules`, or `target`. It searches supported text-file extensions; binary, inaccessible, or larger-than-2-MiB files are skipped and counted. Searches are limited to 300 matching lines, 10,000 files, or four seconds and report when a limit is reached. Rust regex syntax applies; unsupported patterns produce an error. Results are one per matching line, at the first match. No universal sub-second performance guarantee is made.

Changing a query cancels obsolete work, and switching projects discards stale results. This is not yet the requested fuzzy file/command palette or LSP integration.
