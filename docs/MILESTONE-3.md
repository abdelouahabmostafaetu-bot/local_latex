# Milestone 3 — work in progress

This branch begins milestone 3; it does not complete that milestone.

## Review a Copilot suggestion

Open a LaTeX file, select the text to replace (or position the cursor to insert), and request a suggestion in Copilot. On a `latex` or `tex` code block, choose **Review diff**. Accept or reject each highlighted hunk, then apply. The review is staged separately from the file; applying uses the existing editor and autosave. An editor undo reverts the applied changes. A review decision can be undone with the button or Ctrl/Cmd+Z when the review editor has focus. If the project, file, or editor text changes while the review is open, applying is refused and the suggestion must be reviewed again. Direct Insert and Replace actions remain available and do not require review.

In Logs, **Send to AI** sends the selected diagnostic, up to 11 surrounding source lines when available, and the main document preamble (up to 16,000 characters). If the selected provider is not ready or is busy, the request is prepared instead. The existing optional context checkboxes still apply. No automatic file edits or MCP writes are introduced.

## Verification

- `cd desktop && npm test && npm run build`
- `cargo check --manifest-path desktop/src-tauri/Cargo.toml`
- With the Vite dev server running, `cd desktop && AI_REVIEW_ONLY=1 node e2e/ui-smoke.mjs` exercises a **mocked** Copilot response, partial hunk review, decision and editor undo, stale-file protection, and diagnostic context. `CHROMIUM_PATH` can select a Chromium executable.

The full desktop Rust test build was attempted but the Copilot SDK compilation was killed by the available memory limit. The provider URL and SSE parser tests were also run in an isolated harness copied from `provider.rs` and passed.

This does not test a real Copilot account or model, live compilation, SyncTeX, or PDF rendering end to end. Realtime TeX, rmcp migration, OCR and local RAG are not implemented on this branch yet. The provider UI test uses a mock; a live Ollama or custom server has not been tested in this environment.


## Other AI providers

The AI provider selector supports a local Ollama server's OpenAI-compatible endpoint, or a custom OpenAI-compatible HTTPS endpoint. Start Ollama and install a model separately; enter its model name in the sidebar. A custom provider needs its `/v1` base URL, a model name, and an API key if required. Responses use streaming chat completions. Keys are kept in memory only and cleared when switching providers; remote plain HTTP and redirects are refused. The full current conversation, including context previously sent in that conversation, is sent to the selected provider. The connector does not grant the provider MCP access. Provider-specific compatibility and billing depend on the server; no model is bundled.
