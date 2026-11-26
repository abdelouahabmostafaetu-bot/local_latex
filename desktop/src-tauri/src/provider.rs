//! Explicit, user-configured OpenAI-compatible HTTP chat. No credentials are persisted.
use std::time::Duration;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use crate::{CmdResult, err};

#[derive(Default)]
pub struct ProviderState { task: Mutex<Option<JoinHandle<()>>> }

#[derive(Deserialize)]
pub struct ChatMessage { role: String, content: String }

fn endpoint(base: &str) -> CmdResult<reqwest::Url> {
    let mut url = reqwest::Url::parse(base.trim()).map_err(|_| "Enter a valid provider base URL.")?;
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("The provider URL must not include credentials, a query, or a fragment.".into());
    }
    let host = url.host_str().ok_or("Provider URL needs a host.")?;
    let local = host.eq_ignore_ascii_case("localhost") || host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err("Use HTTPS for remote providers; HTTP is allowed only on localhost.".into());
    }
    let path = format!("{}/chat/completions", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

// Consume complete SSE data lines, retaining partial UTF-8 sequences in the byte buffer.
fn drain_sse(buffer: &mut Vec<u8>) -> CmdResult<(Vec<String>, bool)> {
    let mut deltas = Vec::new();
    let mut done = false;
    while let Some(end) = buffer.iter().position(|&b| b == b'\n') {
        if end > 1024 * 1024 { return Err("Provider response line is too large.".into()); }
        let line: Vec<u8> = buffer.drain(..=end).collect();
        let line = std::str::from_utf8(&line).map_err(|_| "Provider sent invalid UTF-8.")?.trim_end_matches(['\r', '\n']);
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim_start();
            if data == "[DONE]" { done = true; continue; }
            if data.is_empty() { continue; }
            let v: Value = serde_json::from_str(data).map_err(|_| "Provider sent invalid streaming JSON.")?;
            if v.get("error").is_some() { return Err("Provider reported a streaming error.".into()); }
            if let Some(text) = v.pointer("/choices/0/delta/content").and_then(Value::as_str) { deltas.push(text.to_owned()); }
        }
    }
    if buffer.len() > 1024 * 1024 { return Err("Provider response line is too large.".into()); }
    Ok((deltas, done))
}

#[tauri::command]
pub async fn provider_send(app: AppHandle, state: State<'_, ProviderState>, base_url: String, api_key: String, model: String, messages: Vec<ChatMessage>, request_id: u64) -> CmdResult<()> {
    let url = endpoint(&base_url)?;
    if model.trim().is_empty() || model.len() > 256 { return Err("Enter a model name.".into()); }
    if api_key.len() > 4096 || messages.is_empty() || messages.len() > 100 || messages.iter().any(|m| !matches!(m.role.as_str(), "user" | "assistant") || m.content.len() > 1_000_000) {
        return Err("Invalid or oversized provider request.".into());
    }
    if messages.iter().map(|m| m.content.len()).sum::<usize>() > 2_000_000 { return Err("Conversation is too large; start a new chat.".into()); }
    let mut task = state.task.lock().await;
    if task.as_ref().is_some_and(|t| !t.is_finished()) { return Err("A provider response is already in progress.".into()); }
    let body = json!({"model": model.trim(), "messages": messages.iter().map(|m| json!({"role":m.role,"content":m.content})).collect::<Vec<_>>(), "stream": true});
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(300)).build().map_err(err)?;
    *task = Some(tokio::spawn(async move {
        let result = async {
            let mut request = client.post(url).json(&body);
            if !api_key.is_empty() { request = request.bearer_auth(api_key); }
            let mut response = request.send().await.map_err(|_| "Could not connect to the selected provider.")?;
            if !response.status().is_success() { return Err(format!("Provider returned HTTP {}.", response.status().as_u16())); }
            let mut buffer = Vec::new();
            let mut done = false;
            let mut output_size = 0usize;
            while let Some(bytes) = response.chunk().await.map_err(|_| "Provider stream was interrupted.")? {
                buffer.extend_from_slice(&bytes);
                let (deltas, complete) = drain_sse(&mut buffer)?;
                for delta in deltas { output_size += delta.len(); if output_size > 4_000_000 { return Err("Provider response is too large.".into()); } let _ = app.emit("provider:chunk", json!({"requestId": request_id, "text": delta})); }
                if complete { done = true; break; }
            }
            if !done { return Err("Provider stream ended before completion.".to_string()); }
            Ok(())
        }.await;
        match result { Ok(()) => { let _ = app.emit("provider:done", request_id); }, Err(e) => { let _ = app.emit("provider:error", json!({"requestId": request_id, "message": e})); } }
    }));
    Ok(())
}

#[tauri::command]
pub async fn provider_stop(state: State<'_, ProviderState>) -> CmdResult<()> {
    if let Some(task) = state.task.lock().await.take() { task.abort(); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn url_policy() {
        assert_eq!(endpoint("http://localhost:11434/v1/").unwrap().as_str(), "http://localhost:11434/v1/chat/completions");
        assert!(endpoint("http://example.com/v1").is_err());
        assert!(endpoint("https://user:secret@example.com/v1").is_err());
        assert!(endpoint("https://example.com/v1?key=secret").is_err());
        assert!(endpoint("https://example.com/v1").is_ok());
    }
    #[test] fn fragmented_sse() {
        let source = "data: {\"choices\":[{\"delta\":{\"content\":\"مرحبا\"}}]}\r\n\r\ndata: [DONE]\n\n".as_bytes();
        let mut buffer = Vec::new(); let mut result = Vec::new(); let mut complete = false;
        for byte in source { buffer.push(*byte); let (chunks, done) = drain_sse(&mut buffer).unwrap(); result.extend(chunks); complete |= done; }
        assert_eq!(result, vec!["مرحبا"]); assert!(complete); assert!(buffer.is_empty());
    }
}
