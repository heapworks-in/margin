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

#[cfg(test)]
mod prompts {
    //! Real-model checks of the coach prompt, one per behaviour the app parses.
    //! `cargo test prompts -- --ignored --nocapture --test-threads=1` (about half a minute each).
    use crate::prompt;
    use serde_json::{json, Value};

    const CONTEXT: &str = "The Payments API gets too many requests from a few clients at month end. p99 latency rise from 200 ms to 2.1 s for everyone, and the on-call engineer throttles clients by hand. Three incidents in the last quarter were caused by this.";
    const DESIGN: &str = "We propose a per-client token bucket in Redis. Each client gets 500 requests per second, refilled continuously. The limiter sits in the API gateway and answers 429 with a Retry-After header. Limits live in a config map and change without a deploy. If Redis is slow, the limiter fails open so no request is dropped.";
    const ALTS: &str = "- Do nothing: three more incidents a quarter, and the on-call keeps throttling by hand.\n- Quotas at the load balancer: cheaper, but it cannot see the client id behind the shared gateway key.\n- Scale the API: about $18k a month more, for a peak that lasts two days.";

    fn body(mode: &str, current: &str, history: Vec<Value>) -> Value {
        json!({
            "mode": mode, "current": current, "title": "Rate limiter for the Payments API", "notes": "",
            "audience": "the platform team lead", "purpose": "approve the Redis cluster by Friday",
            "brief": {"ask": "Approve a 3-node Redis cluster for per-client rate limits", "objection": "Why not just scale the API?", "deadline": "Friday", "numbers": "p99 200 ms to 2.1 s at month end; 3 incidents last quarter", "nongoals": "Billing"},
            "template": {"name": "Design doc", "sections": [
                {"title": "Context and problem", "hint": "What is broken?", "checks": "clear problem, who is affected, evidence with numbers, why now"},
                {"title": "Proposed design", "hint": "How does it work?", "checks": "end-to-end flow, interfaces, data model, scale and limits"},
                {"title": "Alternatives considered", "hint": "What else?", "checks": "at least two real alternatives incl. doing nothing, the reason each lost"},
                {"title": "Failure modes", "hint": "What breaks?", "checks": "failure modes listed, impact and detection, mitigation"}]},
            "sections": [{"title": "Context and problem", "text": CONTEXT}, {"title": "Proposed design", "text": DESIGN}, {"title": "Alternatives considered", "text": ALTS}, {"title": "Failure modes", "text": ""}],
            "history": history
        })
    }
    fn turn(mode: &str, current: &str, history: Vec<Value>) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let script = root.join("agent/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js");
        let workdir = std::env::temp_dir().join("docu-coach-test");
        std::fs::create_dir_all(&workdir).unwrap();
        let node = std::env::var("DOCU_COACH_NODE").unwrap_or_else(|_| "node".into());
        let acp = super::Acp::start(node.as_ref(), &[script.to_string_lossy().to_string()], &workdir).unwrap();
        let mut reply = String::new();
        acp.ask(prompt::SYSTEM, &prompt::build(&body(mode, current, history)), &[], &super::RunConfig { claude: true, model: "claude-opus-5-5".into(), ..Default::default() }, |t| reply.push_str(t)).unwrap();
        println!("----- {mode} / {current}\n{reply}\n");
        reply
    }
    fn user(s: &str) -> Value { json!({"role": "user", "content": s}) }
    fn squash(s: &str) -> String { s.to_lowercase().replace(['“', '”', '"', '\'', '’'], "").split_whitespace().collect::<Vec<_>>().join(" ") }
    fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> { tag.split(&format!("{name}=\"")).nth(1).and_then(|r| r.split('"').next()) }
    fn tags<'a>(reply: &'a str, name: &str) -> Vec<&'a str> { reply.split(&format!("<{name} ")).skip(1).collect() }
    fn grounded(reply: &str, name: &str) {
        let hay = squash(&format!("{CONTEXT}\n{DESIGN}\n{ALTS}"));
        for t in tags(reply, name) { if let Some(q) = attr(t, "quote") { assert!(hay.contains(&squash(q)), "{name} quote not in document: {q}"); } }
    }

    #[test] #[ignore]
    fn panel_scores_and_verdict() {
        let who = "Staff peer (attacks technical soundness); On-call SRE (attacks failure modes and rollback); Implementing engineer (attacks ambiguity)";
        let reply = turn("review", "Proposed design", vec![user(&format!("Review the whole document as a panel of three independent reviewers: {who}. Give every note a persona attribute. Then score the rubric and give a verdict. Quote the exact words for every note."))]);
        grounded(&reply, "note");
        let dims = ["bottom line", "evidence", "options", "risk honesty", "audience fit", "structure", "economy", "actionability"];
        let scores = tags(&reply, "score");
        for d in dims { assert!(scores.iter().any(|s| attr(s, "dimension").map(|x| x.trim().to_lowercase()) == Some(d.into())), "missing score for {d}"); }
        for s in &scores { let v: u8 = attr(s, "value").unwrap().parse().unwrap(); assert!((1..=5).contains(&v)); }
        assert!(reply.contains("<verdict>SHIP</verdict>") || reply.contains("<verdict>REVISE</verdict>"), "no verdict");
        assert!(tags(&reply, "note").iter().filter(|n| attr(n, "persona").is_some()).count() >= 2, "notes should name their reviewer");
    }

    #[test] #[ignore]
    fn claims_have_sources() {
        let reply = turn("review", "Context and problem", vec![user("List the claims in the \"Context and problem\" section with their sources.")]);
        let claims = tags(&reply, "claim");
        assert!(claims.len() >= 2, "expected claims, got {}", claims.len());
        grounded(&reply, "claim");
        for c in &claims { let s = attr(c, "source").unwrap_or(""); assert!(s == "brief" || s == "document" || s == "unsupported" || s.starts_with("attachment:"), "bad source: {s}"); }
        assert!(claims.iter().any(|c| attr(c, "source") == Some("unsupported")), "\"a few clients\" and \"three incidents\" should be unsupported");
    }

    #[test] #[ignore]
    fn thirty_second_read_names_the_ask() {
        let reply = turn("review", "Context and problem", vec![user("Do the 30-second read of this document: title, first paragraph and headings only. What do you believe it asks for, and does that match the brief?")]);
        let low = reply.to_lowercase();
        assert!(low.contains("redis") || low.contains("rate limit"), "should say what the document asks for");
        assert!(low.contains("brief") || low.contains("match"), "should compare with the brief");
    }

    #[test] #[ignore]
    fn draw_returns_a_diagram_spec() {
        let reply = turn("review", "Proposed design", vec![user(&format!("Draw this as a diagram:\n\n\"{DESIGN}\""))]);
        let spec = reply.split("<diagram").nth(1).and_then(|r| r.splitn(2, '>').nth(1)).and_then(|r| r.split("</diagram>").next()).expect("no <diagram> tag");
        let v: Value = serde_json::from_str(spec.trim()).expect("diagram JSON");
        let nodes = v["nodes"].as_array().expect("nodes");
        assert!((2..=12).contains(&nodes.len()), "{} nodes", nodes.len());
        let ids: Vec<String> = nodes.iter().map(|n| n["id"].as_str().unwrap_or("").to_string()).collect();
        for e in v["edges"].as_array().expect("edges") { for k in ["from", "to"] { assert!(ids.contains(&e[k].as_str().unwrap_or("").to_string()), "edge refers to unknown node"); } }
    }

    #[test] #[ignore]
    fn rehearse_asks_then_grades() {
        let start = "Rehearse the review meeting. You are the panel: Skeptical VP (attacks the business case); Staff peer (attacks alternatives). Ask your first question, the hardest one.";
        let q = turn("rehearse", "Proposed design", vec![user(start)]);
        assert!(q.contains('?'), "first turn should be a question");
        assert!(!q.contains("<grade"), "nothing to grade yet");
        let a = "We lose about 6% of month-end orders, roughly $40k a month, from clients hitting timeouts. The number is from the checkout dashboard.";
        let g = turn("rehearse", "Proposed design", vec![user(start), json!({"role": "assistant", "content": q}), user(a)]);
        let grade = tags(&g, "grade").first().map(|t| attr(t, "value").unwrap_or("").to_string()).expect("no <grade>");
        let v: u8 = grade.parse().expect("grade value");
        assert!((1..=5).contains(&v));
        assert!(g.contains('?'), "should ask the next question");
    }

    #[test] #[ignore]
    fn reader_read_decides() {
        let reply = turn("review", "Context and problem", vec![user("Read this document as my reader: the reader in the brief, with their job, their time and their objection. Tell me what happens in their head, where it happens, and what they decide.")]);
        let notes: Vec<&str> = tags(&reply, "note").into_iter().filter(|n| attr(n, "type") == Some("reader")).collect();
        assert!(notes.len() >= 3, "expected reader notes, got {}", notes.len());
        grounded(&reply, "note");
        let d = tags(&reply, "reader").first().and_then(|t| attr(t, "decision")).expect("no <reader decision>");
        assert!(["yes", "ask", "no"].contains(&d), "bad decision: {d}");
    }

    #[test] #[ignore]
    fn thesis_is_written_or_blocked() {
        let reply = turn("interview", "Context and problem", vec![user("Give me the thesis for this document: the bottom line in at most 25 words, the ask, three supporting claims with facts, the strongest counterargument, and what would change your mind. If you cannot without inventing facts, say THESIS BLOCKED and ask me the questions.")]);
        assert!(reply.contains("THESIS BLOCKED") || reply.contains("<draft section=\""), "thesis should end in a draft, or be blocked");
    }
}
