// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! LocalLeaf desktop: an Overleaf-style UI on top of `texcraft-core`.
//!
//! The same executable doubles as an MCP server (`--mcp [--root DIR]`), so an AI agent
//! can drive the very project that is open in the editor. Edits the agent makes are
//! picked up by the file watcher and pushed to the UI as events.

mod build_cancel;
mod copilot;
mod menu;
mod projects;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use texcraft_core::compiler::{self, CompileOptions, TeXEngine};
use texcraft_core::project::{FileNode, Project, ProjectSettings};
use texcraft_core::{synctex, DEFAULT_OUTPUT_DIR};

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn anyhow_err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

struct OpenProject {
    project: Project,
    _watcher: Option<RecommendedWatcher>,
}

#[derive(Default)]
pub(crate) struct AppState {
    /// Set once the UI has saved everything and allowed the app to quit.
    exit_allowed: AtomicBool,
    open: RwLock<Option<OpenProject>>,
    compile_lock: tokio::sync::Mutex<()>,
    compile_cancel: build_cancel::BuildCancellation,
    compiling: Arc<AtomicBool>,
}

impl AppState {
    /// Root of the project currently open in the editor, if any.
    pub(crate) fn open_root(&self) -> Option<PathBuf> {
        self.open.read().ok()?.as_ref().map(|o| o.project.root().to_path_buf())
    }

    fn project(&self) -> CmdResult<Project> {
        self.open
            .read()
            .map_err(err)?
            .as_ref()
            .map(|o| o.project.clone())
            .ok_or_else(|| "no project is open".to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectInfo {
    root: String,
    name: String,
    main_document: Option<String>,
    /// Saved per-project settings (`.texcraft/settings.json`), if any.
    settings: Option<ProjectSettings>,
    tree: Vec<FileNode>,
    /// Command line an MCP client can use to drive this project.
    mcp_command: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineInfo {
    engine: TeXEngine,
    installed: bool,
    path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncTexSource {
    file: String,
    line: u32,
}

#[derive(Deserialize)]
struct ImportSource {
    path: String,
}

// ---------------------------------------------------------------------------
// Project lifecycle
// ---------------------------------------------------------------------------

/// Opens a project: scans it off the UI thread and starts watching it.
async fn open_project_at(app: AppHandle, state: &AppState, path: PathBuf) -> CmdResult<ProjectInfo> {
    let (project, info) = tokio::task::spawn_blocking(move || -> CmdResult<(Project, ProjectInfo)> {
        let project = Project::open(&path).map_err(anyhow_err)?;
        let info = project_info(&project)?;
        Ok((project, info))
    })
    .await
    .map_err(err)??;
    let watcher = match start_watcher(app, project.root().to_path_buf(), state.compiling.clone()) {
        Ok(w) => Some(w),
        Err(e) => {
            tracing::warn!("file watcher unavailable: {e}");
            None
        }
    };
    *state.open.write().map_err(err)? = Some(OpenProject { project, _watcher: watcher });
    Ok(info)
}

#[tauri::command]
async fn open_project(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<ProjectInfo> {
    open_project_at(app, &state, PathBuf::from(path)).await
}

#[tauri::command]
async fn create_project(
    app: AppHandle,
    state: State<'_, AppState>,
    parent: String,
    name: String,
    template_kind: Option<String>,
) -> CmdResult<ProjectInfo> {
    let name = name.trim().to_string();
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.') {
        return Err("invalid project name".into());
    }
    if !matches!(template_kind.as_deref(), None | Some("blank") | Some("arabic")) {
        return Err("unknown project template".into());
    }
    let dir = Path::new(&parent).join(&name);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    tokio::fs::create_dir_all(&dir).await.map_err(err)?;
    let title = name.replace(['\\', '{', '}', '%', '#', '&', '$', '_', '^', '~'], " ");
    let blank_template = format!(
        "\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\n\
         \\title{{{title}}}\n\\author{{}}\n\\date{{\\today}}\n\n\\begin{{document}}\n\n\\maketitle\n\n\
         \\section{{Introduction}}\n\nYour text here.\n\n\\end{{document}}\n"
    );
    let template = match template_kind.as_deref() {
        None | Some("blank") => blank_template,
        Some("arabic") => include_str!("../../../templates/arabic-main.tex").to_string(),
        Some(_) => return Err("unknown project template".into()),
    };
    tokio::fs::write(dir.join("main.tex"), template).await.map_err(err)?;
    open_project_at(app, &state, dir).await
}

/// Renames the folder of the open project (and reopens it at the new path).
#[tauri::command]
async fn rename_open_project(app: AppHandle, state: State<'_, AppState>, new_name: String) -> CmdResult<ProjectInfo> {
    if state.compiling.load(Ordering::SeqCst) {
        return Err("wait for the compilation to finish before renaming the project".into());
    }
    let old_root = state.project()?.root().to_path_buf();
    // Stop watching before the folder moves.
    *state.open.write().map_err(err)? = None;
    let from = old_root.clone();
    let renamed = tokio::task::spawn_blocking(move || projects::rename_dir(&from, &new_name))
        .await
        .map_err(err)?;
    match renamed {
        Ok(new_root) => open_project_at(app, &state, PathBuf::from(new_root)).await,
        Err(e) => {
            // Leave the project open where it was.
            let _ = open_project_at(app, &state, old_root).await;
            Err(e)
        }
    }
}

#[tauri::command]
fn close_project(state: State<'_, AppState>) -> CmdResult<()> {
    *state.open.write().map_err(err)? = None;
    Ok(())
}

#[tauri::command]
async fn get_tree(state: State<'_, AppState>) -> CmdResult<Vec<FileNode>> {
    let project = state.project()?;
    tokio::task::spawn_blocking(move || project.tree(&[]).map_err(anyhow_err))
        .await
        .map_err(err)?
}

#[tauri::command]
async fn save_project_settings(state: State<'_, AppState>, settings: ProjectSettings) -> CmdResult<()> {
    state.project()?.save_settings(&settings).await.map_err(anyhow_err)
}

fn project_info(project: &Project) -> CmdResult<ProjectInfo> {
    let root = project.root();
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "texcraft-core".into());
    Ok(ProjectInfo {
        root: root.to_string_lossy().into_owned(),
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Project".into()),
        main_document: project.main_document(),
        settings: project.load_settings(),
        tree: project.tree(&[]).map_err(anyhow_err)?,
        mcp_command: vec![exe, "--mcp".into(), "--root".into(), root.to_string_lossy().into_owned()],
    })
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[tauri::command]
async fn read_project_file(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    state.project()?.read_text(&path).await.map_err(anyhow_err)
}

#[tauri::command]
async fn read_binary_file(state: State<'_, AppState>, path: String) -> CmdResult<tauri::ipc::Response> {
    let bytes = state.project()?.read_bytes(&path).await.map_err(anyhow_err)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
async fn write_project_file(state: State<'_, AppState>, path: String, content: String) -> CmdResult<()> {
    state.project()?.write(&path, content.as_bytes()).await.map_err(anyhow_err)
}

#[tauri::command]
async fn create_file(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state.project()?.create_file(&path).await.map_err(anyhow_err)
}

#[tauri::command]
async fn create_folder(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state.project()?.create_dir(&path).await.map_err(anyhow_err)
}

#[tauri::command]
async fn rename_path(state: State<'_, AppState>, from: String, to: String) -> CmdResult<()> {
    state.project()?.rename(&from, &to).await.map_err(anyhow_err)
}

#[tauri::command]
async fn delete_path(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state.project()?.delete(&path).await.map_err(anyhow_err)
}

/// Copies files chosen by the user (dialog or drag & drop) into `target_dir`.
/// Existing files are only replaced when `overwrite` is true (the UI asks first).
#[tauri::command]
async fn import_files(
    state: State<'_, AppState>,
    target_dir: String,
    sources: Vec<ImportSource>,
    overwrite: Option<bool>,
) -> CmdResult<Vec<String>> {
    let project = state.project()?;
    let mut imported = Vec::new();
    for src in sources {
        let src_path = PathBuf::from(&src.path);
        let name = src_path
            .file_name()
            .ok_or_else(|| format!("invalid file {}", src.path))?
            .to_string_lossy()
            .into_owned();
        if src_path.is_dir() {
            return Err(format!("{name} is a folder; only files can be uploaded"));
        }
        let rel = if target_dir.is_empty() { name } else { format!("{target_dir}/{name}") };
        if !overwrite.unwrap_or(false) && project.resolve(&rel).map_err(anyhow_err)?.exists() {
            return Err(format!("{rel} already exists"));
        }
        let bytes = tokio::fs::read(&src_path).await.map_err(err)?;
        project.write(&rel, &bytes).await.map_err(anyhow_err)?;
        imported.push(rel);
    }
    Ok(imported)
}

/// Copies a project file (typically the output PDF) to a location chosen by the user.
#[tauri::command]
async fn export_file(state: State<'_, AppState>, path: String, destination: String) -> CmdResult<()> {
    let src = state.project()?.resolve(&path).map_err(anyhow_err)?;
    let bytes = tokio::fs::read(&src).await.map_err(err)?;
    // Write only the contents into a brand-new file. `fs::copy` would also carry over
    // macOS file flags, and build output inside the hidden `.texcraft` folder is flagged
    // `hidden`, which made exported PDFs invisible in Finder. Removing an existing file
    // first matters too: overwriting it in place would keep its old flags.
    match tokio::fs::remove_file(&destination).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(err(e)),
    }
    tokio::fs::write(&destination, bytes).await.map_err(err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolInfo { name: String, path: Option<String> }

#[tauri::command]
fn list_optional_tools() -> Vec<ToolInfo> {
    ["pdflatex", "xelatex", "lualatex", "latexmk", "tectonic", "synctex", "tex-fmt", "texlab", "texquill", "rtex", "pandoc"]
        .into_iter().map(|name| ToolInfo { name: name.into(), path: compiler::find_in_path(name).map(|p| p.to_string_lossy().into_owned()) }).collect()
}

/// Format an in-memory buffer. The caller applies it only if that buffer has not changed.
#[tauri::command]
async fn format_tex_buffer(content: String) -> CmdResult<String> {
    use tokio::io::AsyncWriteExt;
    use std::process::Stdio;
    if content.len() > texcraft_core::project::MAX_TEXT_FILE_BYTES as usize { return Err("File is too large to format.".into()); }
    let binary = compiler::find_in_path("tex-fmt").ok_or_else(|| "tex-fmt was not found in PATH. See Toolchain & Diagnostics for installation guidance.".to_string())?;
    let mut child = tokio::process::Command::new(binary).arg("--stdin")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true)
        .spawn().map_err(err)?;
    let mut stdin = child.stdin.take().ok_or("Could not open formatter input.")?;
    let job = async move {
        let write = async move { stdin.write_all(content.as_bytes()).await };
        let (written, output) = tokio::join!(write, child.wait_with_output());
        written.map_err(err)?;
        let output = output.map_err(err)?;
        if !output.status.success() { return Err(format!("tex-fmt failed: {}", String::from_utf8_lossy(&output.stderr).trim())); }
        String::from_utf8(output.stdout).map_err(err)
    };
    tokio::time::timeout(Duration::from_secs(15), job).await.map_err(|_| "tex-fmt timed out".to_string())?
}

#[tauri::command]
fn list_available_engines() -> Vec<EngineInfo> {
    TeXEngine::ALL
        .iter()
        .map(|&engine| {
            let path = engine.locate();
            EngineInfo {
                engine,
                installed: path.is_some(),
                path: path.map(|p| p.to_string_lossy().into_owned()),
            }
        })
        .collect()
}

#[tauri::command]
fn cancel_compile(state: State<'_, AppState>, request_id: String) {
    state.compile_cancel.cancel(&request_id);
}

#[tauri::command]
async fn compile_tex(
    app: AppHandle,
    state: State<'_, AppState>,
    entry_point: String,
    engine: TeXEngine,
    options: Option<CompileOptions>,
    request_id: Option<String>,
) -> CmdResult<Value> {
    let project = state.project()?;
    let source = project.read_text(&entry_point).await.map_err(anyhow_err)?;
    let entry = project.resolve_tex_root(&entry_point).await.map_err(anyhow_err)?;
    let root_source = project.read_text(&project.relative(&entry)).await.map_err(anyhow_err)?;
    let directive = compiler::program_directive(&root_source).or_else(|| compiler::program_directive(&source));
    let engine = directive.unwrap_or(engine);
    if directive.is_some() && engine.locate().is_none() {
        return Err(format!("This document requests {engine}, but it is not installed."));
    }
    let output_dir = project.resolve(DEFAULT_OUTPUT_DIR).map_err(anyhow_err)?;
    let mut opts = options.unwrap_or_default();
    opts.synctex = true;

    // Register before waiting for the lock: even a queued build can be cancelled.
    static NEXT_LEGACY_ID: AtomicU64 = AtomicU64::new(0);
    let id = request_id.unwrap_or_else(|| format!("legacy-{}", NEXT_LEGACY_ID.fetch_add(1, Ordering::Relaxed)));
    let mut cancel = state.compile_cancel.register(&id);
    let _registration = build_cancel::Registration { registry: &state.compile_cancel, id: &id };
    if *cancel.borrow() { return Err("Build cancelled".into()); }
    let _guard = tokio::select! {
        guard = state.compile_lock.lock() => guard,
        _ = cancel.changed() => return Err("Build cancelled".into()),
    };
    if *cancel.borrow() { return Err("Build cancelled".into()); }
    state.compiling.store(true, Ordering::SeqCst);
    let _ = app.emit("compile://started", json!({ "entryPoint": entry_point, "engine": engine }));
    // Dropping the compiler future kills its direct child (kill_on_drop in run_tool).
    let outcome = tokio::select! {
        result = compiler::compile_with(engine, &entry, &output_dir, &opts) => Some(result),
        _ = cancel.changed() => None,
    };
    state.compiling.store(false, Ordering::SeqCst);
    let Some(outcome) = outcome else { return Err("Build cancelled".into()); };
    let payload = match outcome {
        Ok(result) => project.relativize_compilation(&entry, &result),
        Err(e) => {
            let msg = anyhow_err(e);
            let _ = app.emit("compile://failed", json!({ "message": msg }));
            return Err(msg);
        }
    };
    let _ = app.emit("compile://finished", &payload);
    Ok(payload)
}

#[tauri::command]
async fn clear_build_dir(state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state.project()?.resolve(DEFAULT_OUTPUT_DIR).map_err(anyhow_err)?;
    if dir.exists() {
        tokio::fs::remove_dir_all(&dir).await.map_err(err)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SyncTeX
// ---------------------------------------------------------------------------

async fn output_pdf(project: &Project, main_document: &str) -> CmdResult<PathBuf> {
    let root = project.resolve_tex_root(main_document).await.map_err(anyhow_err)?;
    let stem = root
        .file_stem()
        .ok_or("invalid main document")?
        .to_string_lossy()
        .into_owned();
    let pdf = project
        .resolve(&format!("{DEFAULT_OUTPUT_DIR}/{stem}.pdf"))
        .map_err(anyhow_err)?;
    if !pdf.is_file() {
        return Err("compile the project first".into());
    }
    Ok(pdf)
}

#[tauri::command]
async fn synctex_forward(
    state: State<'_, AppState>,
    main_document: String,
    file: String,
    line: u32,
    column: u32,
) -> CmdResult<Vec<synctex::PdfLocation>> {
    let project = state.project()?;
    let pdf = output_pdf(&project, &main_document).await?;
    let source = project.resolve(&file).map_err(anyhow_err)?;
    synctex::forward(&pdf, &source, line, column).await.map_err(anyhow_err)
}

#[tauri::command]
async fn synctex_inverse(
    state: State<'_, AppState>,
    main_document: String,
    page: u32,
    x: f64,
    y: f64,
) -> CmdResult<Option<SyncTexSource>> {
    let project = state.project()?;
    let pdf = output_pdf(&project, &main_document).await?;
    let Some(loc) = synctex::inverse(&pdf, page, x, y).await.map_err(anyhow_err)? else {
        return Ok(None);
    };
    // SyncTeX reports paths like `/root/./chapters/intro.tex`; only files inside the project are openable.
    let file = loc
        .file
        .canonicalize()
        .ok()
        .filter(|p| p.starts_with(project.root()))
        .map(|p| project.relative(&p));
    Ok(file.map(|file| SyncTexSource { file, line: loc.line }))
}

#[tauri::command]
fn synctex_available() -> bool {
    synctex::is_available()
}

/// Opens a folder or URL with the system's default handler (Finder, Explorer, xdg-open).
fn open_with_system(target: &str) -> CmdResult<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(target).spawn().map_err(err)?;
    Ok(())
}

/// Opens the system file manager on a folder (Finder on macOS).
#[tauri::command]
fn reveal_in_file_manager(path: String) -> CmdResult<()> {
    open_with_system(&path)
}

/// Opens a web page in the default browser (Help menu links).
#[tauri::command]
fn open_external(url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("only https links can be opened".into());
    }
    open_with_system(&url)
}

/// Project folder passed on the command line (`LocalLeaf path/to/project`), if any.
#[tauri::command]
fn initial_project() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-') && Path::new(a).is_dir())
        .and_then(|a| std::fs::canonicalize(a).ok())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Called by the UI after it has flushed pending saves in response to `app://exit-requested`.
#[tauri::command]
fn confirm_exit(app: AppHandle, state: State<'_, AppState>) {
    tracing::info!("pending edits saved, exiting");
    state.exit_allowed.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// Surfaces webview errors in the terminal, where `tauri dev` users look first.
#[tauri::command]
fn frontend_log(level: String, message: String) {
    match level.as_str() {
        "error" => tracing::error!(target: "frontend", "{message}"),
        "warn" => tracing::warn!(target: "frontend", "{message}"),
        _ => tracing::info!(target: "frontend", "{message}"),
    }
}

// ---------------------------------------------------------------------------
// File watching
// ---------------------------------------------------------------------------

/// Watches the project and emits debounced change events:
/// - `project://changed` `{ paths }` for source edits (by the user elsewhere or by an MCP agent)
/// - `project://pdf-changed` when a PDF is rebuilt outside this app (e.g. an agent ran `compile_tex`)
fn start_watcher(app: AppHandle, root: PathBuf, compiling: Arc<AtomicBool>) -> notify::Result<RecommendedWatcher> {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        // Ends when the watcher (and therefore the sender) is dropped.
        while let Ok(first) = rx.recv() {
            let mut batch = vec![first];
            let deadline = Instant::now() + Duration::from_millis(250);
            while let Ok(ev) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                batch.push(ev);
            }

            let mut changed = BTreeSet::new();
            let mut pdf_changed = false;
            for ev in batch.into_iter().flatten() {
                if ev.kind.is_access() {
                    continue;
                }
                for path in ev.paths {
                    let Ok(rel) = path.strip_prefix(&root) else { continue };
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    if rel.starts_with(DEFAULT_OUTPUT_DIR) {
                        pdf_changed |= rel.ends_with(".pdf");
                        continue;
                    }
                    let name = rel.rsplit('/').next().unwrap_or("");
                    // Compare whole folder names: `.gitignore` and `.github/` are project files.
                    let top = rel.split('/').next().unwrap_or("");
                    if rel.is_empty()
                        || top == ".git"
                        || top == ".texcraft"
                        || name == ".DS_Store"
                        || name.ends_with(".texcraft-tmp")
                    {
                        continue;
                    }
                    changed.insert(rel);
                }
            }
            if !changed.is_empty() {
                let _ = app.emit("project://changed", json!({ "paths": changed }));
            }
            if pdf_changed && !compiling.load(Ordering::SeqCst) {
                let _ = app.emit("project://pdf-changed", ());
            }
        }
    });
    Ok(watcher)
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    texcraft_core::env::fix_path();

    // `LocalLeaf --mcp [--root DIR]` runs the headless MCP server instead of the GUI.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--mcp") {
        let root = args
            .iter()
            .position(|a| a == "--root")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        if let Err(e) = runtime.block_on(texcraft_core::mcp_server::run_with_mode(root, args.iter().any(|a| a == "--mcp-read-only"))) {
            eprintln!("texcraft MCP server error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(copilot::CopilotState::default())
        .manage(menu::MenuState::default())
        .setup(|app| {
            menu::install(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            copilot::copilot_auth_status,
            copilot::copilot_github_login,
            copilot::copilot_use_cli_credentials,
            copilot::copilot_cancel_login,
            copilot::copilot_logout,
            copilot::copilot_list_models,
            copilot::copilot_send_message,
            copilot::copilot_reset_session,
            copilot::copilot_stop,
            list_optional_tools,
            format_tex_buffer,
            open_project,
            create_project,
            close_project,
            rename_open_project,
            save_project_settings,
            get_tree,
            read_project_file,
            read_binary_file,
            write_project_file,
            create_file,
            create_folder,
            rename_path,
            delete_path,
            import_files,
            export_file,
            list_available_engines,
            compile_tex,
            cancel_compile,
            clear_build_dir,
            synctex_forward,
            synctex_inverse,
            synctex_available,
            initial_project,
            confirm_exit,
            reveal_in_file_manager,
            open_external,
            menu::update_menu_state,
            projects::rename_project_folder,
            projects::move_project_folder,
            projects::trash_project_folder,
            frontend_log,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LocalLeaf")
        .run(|app, event| {
            // Quitting (⌘Q, Dock → Quit) would drop edits still waiting for autosave: let the
            // UI save first. It answers with `confirm_exit`; a timeout guards against a hung UI.
            if let tauri::RunEvent::Exit = event { tauri::async_runtime::block_on(copilot::shutdown(app)); return; }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<AppState>();
                if state.exit_allowed.load(Ordering::SeqCst) || app.webview_windows().is_empty() {
                    return;
                }
                api.prevent_exit();
                let _ = app.emit("app://exit-requested", ());
                let handle = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    handle.state::<AppState>().exit_allowed.store(true, Ordering::SeqCst);
                    handle.exit(0);
                });
            }
        });
}
