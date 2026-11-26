//! A read-only project Git overview. No git hooks, credential helpers, or write operations.
use crate::{AppState, CmdResult};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitOverview {
    is_repository: bool,
    branch: Option<String>,
    detached: bool,
    changes: Vec<GitChange>,
    truncated: bool,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct GitChange {
    path: String,
    staged: bool,
    working_tree: bool,
    untracked: bool,
}

pub fn overview(root: &Path) -> CmdResult<GitOverview> {
    if !root.join(".git").exists() {
        return Ok(GitOverview {
            is_repository: false,
            branch: None,
            detached: false,
            changes: vec![],
            truncated: false,
        });
    }
    // Isolated config prevents reading user/global configuration and executing Git to discover it.
    let repo = gix::open_opts(root, gix::open::Options::isolated())
        .map_err(|e| format!("Cannot open project repository: {e}"))?;
    let workdir = repo.workdir().ok_or("A working tree is required")?;
    if workdir.canonicalize().map_err(|e| e.to_string())?
        != root.canonicalize().map_err(|e| e.to_string())?
    {
        return Err("The repository must be rooted at the open project folder.".into());
    }
    // Git clean/process filters can run arbitrary programs during a status scan. Refuse
    // the scan rather than execute project-configured filters without user consent.
    if repo.config_snapshot().sections_by_name("filter").is_some() {
        return Err("Git status is unavailable for repositories with configured filters; LocalLeaf will not run external filter commands.".into());
    }
    let head = repo.head_name().map_err(|e| e.to_string())?;
    let detached = head.is_none();
    let branch = head
        .map(|n| n.to_string().trim_start_matches("refs/heads/").to_string())
        .or_else(|| {
            repo.head_id()
                .ok()
                .map(|id| id.to_string().chars().take(12).collect())
        });
    let mut changes: BTreeMap<String, GitChange> = BTreeMap::new();
    let iter = repo
        .status(gix::progress::Discard)
        .map_err(|e| e.to_string())?
        .index_worktree_submodules(None)
        .untracked_files(gix::status::UntrackedFiles::Files)
        .into_iter(Vec::<gix::bstr::BString>::new())
        .map_err(|e| e.to_string())?;
    for item in iter {
        let item = item.map_err(|e| e.to_string())?;
        let path = String::from_utf8_lossy(item.location().as_ref()).into_owned();
        let change = changes.entry(path.clone()).or_insert_with(|| GitChange {
            path,
            ..Default::default()
        });
        match item {
            gix::status::Item::TreeIndex(_) => change.staged = true,
            gix::status::Item::IndexWorktree(
                gix::status::index_worktree::Item::DirectoryContents { .. },
            ) => change.untracked = true,
            gix::status::Item::IndexWorktree(_) => change.working_tree = true,
        }
    }
    let truncated = changes.len() > 1000;
    Ok(GitOverview {
        is_repository: true,
        branch,
        detached,
        changes: changes.into_values().take(1000).collect(),
        truncated,
    })
}

#[tauri::command]
pub async fn git_overview(state: State<'_, AppState>) -> CmdResult<GitOverview> {
    let root = state.open_root().ok_or("No project is open")?;
    tauri::async_runtime::spawn_blocking(move || overview(&root))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    fn git(p: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(p)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    #[test]
    fn scan_real_repo_and_deny_filters() {
        let p = std::env::temp_dir().join(format!(
            "localleaf-git-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        assert!(!overview(&p).unwrap().is_repository);
        git(&p, &["init", "-q"]);
        git(&p, &["config", "user.email", "test@example.invalid"]);
        git(&p, &["config", "user.name", "Test"]);
        fs::write(p.join("paper.tex"), "initial\n").unwrap();
        git(&p, &["add", "paper.tex"]);
        git(&p, &["commit", "-qm", "initial"]);
        let clean = overview(&p).unwrap();
        assert!(clean.is_repository && clean.changes.is_empty());
        fs::write(p.join("paper.tex"), "staged\n").unwrap();
        git(&p, &["add", "paper.tex"]);
        fs::write(p.join("paper.tex"), "unstaged\n").unwrap();
        fs::write(p.join("new.bib"), "new\n").unwrap();
        let status = overview(&p).unwrap();
        let item = status
            .changes
            .iter()
            .find(|i| i.path == "paper.tex")
            .unwrap();
        assert!(item.staged && item.working_tree);
        assert!(status
            .changes
            .iter()
            .any(|i| i.path == "new.bib" && i.untracked));
        git(&p, &["config", "filter.evil.clean", "touch INSECURE"]);
        assert!(overview(&p).err().unwrap().contains("filters"));
        assert!(!p.join("INSECURE").exists());
        fs::remove_dir_all(p).unwrap();
    }
}
