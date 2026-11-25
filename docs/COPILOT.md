# GitHub Copilot in LocalLeaf

LocalLeaf uses Microsoft's `copilot-sdk` Rust crate. An eligible GitHub account and Copilot subscription, an internet connection, and a supported system credential store are required. Model availability and usage limits are determined by GitHub.

## Sign in

**Option A — Copilot CLI:** install the official GitHub Copilot CLI, run `copilot` in a terminal, and complete `/login`. Open a project in LocalLeaf, select **Copilot**, then choose **Use CLI credentials / Refresh**. The SDK can also detect supported token environment variables (`COPILOT_GITHUB_TOKEN`, `GH_TOKEN`, or `GITHUB_TOKEN`) when the application starts.

**Option B — GitHub device flow:** create a GitHub OAuth App in GitHub's Developer Settings, enable its device flow, and copy its **Client ID** into LocalLeaf's Copilot panel. Click **Sign in with GitHub**, open the displayed GitHub link, and enter the displayed code. No OAuth client secret is required in the desktop app. The resulting token is stored in the OS credential store, not project files or settings. Linux requires a working Secret Service; Windows uses Credential Manager and macOS uses Keychain.

A classic personal access token (`ghp_`) is not a supported Copilot SDK authentication method. Never put a token in source code or send it in chat. Consult [GitHub's authentication documentation](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/authenticate) for supported credentials and [GitHub's OAuth setup guide](https://docs.github.com/en/copilot/how-tos/copilot-sdk/setup/github-oauth) for details.

**Sign out** removes the token stored by LocalLeaf and resets its session. It does not remove or revoke credentials belonging to the Copilot CLI or environment variables; these may be detected again after restarting the app. Revoke access in GitHub if required.

## Use

Open a project and select **Copilot**. Choose an available model, enter a message and press **Send** (or Ctrl/⌘+Enter). Change the model or choose **New chat** to reset the conversation. Use **Stop** to request cancellation. You can optionally include the current file or selected text. **Fix last compile error** sends the latest diagnostic and nearby source lines to Copilot. LaTeX code blocks can be copied or inserted into the editor; applying code is undoable.

By default, project contents are not shared automatically. The optional MCP checkbox allows the assistant to request project-file reads. Only the read-file and engine-list MCP tools are configured; MCP writes and other tools are disabled. There is currently no per-write approval workflow. This restriction is deliberate until it can be independently enforced and tested. See [MCP.md](MCP.md) for the existing server's broader capabilities.

## Build and verification

The SDK requires Rust 1.94 or newer. Its bundled runtime is downloaded or obtained from the configured SDK cache during a normal build. Build the frontend with `npm run build` from `desktop/`, and check the native application with `cargo check --manifest-path desktop/src-tauri/Cargo.toml` from the repository root. First-time native builds also need Tauri's platform dependencies.
