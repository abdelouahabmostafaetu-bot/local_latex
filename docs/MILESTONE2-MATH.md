# Milestone 2: math authoring (work in progress)

The editor can render supported, same-line LaTeX math locally on hover with KaTeX. This is a preview, not a substitute for compiling with TeX: custom macros and other unsupported expressions may not render. It never changes the source. Rendering disables trusted HTML/URL commands and caps expansion and size. Preview can be disabled in Settings → Editor.

Quick fractions are **off by default**. To try them, open a `.tex` file, enable Settings → Editor → Quick fractions, and type `$a/`. The editor inserts `\frac{a}{}` with the cursor in the denominator. Undo restores the original numerator. The rule only activates once both delimiters are present on the same line, for a simple numerator in a `$…$`, `$$…$$`, `\(…\)` or `\[…\]` expression; it deliberately does not transform multiline math, comments, common literal environments, selections or multiple cursors. It also declines documents with more than 250,000 characters before the cursor. It executes no user code.

KaTeX 0.18.0 and its `commander` dependency are MIT licensed. The original CodeMirror libraries are MIT licensed. No external service or additional backend tool is required.

This is only the first slice of Milestone 2. LSP, MathLive, tables/diagrams, Zotero, search, grammar checking and the remaining requested features are not yet implemented.
