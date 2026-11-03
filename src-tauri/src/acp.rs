//! Minimal Agent Client Protocol client: JSON-RPC over the agent's stdin/stdout,
//! one JSON message per line. The agent is Claude Code's ACP adapter, so the
//! coach runs on your Claude Code login (no API key).
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const STOPPED: &str = "the Claude agent process stopped";

enum Event {
    Update(Value),
    Done(Result<Value, String>),
}

type Waiting = Arc<Mutex<HashMap<u64, Sender<Event>>>>;
type Sessions = Arc<Mutex<HashMap<String, Sender<Event>>>>;

pub struct Acp {
    child: Mutex<Child>,
    stdin: Arc<Mutex<ChildStdin>>,
    next: AtomicU64,
    waiting: Waiting,
    sessions: Sessions,
    workdir: PathBuf,
}

fn send(stdin: &Mutex<ChildStdin>, msg: &Value) -> Result<(), String> {
    let mut w = stdin.lock().unwrap();
    writeln!(w, "{msg}").and_then(|_| w.flush()).map_err(|_| STOPPED.to_string())
}

fn read_loop(stdout: ChildStdout, stdin: Arc<Mutex<ChildStdin>>, waiting: Waiting, sessions: Sessions) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        match (msg["method"].as_str(), msg.get("id")) {
            // The agent asks us something. The coach has no tools, so we allow nothing.
            (Some(method), Some(id)) => {
                let reply = if method == "session/request_permission" {
                    json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": {"outcome": "cancelled"}}})
                } else {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "not supported"}})
                };
                let _ = send(&stdin, &reply);
            }
            (Some("session/update"), None) => {
                let sid = msg["params"]["sessionId"].as_str().unwrap_or("");
                if let Some(tx) = sessions.lock().unwrap().get(sid) {
                    let _ = tx.send(Event::Update(msg["params"]["update"].clone()));
                }
            }
            (None, Some(id)) => {
                let tx = id.as_u64().and_then(|id| waiting.lock().unwrap().remove(&id));
                if let Some(tx) = tx {
                    let result = match msg.get("error") {
                        Some(e) => Err(e["message"].as_str().unwrap_or("unknown error").to_string()),
                        None => Ok(msg["result"].clone()),
                    };
                    let _ = tx.send(Event::Done(result));
                }
            }
            _ => {}
        }
    }
    for (_, tx) in waiting.lock().unwrap().drain() {
        let _ = tx.send(Event::Done(Err(STOPPED.into())));
    }
    sessions.lock().unwrap().clear();
}

impl Acp {
    /// `program` + `args` is the ACP agent to run (Claude Code's adapter by default, or any ACP-speaking command).
    pub fn start(program: &Path, args: &[String], workdir: &Path) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Cannot start the agent `{}` ({e}). Check Settings, or install Node.js if you use the bundled Claude Code agent.", program.display()))?;
        let stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));
        let stdout = child.stdout.take().unwrap();
        let (waiting, sessions) = (Waiting::default(), Sessions::default());
        {
            let (stdin, waiting, sessions) = (stdin.clone(), waiting.clone(), sessions.clone());
            std::thread::spawn(move || read_loop(stdout, stdin, waiting, sessions));
        }
        let acp = Acp { child: Mutex::new(child), stdin, next: AtomicU64::new(1), waiting, sessions, workdir: workdir.into() };
        acp.call("initialize", json!({"protocolVersion": 1, "clientCapabilities": {}}), 60)?;
        Ok(acp)
    }

    pub fn alive(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    fn request(&self, method: &str, params: Value, into: Sender<Event>) -> Result<(), String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        self.waiting.lock().unwrap().insert(id, into);
        send(&self.stdin, &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    fn call(&self, method: &str, params: Value, secs: u64) -> Result<Value, String> {
        let (tx, rx) = channel();
        self.request(method, params, tx)?;
        match rx.recv_timeout(Duration::from_secs(secs)) {
            Ok(Event::Done(r)) => r,
            Ok(Event::Update(_)) => Err("unexpected message from the Claude agent".into()),
            Err(RecvTimeoutError::Timeout) => Err("the Claude agent did not answer in time".into()),
            Err(RecvTimeoutError::Disconnected) => Err(STOPPED.into()),
        }
    }

    /// Runs one coach turn in a fresh session. Calls `on_text` for each streamed
    /// chunk and returns the stop reason.
    pub fn ask(&self, system: &str, prompt: &str, images: &[(String, String)], run: &RunConfig, mut on_text: impl FnMut(&str)) -> Result<Option<String>, String> {
        // Claude Code reads these; another ACP agent ignores `_meta.claudeCode` and gets `session/set_model` instead.
        let mut options = json!({"tools": [], "promptSuggestions": false, "settingSources": if run.use_my_settings { json!(["user"]) } else { json!([]) }});
        if !run.model.is_empty() { options["model"] = json!(run.model); }
        if !run.effort.is_empty() { options["effort"] = json!(run.effort); }
        if !run.skills.is_empty() { options["skills"] = json!(run.skills); }
        let session = self.call("session/new", json!({
            "cwd": self.workdir.to_string_lossy(),
            "mcpServers": [],
            "systemPrompt": {"mode": "override", "content": [{"type": "text", "text": system}]},
            "_meta": {"claudeCode": {"options": options}}
        }), 120)?;
        // `systemPrompt` on session/new is a Claude-adapter extension; any other agent gets the instructions in the prompt itself.
        let prompt_text = if run.claude { prompt.to_string() } else { format!("<instructions>\n{system}\n</instructions>\n\n{prompt}") };
        let prompt = prompt_text.as_str();
        if !run.claude && !run.model.is_empty() {
            let sid = session["sessionId"].as_str().unwrap_or("").to_string();
            let _ = self.call("session/set_model", json!({"sessionId": sid, "modelId": run.model}), 30);
        }
        let sid = session["sessionId"].as_str().ok_or("the Claude agent sent no session id")?.to_string();
        let (tx, rx) = channel();
        self.sessions.lock().unwrap().insert(sid.clone(), tx.clone());

        let mut blocks = vec![json!({"type": "text", "text": prompt})];
        for (mime, data) in images {
            blocks.push(json!({"type": "image", "mimeType": mime, "data": data}));
        }
        let mut result = self.request("session/prompt", json!({"sessionId": sid, "prompt": blocks}), tx).map(|_| None);
        if result.is_ok() {
            result = loop {
                match rx.recv_timeout(Duration::from_secs(600)) {
                    // Updates and the prompt's response share this channel, in the order the agent sent them.
                    Ok(Event::Done(r)) => break r.map(|v| v["stopReason"].as_str().map(String::from)),
                    Ok(Event::Update(u)) => {
                        if u["sessionUpdate"] == "agent_message_chunk" && u["content"]["type"] == "text" {
                            on_text(u["content"]["text"].as_str().unwrap_or(""));
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => break Err("The coach took too long to answer. Please try again.".into()),
                    Err(RecvTimeoutError::Disconnected) => break Err(STOPPED.into()),
                }
            };
        }
        self.sessions.lock().unwrap().remove(&sid);
        let (close_tx, _) = channel();
        let _ = self.request("session/close", json!({"sessionId": sid}), close_tx);
        result
    }
}

impl Drop for Acp {
    fn drop(&mut self) {
        let _ = self.child.get_mut().unwrap().kill();
    }
}

/// What a coach session runs with. Comes from Settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunConfig {
    pub claude: bool,
    pub model: String,
    pub effort: String,
    pub skills: Vec<String>,
    pub use_my_settings: bool,
}

#[cfg(test)]
mod tests {
    /// Talks to the real adapter with your Claude login: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn real_coach_turn() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let script = root.join("agent/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js");
        let workdir = std::env::temp_dir().join("docu-coach-test");
        std::fs::create_dir_all(&workdir).unwrap();
        let node = std::env::var("DOCU_COACH_NODE").unwrap_or_else(|_| "node".into());
        let acp = super::Acp::start(node.as_ref(), &[script.to_string_lossy().to_string()], &workdir).unwrap();
        let mut text = String::new();
        let stop = acp.ask("Reply with exactly: pong", "ping", &[], &super::RunConfig { claude: true, model: "claude-opus-5-5".into(), ..Default::default() }, |t| text.push_str(t)).unwrap();
        assert_eq!(stop.as_deref(), Some("end_turn"));
        assert!(text.to_lowercase().contains("pong"), "got: {text}");
    }
}

#[cfg(test)]
mod devin {
    /// A real turn through Devin's ACP server (`devin acp`), needs `devin auth login`.
    /// `cargo test devin -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_devin_turn() {
        let devin = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".local/bin/devin");
        let workdir = std::env::temp_dir().join("docu-coach-devin-test");
        std::fs::create_dir_all(&workdir).unwrap();
        let acp = super::Acp::start(&devin, &["acp".to_string()], &workdir).unwrap();
        let mut text = String::new();
        let stop = acp.ask("You are a test. Reply with exactly: pong", "ping", &[], &super::RunConfig { claude: false, ..Default::default() }, |t| text.push_str(t)).unwrap();
        println!("stop={stop:?} text={text}");
        assert!(text.to_lowercase().contains("pong"), "got: {text}");
    }
}

#[cfg(test)]
mod grounding {
    /// A real review with the real prompt: every quoted note must exist in the document.
    /// `cargo test grounding -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn quotes_are_grounded() {
        use crate::prompt;
        use serde_json::json;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let script = root.join("agent/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js");
        let workdir = std::env::temp_dir().join("docu-coach-test");
        std::fs::create_dir_all(&workdir).unwrap();
        let node = std::env::var("DOCU_COACH_NODE").unwrap_or_else(|_| "node".into());
        let acp = super::Acp::start(node.as_ref(), &[script.to_string_lossy().to_string()], &workdir).unwrap();
        let text = "Payments API get too many request from some client, it make slow for everyone. We see this mostly at month end when batch jobs run.\n\n- p99 goes from 200ms to 2s\n- support tickets double";
        let body = json!({
            "mode": "review", "current": "Context and problem", "title": "Rate limiter for the Payments API", "notes": "",
            "template": {"name": "Design doc", "sections": [{"title": "Context and problem", "hint": "What is broken?", "checks": "numbers"}]},
            "sections": [{"title": "Context and problem", "text": text}],
            "history": [{"role": "user", "content": "Review the \"Context and problem\" section."}]
        });
        let mut reply = String::new();
        acp.ask(prompt::SYSTEM, &prompt::build(&body), &[], &super::RunConfig { claude: true, model: "claude-opus-5-5".into(), ..Default::default() }, |t| reply.push_str(t)).unwrap();
        println!("{reply}");
        let squash = |s: &str| s.to_lowercase().replace(['“', '”', '"', '\'', '’'], "").split_whitespace().collect::<Vec<_>>().join(" ");
        let hay = squash(text);
        let mut notes = 0;
        for cap in reply.split("<note ").skip(1) {
            notes += 1;
            if let Some(q) = cap.split("quote=\"").nth(1).and_then(|r| r.split('"').next()) {
                assert!(hay.contains(&squash(q)), "quote not in document: {q}");
            }
        }
        assert!(notes >= 2, "expected a few notes, got {notes}");
        assert!(reply.contains("fix=\""), "expected at least one fix");
    }
}
