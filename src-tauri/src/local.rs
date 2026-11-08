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

/// One coach turn, streamed. Returns the finish reason when the server gives one.
pub fn ask(base: &str, model: &str, system: &str, prompt: &str, images: &[(String, String)], mut on_text: impl FnMut(&str)) -> Result<Option<String>, String> {
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
    let res = client(600)?.post(&url).json(&body).send().map_err(|e| format!("could not reach the local model at {url} ({e})"))?;
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
        let base = "http://localhost:11434";
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
        super::ask(base, &model, prompt::SYSTEM, &prompt::build(&body), &[], |t| reply.push_str(t)).unwrap();
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
