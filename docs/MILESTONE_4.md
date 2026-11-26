# Milestone 4 — work in progress

## Read-only Git overview

Open a project whose root contains a `.git` directory or worktree file, then select **Git** in the header. The sidebar shows the current branch (or detached commit) and the project's staged, working-tree and untracked paths. Select **Refresh** to rescan after saving or changing files. The view reads disk; it does not include an unsaved editor buffer. Up to 1,000 changed paths are displayed. Submodule contents are not scanned.

The scan uses gitoxide with isolated configuration; it does not run Git hooks or write the repository. Git clean/process filters may execute commands, so a repository declaring local filters is not scanned. Global Git configuration and its filters are not read; repositories relying on global filters may report inaccurate status. No staging, commit, branch switching, or diffs are provided yet.

## Remaining work

Git write operations and diffs, PDF revision comparison, CRDT collaboration, expanded observability and credential security, release automation, updater, telemetry and production E2E coverage remain incomplete. Earlier milestones also contain unfinished items; this overview is not a claim of milestone completion.
