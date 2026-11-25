//! LocalLeaf's Copilot integration. Project data is sent only when a user asks.
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use copilot_sdk::{Client, ClientMode, ClientOptions, CreateSessionOptions};
use copilot_sdk::session::Session;
use copilot_sdk::types::{McpServerConfig, McpStdioServerConfig, MessageOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use crate::{AppState, CmdResult, err};

const SERVICE: &str = "me.aliperti.texcraft.copilot";
const ACCOUNT: &str = "github-oauth";

#[derive(Default)]
struct Inner {
    client: Option<Arc<Client>>,
    session: Option<Arc<Session>>,
    session_root: Option<String>,
    signed_out: bool,
}
#[derive(Default)]
pub struct CopilotState {
    inner: Mutex<Inner>,
    generation: AtomicU64,
    busy: AtomicBool,
    turn_generation: AtomicU64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus { signed_in: bool, login: Option<String>, auth_type: Option<String>, message: Option<String> }
#[derive(Serialize)]
pub struct CopilotModel { id: String, name: String }
#[derive(Deserialize)]
struct DeviceReply { device_code: String, user_code: String, verification_uri: String, expires_in: u64, interval: u64 }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo { user_code: String, verification_uri: String, expires_in: u64 }

fn stored_token() -> Option<String> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()?.get_password().ok()
}
async fn get_client(state: &CopilotState) -> CmdResult<Arc<Client>> {
    let mut inner = state.inner.lock().await;
    if inner.signed_out { return Err("Signed out. Sign in or reconnect Copilot CLI credentials.".into()); }
    if let Some(client) = &inner.client { return Ok(client.clone()); }
    let token = tokio::task::spawn_blocking(stored_token).await.map_err(err)?;
    let mut opts = ClientOptions::new(ClientMode::CodingAgent);
    if let Some(token) = token { opts = opts.with_github(|g| g.with_token(token).with_use_logged_in_user(false)); }
    let client = Arc::new(Client::start(opts).await.map_err(err)?);
    inner.client = Some(client.clone());
    Ok(client)
}
async fn reset(state: &CopilotState) {
    state.turn_generation.fetch_add(1, Ordering::SeqCst);
    let session = { let mut inner = state.inner.lock().await; inner.session_root = None; inner.session.take() };
    if let Some(session) = session { let _ = session.abort().await; let _ = session.disconnect().await; }
    state.busy.store(false, Ordering::SeqCst);
}
async fn stop(state: &CopilotState) {
    reset(state).await;
    let client = { state.inner.lock().await.client.take() };
    if let Some(client) = client { let _ = client.stop().await; }
}
pub async fn shutdown(app: &AppHandle) { stop(&app.state::<CopilotState>()).await; }

#[tauri::command]
pub async fn copilot_auth_status(state: State<'_, CopilotState>) -> CmdResult<AuthStatus> {
    if state.inner.lock().await.signed_out { return Ok(AuthStatus { signed_in: false, login: None, auth_type: None, message: None }); }
    let status = get_client(&state).await?.get_auth_status().await.map_err(err)?;
    Ok(AuthStatus { signed_in: status.is_authenticated, login: status.login, auth_type: status.auth_type, message: status.status_message })
}
#[tauri::command]
pub async fn copilot_list_models(state: State<'_, CopilotState>) -> CmdResult<Vec<CopilotModel>> {
    Ok(get_client(&state).await?.list_models().await.map_err(err)?.into_iter().map(|m| CopilotModel { id:m.id, name:m.name }).collect())
}

#[tauri::command]
pub async fn copilot_github_login(app: AppHandle, state: State<'_, CopilotState>, client_id: String) -> CmdResult<DeviceInfo> {
    let client_id = client_id.trim().to_string();
    if client_id.is_empty() || client_id.len() > 200 { return Err("Enter a GitHub OAuth app client ID with device flow enabled, or sign in using the Copilot CLI.".into()); }
    let http = reqwest::Client::builder().user_agent("LocalLeaf-Copilot").timeout(Duration::from_secs(20)).build().map_err(err)?;
    let reply: DeviceReply = http.post("https://github.com/login/device/code").header("Accept", "application/json")
        .form(&[("client_id", client_id.as_str()), ("scope", "read:user")]).send().await.map_err(err)?
        .error_for_status().map_err(err)?.json().await.map_err(err)?;
    // Never let a server-controlled URL become an arbitrary opener target.
    if reply.verification_uri != "https://github.com/login/device" { return Err("Unexpected GitHub verification URL".into()); }
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let result = DeviceInfo { user_code: reply.user_code.clone(), verification_uri: reply.verification_uri.clone(), expires_in: reply.expires_in };
    let handle = app.clone();
    tokio::spawn(async move {
        let mut interval = reply.interval.max(5); let deadline = Instant::now() + Duration::from_secs(reply.expires_in);
        loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            let state = handle.state::<CopilotState>();
            if state.generation.load(Ordering::SeqCst) != generation { return; }
            if Instant::now() >= deadline { let _ = handle.emit("copilot:login-error", "The device code expired. Start sign-in again."); return; }
            let response = http.post("https://github.com/login/oauth/access_token").header("Accept", "application/json")
                .form(&[("client_id",client_id.as_str()),("device_code",reply.device_code.as_str()),("grant_type","urn:ietf:params:oauth:grant-type:device_code")]).send().await;
            let value: Value = match response { Ok(r) => match r.json().await { Ok(v) => v, Err(_) => { let _ = handle.emit("copilot:login-error", "Invalid GitHub response."); return; } }, Err(_) => { let _ = handle.emit("copilot:login-error", "Could not contact GitHub."); return; } };
            if let Some(token) = value.get("access_token").and_then(Value::as_str) {
                let token = token.to_owned();
                // Require the OS credential store; never fall back to plaintext settings.
                let guard = state.inner.lock().await;
                if state.generation.load(Ordering::SeqCst) != generation { return; }
                let saved = tokio::task::spawn_blocking(move || keyring::Entry::new(SERVICE, ACCOUNT).and_then(|e| e.set_password(&token))).await;
                drop(guard);
                if !matches!(saved, Ok(Ok(()))) { let _ = handle.emit("copilot:login-error", "Could not save the token in the system credential store."); return; }
                if state.generation.load(Ordering::SeqCst) != generation { return; }
                stop(&state).await;
                let mut inner = state.inner.lock().await;
                if state.generation.load(Ordering::SeqCst) != generation { return; }
                inner.signed_out = false;
                drop(inner);
                let _ = handle.emit("copilot:login-complete", ());
                return;
            }
            match value.get("error").and_then(Value::as_str) {
                Some("authorization_pending") => {}, Some("slow_down") => interval += 5,
                Some(code) => { let _ = handle.emit("copilot:login-error", format!("GitHub sign-in failed: {code}")); return; },
                None => { let _ = handle.emit("copilot:login-error", "GitHub did not return a token."); return; }
            }
        }
    });
    Ok(result)
}
#[tauri::command]
pub fn copilot_cancel_login(state: State<'_, CopilotState>) { state.generation.fetch_add(1, Ordering::SeqCst); }
#[tauri::command]
pub async fn copilot_use_cli_credentials(state: State<'_, CopilotState>) -> CmdResult<()> {
    state.generation.fetch_add(1, Ordering::SeqCst); stop(&state).await;
    state.inner.lock().await.signed_out = false; Ok(())
}
#[tauri::command]
pub async fn copilot_logout(state: State<'_, CopilotState>) -> CmdResult<()> {
    state.generation.fetch_add(1, Ordering::SeqCst);
    state.inner.lock().await.signed_out = true; stop(&state).await;
    let result = tokio::task::spawn_blocking(|| keyring::Entry::new(SERVICE, ACCOUNT).and_then(|e| e.delete_credential())).await.map_err(err)?;
    if let Err(e) = result { if !matches!(e, keyring::Error::NoEntry) { return Err(err(e)); } }
    Ok(())
}
#[tauri::command]
pub async fn copilot_reset_session(state: State<'_, CopilotState>) -> CmdResult<()> { reset(&state).await; Ok(()) }
#[tauri::command]
pub async fn copilot_stop(state: State<'_, CopilotState>) -> CmdResult<()> {
    let session = state.inner.lock().await.session.clone();
    if let Some(session) = session { session.abort().await.map_err(err)?; }
    Ok(())
}

async fn session_for(app: &AppHandle, state: &CopilotState, root: String, model: Option<String>, allow_read_tools: bool, turn: u64) -> CmdResult<Arc<Session>> {
    let client = get_client(state).await?;
    let mut inner = state.inner.lock().await;
    if state.turn_generation.load(Ordering::SeqCst) != turn { return Err("Request cancelled.".into()); }
    if inner.session_root.as_deref() == Some(root.as_str()) {
        if let Some(session) = &inner.session { return Ok(session.clone()); }
    }
    if let Some(old) = inner.session.take() { let _ = old.disconnect().await; }
    let mut options = CreateSessionOptions::default();
    options.working_directory = Some(root.clone().into()); options.model = model.filter(|s| !s.is_empty());
    options.client_name = Some("LocalLeaf".into()); options.events.streaming = Some(true);
    // Fail closed: explicitly expose only read-only MCP tools. Writes are not enabled until
    // per-invocation approval can be independently enforced for every runtime version.
    options.enable_config_discovery = Some(false);
    let exe = std::env::current_exe().map_err(err)?;
    let server = McpStdioServerConfig { command: exe.to_string_lossy().into_owned(), args: vec!["--mcp".into(), "--mcp-read-only".into(), "--root".into(), root.clone()], tools: Some(vec!["read_project_file".into(),"list_available_engines".into()]), ..Default::default() };
    options.tools.available_tools = Some(Vec::new());
    if allow_read_tools {
        options.mcp.servers = Some([("localleaf".to_string(), McpServerConfig::Stdio(server))].into_iter().collect());
        options.tools.available_tools = Some(vec!["mcp:localleaf-read_project_file".into(),"mcp:localleaf-list_available_engines".into()]);
    }
    options = options.with_permissions(|p| p.deny_all_permissions());
    let session = Arc::new(client.create_session(options).await.map_err(err)?);
    if state.turn_generation.load(Ordering::SeqCst) != turn { let _ = session.disconnect().await; return Err("Request cancelled.".into()); }
    inner.session_root = Some(root); inner.session = Some(session.clone());
    let _ = app; Ok(session)
}
#[tauri::command]
pub async fn copilot_send_message(app: AppHandle, state: State<'_, CopilotState>, project: State<'_, AppState>, prompt: String, model: Option<String>, allow_read_tools: bool) -> CmdResult<()> {
    if prompt.trim().is_empty() { return Err("Enter a message first.".into()); }
    if state.busy.swap(true, Ordering::SeqCst) { return Err("A response is already in progress.".into()); }
    let turn = state.turn_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let root = match project.open_root() { Some(r) => r.to_string_lossy().into_owned(), None => { state.busy.store(false, Ordering::SeqCst); return Err("Open a project first.".into()); } };
    let session = match session_for(&app, &state, root, model, allow_read_tools, turn).await { Ok(s) => s, Err(e) => { if state.turn_generation.load(Ordering::SeqCst) == turn { state.busy.store(false, Ordering::SeqCst); } return Err(e); } };
    if state.turn_generation.load(Ordering::SeqCst) != turn { return Err("Request cancelled.".into()); }
    let mut events = session.subscribe();
    let handle = app.clone();
    tokio::spawn(async move {
        let wait = session.send_and_wait(MessageOptions::new(prompt).with_wait_timeout(Duration::from_secs(300)));
        tokio::pin!(wait);
        let mut streamed = String::new();
        loop {
            tokio::select! {
                result = &mut wait => {
                    if handle.state::<CopilotState>().turn_generation.load(Ordering::SeqCst) != turn { return; }
                    match result {
                        Ok(response) => {
                            if let Some(content) = response.as_ref().and_then(|e| e.data.get("content")).and_then(Value::as_str) {
                                if let Some(rest) = content.strip_prefix(&streamed) { if !rest.is_empty() { let _ = handle.emit("copilot:chunk", rest); } }
                            }
                            let _ = handle.emit("copilot:done", ());
                        }
                        Err(e) => { let _ = handle.emit("copilot:error", err(e)); }
                    }
                    break;
                }
                event = events.recv() => {
                    if handle.state::<CopilotState>().turn_generation.load(Ordering::SeqCst) != turn { return; }
                    match event {
                        Ok(event) if event.event_type == "assistant.message_delta" => {
                            if let Some(delta) = event.data.get("deltaContent").and_then(Value::as_str) {
                                streamed.push_str(delta); let _ = handle.emit("copilot:chunk", delta);
                            }
                        }
                        Ok(_) => {},
                        Err(e) => { let _ = session.abort().await; let _ = handle.emit("copilot:error", format!("Copilot stream interrupted: {e}")); break; }
                    }
                }
            }
        }
        let state = handle.state::<CopilotState>();
        if state.turn_generation.load(Ordering::SeqCst) == turn { state.busy.store(false, Ordering::SeqCst); }
    });
    Ok(())
}
