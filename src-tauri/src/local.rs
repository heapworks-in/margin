//! A local model as the coach: any server speaking the OpenAI chat API
//! (Ollama, LM Studio, llama.cpp, vLLM). Nothing leaves the machine.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

fn client(secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(secs)).build().map_err(|e| e.to_string())
}

fn trim_base(base: &str) -> String {
    let b = base.trim().trim_end_matches('/');
    if b.ends_with("/v1") { b.to_string() } else { format!("{b}/v1") }
}

/// Model ids the server offers (`GET /v1/models`).
pub fn models(base: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/models", trim_base(base));
    let res = client(5)?.get(&url).send().map_err(|e| format!("nothing answered at {url} ({e})"))?;
    if !res.status().is_success() { return Err(format!("{url} answered {}", res.status())); }
    let v: Value = res.json().map_err(|e| e.to_string())?;
    Ok(v["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default())
}

fn host(base: &str) -> String { trim_base(base).trim_end_matches("/v1").to_string() }

/// Is this Ollama? Its native API gives us `think: false` (no silent reasoning phase) and `keep_alive`.
pub fn is_ollama(base: &str) -> bool {
    client(3).ok().and_then(|c| c.get(format!("{}/api/version", host(base))).send().ok()).map(|r| r.status().is_success()).unwrap_or(false)
}

/// Ollama's own chat API, streamed as NDJSON. Thinking off, the model kept loaded for an hour.
fn ask_ollama(base: &str, model: &str, system: &str, prompt: &str, images: &[(String, String)], on_text: &mut impl FnMut(&str)) -> Result<Option<String>, String> {
    let url = format!("{}/api/chat", host(base));
    let mut user = json!({"role": "user", "content": prompt});
    if !images.is_empty() { user["images"] = json!(images.iter().map(|(_, d)| d.clone()).collect::<Vec<_>>()); }
    let body = json!({ "model": model, "stream": true, "think": false, "keep_alive": "1h",
        "messages": [{"role": "system", "content": system}, user] });
    let res = client(900)?.post(&url).json(&body).send().map_err(|e| format!("could not reach Ollama at {url} ({e})"))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().unwrap_or_default();
        return Err(format!("Ollama answered {status}: {}", text.chars().take(300).collect::<String>()));
    }
    let mut finish = None;
    for line in BufReader::new(res).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
        if let Some(e) = v["error"].as_str() { return Err(format!("Ollama: {e}")); }
        if let Some(t) = v["message"]["content"].as_str() { if !t.is_empty() { on_text(t); } }
        if v["done"].as_bool() == Some(true) { finish = v["done_reason"].as_str().map(|r| if r == "length" { "max_tokens".into() } else { r.to_string() }); break; }
    }
    Ok(finish)
}

/// Load the model and let the server cache the shared prompt prefix, so the first real turn is not the slow one.
/// Ollama only (its native API caches by prefix); a no-op elsewhere.
pub fn warm(base: &str, model: &str, system: &str) {
    if !is_ollama(base) { return; }
    let body = json!({ "model": model, "stream": false, "think": false, "keep_alive": "1h", "options": {"num_predict": 1},
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": "ok"}] });
    let _ = client(300).and_then(|c| c.post(format!("{}/api/chat", host(base))).json(&body).send().map_err(|e| e.to_string()));
}

/// One coach turn, streamed. Returns the finish reason when the server gives one.
pub fn ask(base: &str, model: &str, system: &str, prompt: &str, images: &[(String, String)], mut on_text: impl FnMut(&str)) -> Result<Option<String>, String> {
    if is_ollama(base) { return ask_ollama(base, model, system, prompt, images, &mut on_text); }
    let url = format!("{}/chat/completions", trim_base(base));
    let mut content = vec![json!({"type": "text", "text": prompt})];
    for (mime, data) in images {
        content.push(json!({"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{data}")}}));
    }
    let user: Value = if images.is_empty() { json!(prompt) } else { json!(content) };
    let body = json!({
        "model": model,
        "stream": true,
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
    });
    let res = client(900)?.post(&url).json(&body).send().map_err(|e| format!("could not reach the local model at {url} ({e})"))?;
    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().unwrap_or_default();
        return Err(format!("the local model answered {status}: {}", text.chars().take(300).collect::<String>()));
    }
    let mut finish = None;
    for line in BufReader::new(res).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data:") else { continue };
        let data = data.trim();
        if data == "[DONE]" { break; }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(t) = v["choices"][0]["delta"]["content"].as_str() { if !t.is_empty() { on_text(t); } }
        if let Some(r) = v["choices"][0]["finish_reason"].as_str() { finish = Some(match r { "length" => "max_tokens".to_string(), other => other.to_string() }); }
    }
    Ok(finish)
}

#[cfg(test)]
mod tests {
    /// The real coach prompt through a local model: does it produce grounded notes?
    /// `LOCAL_MODEL=gemma4:26b cargo test local_review -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn local_review_is_grounded() {
        use crate::prompt;
        use serde_json::json;
        let base_env = std::env::var("LOCAL_BASE").unwrap_or_else(|_| "http://localhost:11434".into());
        let base = base_env.as_str();
        let model = std::env::var("LOCAL_MODEL").unwrap_or_else(|_| "gemma4:26b".into());
        let text = "Payments API get too many request from some client, it make slow for everyone. We see this mostly at month end when batch jobs run.\n\n- p99 goes from 200ms to 2s\n- support tickets double";
        let body = json!({
            "mode": "review", "current": "Context and problem", "title": "Rate limiter for the Payments API", "notes": "",
            "template": {"name": "Design doc", "sections": [{"title": "Context and problem", "hint": "What is broken?", "checks": "clear problem, who is affected, evidence with numbers"}]},
            "sections": [{"title": "Context and problem", "text": text}],
            "history": [{"role": "user", "content": "Review the \"Context and problem\" section."}]
        });
        let started = std::time::Instant::now();
        let mut reply = String::new();
        super::ask(base, &model, prompt::SYSTEM_LOCAL, &prompt::build(&body), &[], |t| reply.push_str(t)).unwrap();
        println!("--- {model} in {:.0}s\n{reply}", started.elapsed().as_secs_f32());
        let squash = |s: &str| s.to_lowercase().replace(['\u{201c}', '\u{201d}', '"', '\'', '\u{2019}'], "").split_whitespace().collect::<Vec<_>>().join(" ");
        let hay = squash(text);
        let (mut notes, mut grounded) = (0, 0);
        for cap in reply.split("<note ").skip(1) {
            notes += 1;
            if let Some(q) = cap.split("quote=\"").nth(1).and_then(|r| r.split('"').next()) { if hay.contains(&squash(q)) { grounded += 1; } else { println!("ungrounded quote: {q}"); } }
        }
        println!("notes={notes} grounded={grounded}");
        assert!(notes >= 2, "expected notes from the local model");
    }

    /// Needs Ollama (or another server) at localhost:11434 with a model pulled.
    /// `cargo test local -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn local_model_coaches() {
        let base = "http://localhost:11434";
        let list = super::models(base).unwrap();
        println!("models: {list:?}");
        let model = list.iter().find(|m| !m.contains("cloud")).expect("a local model");
        let mut text = String::new();
        let stop = super::ask(base, model, "Reply with exactly: pong", "ping", &[], |t| text.push_str(t)).unwrap();
        println!("{model}: stop={stop:?} text={text}");
        assert!(text.to_lowercase().contains("pong"), "got: {text}");
    }
}

#[cfg(test)]
mod quick {
    /// A quick-tier interview turn on the local model, section-scoped: `cargo test local_quick -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn local_quick_turn() {
        use crate::prompt;
        use serde_json::json;
        let base = "http://localhost:11434";
        let model = std::env::var("LOCAL_MODEL").unwrap_or_else(|_| "gemma4:12b-it-qat".into());
        let body = json!({
            "mode": "interview", "scope": "section", "current": "Context and problem", "title": "Rate limiter for the Payments API", "notes": "",
            "template": {"name": "Design doc", "sections": [{"title": "Context and problem", "hint": "What is broken?", "checks": "clear problem, who is affected, evidence with numbers"}, {"title": "Proposed design", "hint": "How?", "checks": "flow"}]},
            "sections": [{"title": "Context and problem", "text": "Payments API get too many request from some client, it make slow for everyone."}, {"title": "Proposed design", "text": ""}],
            "history": [{"role": "user", "content": "Interview me for the \"Context and problem\" section. Ask me one question at a time."}]
        });
        let system = prompt::system_for("interview", "section", true);
        super::warm(base, &model, &system);
        let started = std::time::Instant::now();
        let mut reply = String::new();
        super::ask(base, &model, &system, &prompt::build(&body), &[], |t| reply.push_str(t)).unwrap();
        println!("--- quick turn {model}: {:.0}s\n{reply}", started.elapsed().as_secs_f32());
        assert!(reply.contains('?'), "should ask a question");
    }
}
