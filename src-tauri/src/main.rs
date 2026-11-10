#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod acp;
mod local;
mod prompt;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{ipc::Channel, AppHandle, Manager};

/// The running agent, started on the first coach request, restarted if it dies or Settings change.
#[derive(Default)]
struct Coach(Mutex<Option<(Arc<acp::Acp>, String)>>);

/// Apps opened from Finder do not get your shell PATH (nvm, Homebrew), so ask the login shell.
/// Resolve binaries the way the user's terminal would: through the login shell's PATH.
fn which_all(names: &[&str]) -> std::collections::HashMap<String, PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let script = names.iter().map(|n| format!("printf '%s=%s\\n' '{n}' \"$(command -v '{n}' 2>/dev/null)\"")).collect::<Vec<_>>().join("; ");
    let mut found = std::collections::HashMap::new();
    if let Ok(out) = Command::new(shell).args(["-ilc", &script]).stdin(Stdio::null()).stderr(Stdio::null()).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((n, p)) = line.split_once('=') {
                if p.starts_with('/') { found.insert(n.to_string(), PathBuf::from(p.trim())); }
            }
        }
    }
    found
}
/// Places a binary lives when the login shell did not say: checked without spawning anything.
fn well_known(name: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    [format!("/opt/homebrew/bin/{name}"), format!("/usr/local/bin/{name}"), format!("{home}/.local/bin/{name}"), format!("{home}/.npm-global/bin/{name}"), format!("{home}/.cargo/bin/{name}")]
        .into_iter().map(PathBuf::from).find(|p| p.exists())
}
fn which(name: &str) -> Option<PathBuf> {
    if let Some(p) = discovered().get(name) { return Some(p.clone()); }
    which_all(&[name]).remove(name).or_else(|| well_known(name))
}
/// One login-shell lookup for every binary we care about, done once per app run (a login shell costs about a second).
fn discovered() -> std::collections::HashMap<String, PathBuf> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<std::collections::HashMap<String, PathBuf>>>> = std::sync::OnceLock::new();
    let cell = CACHE.get_or_init(|| std::sync::Mutex::new(None));
    let mut slot = cell.lock().unwrap();
    if DISCOVERY_RESET.swap(false, std::sync::atomic::Ordering::SeqCst) { *slot = None; }
    if let Some(m) = slot.as_ref() { return m.clone(); }
    let names: Vec<&str> = KNOWN_AGENTS.iter().map(|a| a.2).chain(std::iter::once("node")).collect();
    let mut found = which_all(&names);
    for n in names { if !found.contains_key(n) { if let Some(p) = well_known(n) { found.insert(n.to_string(), p); } } }
    *slot = Some(found.clone());
    found
}
fn forget_discovered() { DISCOVERY_RESET.store(true, std::sync::atomic::Ordering::SeqCst); }
static DISCOVERY_RESET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn find_node() -> PathBuf {
    if let Ok(p) = std::env::var("DOCU_COACH_NODE") { return p.into(); }
    which("node").unwrap_or_else(|| "node".into())
}

/// ACP agents the app can find by itself: id, name, binary on PATH, arguments that start ACP over stdio.
const KNOWN_AGENTS: &[(&str, &str, &str, &[&str])] = &[
    ("devin", "Devin", "devin", &["acp"]),
    ("claude-code-acp", "Claude Code (your own install)", "claude-code-acp", &[]),
    ("codex", "Codex", "codex-acp", &[]),
    ("gemini", "Gemini CLI", "gemini", &["--acp"]),
    ("goose", "Goose", "goose", &["acp"]),
];
/// Models the selected (or a given) agent offers, asked over ACP.
#[tauri::command]
async fn agent_models(app: AppHandle, agent_id: Option<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // The list for the agent the page is looking at, without changing what is saved.
        let mut s = settings(&app);
        if let Some(id) = agent_id.filter(|id| !id.is_empty()) { s["agent"] = serde_json::json!(id); }
        if s["agent"].as_str() == Some("local") {
            let base = s["localUrl"].as_str().unwrap_or("http://localhost:11434");
            let list = local::models(base)?;
            let current = list.iter().find(|m| !m.contains("cloud")).cloned().unwrap_or_default();
            return Ok(serde_json::json!({ "models": list.iter().map(|m| serde_json::json!({"id": m, "name": m})).collect::<Vec<_>>(), "current": current }));
        }
        let (models, current) = agent_for(&app, &s)?.models()?;
        Ok(serde_json::json!({ "models": models.iter().map(|(id, name)| serde_json::json!({"id": id, "name": name})).collect::<Vec<_>>(), "current": current }))
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn agents_discover(refresh: Option<bool>) -> Vec<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(move || {
        if refresh.unwrap_or(false) { forget_discovered(); }
        agents_list()
    }).await.unwrap_or_default()
}
fn agents_list() -> Vec<serde_json::Value> {
    let found = discovered();
    let mut list = vec![serde_json::json!({ "id": "claude", "name": "Claude Code (bundled)", "path": "bundled with Margin; uses your Claude login", "found": true })];
    for (id, name, bin, args) in KNOWN_AGENTS {
        let path = found.get(*bin).cloned();
        list.push(serde_json::json!({ "id": id, "name": name, "found": path.is_some(),
            "path": path.map(|p| format!("{} {}", p.display(), args.join(" ")).trim().to_string()).unwrap_or_else(|| format!("`{bin}` is not on your PATH")) }));
    }
    let local_up = local::models("http://localhost:11434").map(|m| m.len()).ok();
    list.push(serde_json::json!({ "id": "local", "name": "Local model (Ollama, LM Studio, llama.cpp…)", "found": true,
        "path": match local_up { Some(n) => format!("Ollama is running at localhost:11434 with {n} model{}; no tokens used", if n == 1 { "" } else { "s" }), None => "nothing answering at localhost:11434 yet; start Ollama or set the server URL below".to_string() } }));
    list.push(serde_json::json!({ "id": "custom", "name": "Custom ACP agent command", "path": "", "found": true }));
    list
}

/* ---------- settings ---------- */
fn settings(app: &AppHandle) -> serde_json::Value {
    let cfg = read_config(app);
    let mut s = cfg["settings"].clone();
    if !s.is_object() { s = serde_json::json!({}); }
    let defaults = serde_json::json!({
        "agent": "claude", "command": "", "args": "", "localUrl": "http://localhost:11434",
        "model": "claude-opus-5-5", "effort": "medium", "skills": [], "useMySettings": false
    });
    for (k, v) in defaults.as_object().unwrap() { if s.get(k).is_none() { s[k] = v.clone(); } }
    s
}
#[tauri::command]
fn settings_get(app: AppHandle) -> Result<serde_json::Value, String> { Ok(settings(&app)) }
#[tauri::command]
fn settings_set(app: AppHandle, value: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut cfg = read_config(&app);
    let mut cur = settings(&app);
    if let Some(obj) = value.as_object() { for (k, v) in obj { cur[k] = v.clone(); } }
    cfg["settings"] = cur;
    write_config(&app, &cfg)?;
    Ok(settings(&app))
}
fn run_config(app: &AppHandle) -> acp::RunConfig {
    let s = settings(app);
    acp::RunConfig {
        claude: matches!(s["agent"].as_str(), Some("claude") | Some("claude-code-acp") | None),
        model: s["model"].as_str().unwrap_or("").trim().to_string(),
        effort: s["effort"].as_str().unwrap_or("").trim().to_string(),
        skills: s["skills"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default(),
        use_my_settings: s["useMySettings"].as_bool().unwrap_or(false),
    }
}
/// The agent command: the bundled Claude Code adapter, a discovered agent, or the custom command from Settings.
fn agent_command(app: &AppHandle) -> Result<(PathBuf, Vec<String>), String> { agent_command_for(app, &settings(app)) }
fn agent_command_for(app: &AppHandle, s: &serde_json::Value) -> Result<(PathBuf, Vec<String>), String> {
    let id = s["agent"].as_str().unwrap_or("claude");
    if let Some((_, name, bin, args)) = KNOWN_AGENTS.iter().find(|a| a.0 == id) {
        let path = which(bin).ok_or_else(|| format!("{name} was not found: `{bin}` is not on your PATH. Install it, or choose another coach in Settings."))?;
        return Ok((path, args.iter().map(|a| a.to_string()).collect()));
    }
    if id == "custom" {
        let cmd = s["command"].as_str().unwrap_or("").trim().to_string();
        if cmd.is_empty() { return Err("Settings: the custom agent has no command.".into()); }
        let args: Vec<String> = s["args"].as_str().unwrap_or("").split_whitespace().map(String::from).collect();
        return Ok((PathBuf::from(cmd), args));
    }
    let script = app.path().resource_dir().map_err(|e| e.to_string())?
        .join("node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js");
    Ok((find_node(), vec![script.to_string_lossy().to_string()]))
}

fn agent(app: &AppHandle) -> Result<Arc<acp::Acp>, String> { agent_for(app, &settings(app)) }
fn agent_for(app: &AppHandle, s: &serde_json::Value) -> Result<Arc<acp::Acp>, String> {
    let state = app.state::<Coach>();
    let mut slot = state.0.lock().unwrap();
    let (program, args) = agent_command_for(app, s)?;
    let key = format!("{}|{}", program.display(), args.join(" "));
    if let Some((a, _)) = slot.as_ref().filter(|(a, k)| a.alive() && *k == key) {
        return Ok(a.clone());
    }
    *slot = None; // drops (and kills) a previous agent started with a different command
    // Empty working folder: the coach never sees your files.
    let workdir = std::env::temp_dir().join("margin-coach");
    std::fs::create_dir_all(&workdir).map_err(|e| e.to_string())?;
    let a = Arc::new(acp::Acp::start(&program, &args, &workdir)?);
    *slot = Some((a.clone(), key));
    Ok(a)
}

#[tauri::command]
async fn ask(app: AppHandle, body: serde_json::Value, on_chunk: Channel<String>) -> Result<(), String> {
    let prompt = prompt::build(&body);
    let images = prompt::images(&body);
    tauri::async_runtime::spawn_blocking(move || {
        let run = run_config(&app);
        let s = settings(&app);
        let stop = if s["agent"].as_str() == Some("local") {
            let base = s["localUrl"].as_str().unwrap_or("http://localhost:11434");
            let model = s["model"].as_str().unwrap_or("").trim().to_string();
            let available = local::models(base)?;
            let model = if available.iter().any(|m| *m == model) { model } else { available.into_iter().find(|m| !m.contains("cloud")).ok_or("No model is loaded on the local server. Pull one (for example `ollama pull gemma4:26b`) and pick it in Settings.")? };
            // Local models get the instructions in the prompt, like any non-Claude coach, and a system message too.
            local::ask(base, &model, prompt::SYSTEM, &prompt, &images, |t| { let _ = on_chunk.send(t.to_string()); })
        } else {
            agent(&app)?.ask(prompt::SYSTEM, &prompt, &images, &run, |t| { let _ = on_chunk.send(t.to_string()); })
        };
        let note = match stop {
            Ok(Some(r)) if r == "refusal" => "\n\nThe coach could not answer this one. Try asking in a different way.",
            Ok(Some(r)) if r == "max_tokens" => "\n\n(The answer was cut off because it was too long. Ask the coach to continue.)",
            Ok(_) => "",
            Err(e) if e.to_lowercase().contains("auth") => {
                return Err(format!("The coach is not logged in ({e}). Run `claude` in a terminal, log in with /login, then try again."))
            }
            Err(e) => return Err(format!("The coach had a problem: {e}")),
        };
        if !note.is_empty() {
            let _ = on_chunk.send(note.into());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Text of a PDF the writer attached as context (base64 bytes in, plain text out).
#[tauri::command]
async fn extract_pdf(data: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
        let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| format!("Could not read this PDF: {e}"))?;
        let text = text.trim();
        if text.is_empty() { return Err("This PDF has no text layer (it may be scanned images).".into()); }
        Ok(text.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ---------- workspaces: a folder with documents/, diagrams/ and files/ ---------- */
// Config in the app's config dir: { "workspaces": [paths], "current": path }.
fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("config.json");
    // First run after the rename from Docu Coach: carry the old config over.
    if !path.exists() {
        if let Some(old) = dir.parent().map(|p| p.join("com.docucoach.desktop").join("config.json")) {
            if old.exists() { let _ = std::fs::copy(&old, &path); }
        }
    }
    Ok(path)
}
fn read_config(app: &AppHandle) -> serde_json::Value {
    std::fs::read_to_string(config_path(app).unwrap_or_default()).ok()
        .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}))
}
fn write_config(app: &AppHandle, v: &serde_json::Value) -> Result<(), String> {
    std::fs::write(config_path(app)?, v.to_string()).map_err(|e| e.to_string())
}
/// Makes sure a folder is a workspace: subfolders plus workspace.json. Old flat layouts
/// (document folders directly inside) are moved into documents/.
fn init_workspace(dir: &Path) -> Result<(), String> {
    for sub in ["documents", "diagrams", "files"] {
        std::fs::create_dir_all(dir.join(sub)).map_err(|e| format!("Cannot create {}: {e}", dir.join(sub).display()))?;
    }
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("document.json").exists() {
            let _ = std::fs::rename(&p, dir.join("documents").join(entry.file_name()));
        }
    }
    let meta = dir.join("workspace.json");
    if !meta.exists() {
        let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Workspace".into());
        std::fs::write(&meta, serde_json::json!({ "name": name, "created": now_ms() }).to_string()).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }
fn workspace(app: &AppHandle) -> Result<PathBuf, String> {
    let cfg = read_config(app);
    let dir = match cfg["current"].as_str() {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let docs = app.path().document_dir().map_err(|e| e.to_string())?;
            // Keep using an existing Docu Coach folder; new installs get ~/Documents/Margin.
            let old = docs.join("Docu Coach");
            if old.exists() { old } else { docs.join("Margin") }
        }
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create the workspace folder {}: {e}", dir.display()))?;
    init_workspace(&dir)?;
    Ok(dir)
}
fn ws_name(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("workspace.json")).ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["name"].as_str().map(String::from))
        .unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
}
fn slug(name: &str) -> String {
    let s: String = name.chars().flat_map(|c| if c.is_alphanumeric() { c.to_lowercase().collect::<Vec<_>>() } else { vec!['-'] }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "document".into() } else { s.chars().take(60).collect() }
}
fn safe_name(name: &str) -> Result<String, String> {
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') || name.contains("..") { return Err("bad name".into()); }
    Ok(name.to_string())
}
fn doc_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || id.contains(['/', '\\', '.']) { return Err("bad document id".into()); }
    Ok(workspace(app)?.join("documents").join(id))
}

/// The current workspace and all known ones.
#[tauri::command]
fn ws_info(app: AppHandle) -> Result<serde_json::Value, String> {
    let cur = workspace(&app)?;
    let mut cfg = read_config(&app);
    let mut list: Vec<String> = cfg["workspaces"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let cur_s = cur.display().to_string();
    if !list.contains(&cur_s) { list.insert(0, cur_s.clone()); cfg["workspaces"] = serde_json::json!(list); cfg["current"] = serde_json::json!(cur_s); write_config(&app, &cfg)?; }
    let known: Vec<serde_json::Value> = list.iter().map(|p| serde_json::json!({ "path": p, "name": ws_name(Path::new(p)), "exists": Path::new(p).exists() })).collect();
    Ok(serde_json::json!({ "path": cur_s, "name": ws_name(&cur), "workspaces": known }))
}

/// Adds (creating if needed) a workspace folder and switches to it.
#[tauri::command]
fn ws_use(app: AppHandle, path: String, name: Option<String>) -> Result<serde_json::Value, String> {
    let p = PathBuf::from(path.trim());
    if !p.is_absolute() { return Err("Give a full path, for example /Users/you/Documents/Writing".into()); }
    std::fs::create_dir_all(&p).map_err(|e| format!("Cannot use that folder: {e}"))?;
    init_workspace(&p)?;
    if let Some(n) = name.filter(|n| !n.trim().is_empty()) {
        std::fs::write(p.join("workspace.json"), serde_json::json!({ "name": n.trim(), "created": now_ms() }).to_string()).map_err(|e| e.to_string())?;
    }
    let mut cfg = read_config(&app);
    let mut list: Vec<String> = cfg["workspaces"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
    let ps = p.display().to_string();
    if !list.contains(&ps) { list.push(ps.clone()); }
    cfg["workspaces"] = serde_json::json!(list); cfg["current"] = serde_json::json!(ps);
    write_config(&app, &cfg)?;
    ws_info(app)
}

#[tauri::command]
fn ws_forget(app: AppHandle, path: String) -> Result<serde_json::Value, String> {
    let mut cfg = read_config(&app);
    let list: Vec<String> = cfg["workspaces"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).filter(|p| p != &path).collect()).unwrap_or_default();
    if cfg["current"].as_str() == Some(path.as_str()) { cfg["current"] = serde_json::json!(list.first().cloned().unwrap_or_default()); }
    cfg["workspaces"] = serde_json::json!(list);
    write_config(&app, &cfg)?;
    ws_info(app)
}

/// Opens the workspace, a document folder, or a file inside the workspace in Finder.
#[tauri::command]
fn reveal(app: AppHandle, id: Option<String>, sub: Option<String>) -> Result<(), String> {
    let p = match (id, sub) {
        (Some(id), _) => doc_dir(&app, &id)?,
        (None, Some(sub)) => workspace(&app)?.join(safe_name(&sub)?),
        _ => workspace(&app)?,
    };
    Command::new("open").arg(p).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Every document in the workspace, newest first: the `meta` the app saved plus the id.
#[tauri::command]
fn docs_list(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let mut out = vec![];
    for entry in std::fs::read_dir(workspace(&app)?.join("documents")).map_err(|e| e.to_string())?.flatten() {
        let file = entry.path().join("document.json");
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let mut meta = v["meta"].clone();
        if !meta.is_object() { meta = serde_json::json!({}); }
        meta["id"] = serde_json::Value::String(entry.file_name().to_string_lossy().into());
        meta["title"] = v["title"].clone();
        meta["templateId"] = v["templateId"].clone();
        if let Ok(m) = std::fs::metadata(&file).and_then(|m| m.modified()) {
            meta["updated"] = serde_json::json!(m.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0));
        }
        out.push(meta);
    }
    out.sort_by_key(|m| std::cmp::Reverse(m["updated"].as_u64().unwrap_or(0)));
    Ok(out)
}

#[tauri::command]
fn doc_create(app: AppHandle, title: String) -> Result<String, String> {
    let base = slug(&title);
    let mut id = base.clone();
    let mut n = 2;
    while doc_dir(&app, &id)?.exists() { id = format!("{base}-{n}"); n += 1; }
    std::fs::create_dir_all(doc_dir(&app, &id)?.join("assets")).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
fn doc_load(app: AppHandle, id: String) -> Result<String, String> {
    std::fs::read_to_string(doc_dir(&app, &id)?.join("document.json")).map_err(|e| format!("Cannot open this document: {e}"))
}

/// Writes document.json and a Markdown copy next to it.
#[tauri::command]
fn doc_save(app: AppHandle, id: String, json: String, markdown: String) -> Result<(), String> {
    let dir = doc_dir(&app, &id)?;
    std::fs::create_dir_all(dir.join("assets")).map_err(|e| e.to_string())?;
    let tmp = dir.join("document.json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join("document.json")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("document.md"), markdown).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn doc_delete(app: AppHandle, id: String) -> Result<(), String> {
    let dir = doc_dir(&app, &id)?;
    if dir.join("document.json").exists() { std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?; }
    Ok(())
}

/// Saves an image (base64) into the document's assets folder; returns the file name.
#[tauri::command]
fn asset_save(app: AppHandle, id: String, name: String, data: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '-' }).collect();
    let dir = doc_dir(&app, &id)?.join("assets");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(&safe), bytes).map_err(|e| e.to_string())?;
    Ok(safe)
}

/* ---------- workspace diagrams and files ---------- */
fn list_dir(dir: &Path) -> Vec<serde_json::Value> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_file() || e.file_name().to_string_lossy().starts_with('.') { continue; }
            let m = std::fs::metadata(&p).ok();
            out.push(serde_json::json!({
                "name": e.file_name().to_string_lossy(),
                "size": m.as_ref().map(|m| m.len()).unwrap_or(0),
                "updated": m.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0),
            }));
        }
    }
    out.sort_by_key(|v| std::cmp::Reverse(v["updated"].as_u64().unwrap_or(0)));
    out
}
/// Diagrams: `<name>.svg` to show and `<name>.excalidraw` to edit again.
#[tauri::command]
fn diagrams_list(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let dir = workspace(&app)?.join("diagrams");
    Ok(list_dir(&dir).into_iter().filter(|v| v["name"].as_str().map_or(false, |n| n.ends_with(".svg"))).map(|mut v| {
        let n = v["name"].as_str().unwrap_or("").trim_end_matches(".svg").to_string();
        v["svg"] = serde_json::json!(std::fs::read_to_string(dir.join(format!("{n}.svg"))).unwrap_or_default());
        v["name"] = serde_json::json!(n);
        v
    }).collect())
}
#[tauri::command]
fn diagram_save(app: AppHandle, name: String, svg: String, scene: String) -> Result<String, String> {
    let dir = workspace(&app)?.join("diagrams");
    let n = slug(&name);
    std::fs::write(dir.join(format!("{n}.svg")), svg).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{n}.excalidraw")), scene).map_err(|e| e.to_string())?;
    Ok(n)
}
#[tauri::command]
fn diagram_load(app: AppHandle, name: String) -> Result<serde_json::Value, String> {
    let dir = workspace(&app)?.join("diagrams");
    let n = safe_name(&name)?;
    Ok(serde_json::json!({
        "svg": std::fs::read_to_string(dir.join(format!("{n}.svg"))).map_err(|e| e.to_string())?,
        "scene": std::fs::read_to_string(dir.join(format!("{n}.excalidraw"))).unwrap_or_default(),
    }))
}
#[tauri::command]
fn diagram_delete(app: AppHandle, name: String) -> Result<(), String> {
    let dir = workspace(&app)?.join("diagrams");
    let n = safe_name(&name)?;
    let _ = std::fs::remove_file(dir.join(format!("{n}.svg")));
    let _ = std::fs::remove_file(dir.join(format!("{n}.excalidraw")));
    Ok(())
}
/// The Excalidraw shape library for this workspace (diagrams/library.json).
#[tauri::command]
fn library_load(app: AppHandle) -> Result<String, String> {
    Ok(std::fs::read_to_string(workspace(&app)?.join("diagrams").join("library.json")).unwrap_or_else(|_| "[]".into()))
}
#[tauri::command]
fn library_save(app: AppHandle, json: String) -> Result<(), String> {
    std::fs::write(workspace(&app)?.join("diagrams").join("library.json"), json).map_err(|e| e.to_string())
}

/// House style rules for the workspace (style.md), applied in every review.
/// Custom document types (templates.json in the workspace), shared with the team through the folder.
#[tauri::command]
fn templates_get(app: AppHandle) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(workspace(&app)?.join("templates.json")).unwrap_or_else(|_| "{}".into());
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({})))
}
#[tauri::command]
fn templates_set(app: AppHandle, value: serde_json::Value) -> Result<(), String> {
    std::fs::write(workspace(&app)?.join("templates.json"), serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
#[tauri::command]
fn style_get(app: AppHandle) -> Result<String, String> {
    Ok(std::fs::read_to_string(workspace(&app)?.join("style.md")).unwrap_or_default())
}
#[tauri::command]
fn style_set(app: AppHandle, text: String) -> Result<(), String> {
    std::fs::write(workspace(&app)?.join("style.md"), text).map_err(|e| e.to_string())
}

/// Real reviewer feedback for the workspace (feedback.md): calibrates the panels.
#[tauri::command]
fn feedback_get(app: AppHandle) -> Result<String, String> {
    Ok(std::fs::read_to_string(workspace(&app)?.join("feedback.md")).unwrap_or_default())
}
#[tauri::command]
fn feedback_set(app: AppHandle, text: String) -> Result<(), String> {
    std::fs::write(workspace(&app)?.join("feedback.md"), text).map_err(|e| e.to_string())
}

/// Document history: one Markdown snapshot per review round, under <doc>/history/.
#[tauri::command]
fn doc_snapshot(app: AppHandle, id: String, markdown: String, label: String) -> Result<String, String> {
    let dir = doc_dir(&app, &id)?.join("history");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = label.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' { c } else { '-' }).collect();
    let name = format!("{}-{}.md", now_ms(), safe.chars().take(40).collect::<String>());
    std::fs::write(dir.join(&name), markdown).map_err(|e| e.to_string())?;
    Ok(name)
}
#[tauri::command]
fn history_list(app: AppHandle, id: String) -> Result<Vec<serde_json::Value>, String> {
    Ok(list_dir(&doc_dir(&app, &id)?.join("history")))
}
#[tauri::command]
fn history_read(app: AppHandle, id: String, name: String) -> Result<String, String> {
    std::fs::read_to_string(doc_dir(&app, &id)?.join("history").join(safe_name(&name)?)).map_err(|e| e.to_string())
}

/// Files picked with the native dialog, read for the page (name, size, base64). 25 MB cap per file.
#[tauri::command]
fn read_files(paths: Vec<String>) -> Vec<serde_json::Value> {
    use base64::Engine;
    paths.iter().map(|p| {
        let path = PathBuf::from(p);
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match std::fs::metadata(&path).and_then(|m| if m.len() > 25 * 1024 * 1024 { Err(std::io::Error::other("over 25 MB")) } else { std::fs::read(&path) }) {
            Ok(bytes) => serde_json::json!({ "name": name, "size": bytes.len(), "data": base64::engine::general_purpose::STANDARD.encode(bytes) }),
            Err(e) => serde_json::json!({ "name": name, "error": e.to_string() }),
        }
    }).collect()
}

/// Shared files: context any document can attach.
#[tauri::command]
fn files_list(app: AppHandle) -> Result<Vec<serde_json::Value>, String> { Ok(list_dir(&workspace(&app)?.join("files"))) }
#[tauri::command]
fn file_add(app: AppHandle, name: String, data: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '-' }).collect();
    std::fs::write(workspace(&app)?.join("files").join(&safe), bytes).map_err(|e| e.to_string())?;
    Ok(safe)
}
#[tauri::command]
fn file_read(app: AppHandle, name: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(workspace(&app)?.join("files").join(safe_name(&name)?)).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
#[tauri::command]
fn file_delete(app: AppHandle, name: String) -> Result<(), String> {
    std::fs::remove_file(workspace(&app)?.join("files").join(safe_name(&name)?)).map_err(|e| e.to_string())
}

/* ---------- publish to Confluence ---------- */
/// Creates a child page under `parent_url` (a page link) with the document's HTML, or updates `page_id` if given.
#[tauri::command]
async fn confluence_publish(parent_url: String, title: String, html: String, email: String, token: String, page_id: Option<String>) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let u = reqwest::Url::parse(parent_url.trim()).map_err(|_| "That is not a valid page link.".to_string())?;
        let segs: Vec<&str> = u.path_segments().map(|s| s.collect()).unwrap_or_default();
        let parent = segs.iter().position(|s| *s == "pages").and_then(|i| segs.get(i + 1)).map(|s| s.to_string())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .ok_or("Cannot find a page id in that link. Use the parent page URL from your browser.")?;
        let base = format!("{}://{}", u.scheme(), u.host_str().unwrap_or_default());
        let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(40)).build().map_err(|e| e.to_string())?;
        let auth = |r: reqwest::blocking::RequestBuilder| r.basic_auth(email.trim(), Some(token.trim())).header("Accept", "application/json");
        let fail = |status: reqwest::StatusCode, what: &str| -> String {
            match status.as_u16() {
                401 | 403 => "Confluence refused the login. Check the email and the API token.".into(),
                404 => format!("Confluence did not find the {what} with this account."),
                s => format!("Confluence answered {s} while {what}."),
            }
        };
        // Parent page gives the space.
        let res = auth(client.get(format!("{base}/wiki/api/v2/pages/{parent}"))).send().map_err(|e| format!("Could not reach Confluence: {e}"))?;
        if !res.status().is_success() { return Err(fail(res.status(), "parent page")); }
        let pv: serde_json::Value = res.json().map_err(|e| e.to_string())?;
        let space = pv["spaceId"].as_str().unwrap_or("").to_string();
        let body = serde_json::json!({ "representation": "storage", "value": html });
        let (res, verb) = match page_id.filter(|p| !p.is_empty()) {
            Some(pid) => {
                let cur = auth(client.get(format!("{base}/wiki/api/v2/pages/{pid}"))).send().map_err(|e| e.to_string())?;
                if !cur.status().is_success() { return Err(fail(cur.status(), "page to update")); }
                let cv: serde_json::Value = cur.json().map_err(|e| e.to_string())?;
                let next = cv["version"]["number"].as_i64().unwrap_or(1) + 1;
                (auth(client.put(format!("{base}/wiki/api/v2/pages/{pid}"))).json(&serde_json::json!({ "id": pid, "status": "current", "title": title, "body": body, "version": { "number": next, "message": "Updated from Margin" } })).send(), "updating the page")
            }
            None => (auth(client.post(format!("{base}/wiki/api/v2/pages"))).json(&serde_json::json!({ "spaceId": space, "status": "current", "title": title, "parentId": parent, "body": body })).send(), "creating the page"),
        };
        let res = res.map_err(|e| format!("Could not reach Confluence: {e}"))?;
        if !res.status().is_success() { return Err(fail(res.status(), verb)); }
        let v: serde_json::Value = res.json().map_err(|e| e.to_string())?;
        let id = v["id"].as_str().unwrap_or("").to_string();
        let link = v["_links"]["webui"].as_str().map(|w| format!("{base}/wiki{w}")).unwrap_or_default();
        Ok(serde_json::json!({ "id": id, "url": link }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ---------- import from Confluence ---------- */
/// Fetches a Confluence Cloud page as export HTML. `url` is the page link; `email` + `token` an Atlassian API token.
#[tauri::command]
async fn confluence_fetch(url: String, email: String, token: String) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let u = reqwest::Url::parse(url.trim()).map_err(|_| "That is not a valid page link.".to_string())?;
        let segs: Vec<&str> = u.path_segments().map(|s| s.collect()).unwrap_or_default();
        let from_path = segs.iter().position(|s| *s == "pages").and_then(|i| segs.get(i + 1)).map(|s| s.to_string());
        let from_query = u.query_pairs().find(|(k, _)| k == "pageId").map(|(_, v)| v.into_owned());
        let id = from_path.or(from_query).filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .ok_or("Cannot find a page id in that link. Use the page URL from your browser (it contains /pages/<id>/).")?;
        let base = format!("{}://{}", u.scheme(), u.host_str().unwrap_or_default());
        let api = format!("{base}/wiki/api/v2/pages/{id}?body-format=export_view");
        let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())?;
        let res = client.get(&api).basic_auth(email.trim(), Some(token.trim())).header("Accept", "application/json").send().map_err(|e| format!("Could not reach Confluence: {e}"))?;
        let status = res.status();
        if status.as_u16() == 401 || status.as_u16() == 403 { return Err("Confluence refused the login. Check the email and the API token (create one at id.atlassian.com/manage-profile/security/api-tokens).".into()); }
        if status.as_u16() == 404 { return Err("Confluence did not find that page with this account.".into()); }
        if !status.is_success() { return Err(format!("Confluence answered {status}.")); }
        let v: serde_json::Value = res.json().map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "title": v["title"], "html": v["body"]["export_view"]["value"] }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Saves the document as Markdown in Downloads and returns the path.
#[tauri::command]
fn save_markdown(app: AppHandle, name: String, text: String) -> Result<String, String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let stem: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    let stem = if stem.trim_matches('-').is_empty() { "document".to_string() } else { stem };
    let mut path = dir.join(format!("{stem}.md"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.md"));
        n += 1;
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Coach::default())
        .invoke_handler(tauri::generate_handler![ask, save_markdown, extract_pdf, ws_info, ws_use, ws_forget, reveal, docs_list, doc_create, doc_load, doc_save, doc_delete, asset_save, diagrams_list, diagram_save, diagram_load, diagram_delete, files_list, file_add, file_read, file_delete, library_load, library_save, confluence_fetch, settings_get, settings_set, agents_discover, agent_models, read_files, style_get, style_set, templates_get, templates_set, feedback_get, feedback_set, doc_snapshot, history_list, history_read, confluence_publish])
        .run(tauri::generate_context!())
        .expect("error while running Margin");
}

#[cfg(test)]
mod tests {
    #[test]
    fn slugs() {
        assert_eq!(super::slug("Rate limiter for the Payments API"), "rate-limiter-for-the-payments-api");
        assert_eq!(super::slug("  ***  "), "document");
        assert_eq!(super::slug("Ünïcode: ok?"), "ünïcode-ok");
    }
}

#[cfg(test)]
mod discovery_tests {
    #[test]
    fn login_shell_lookup_finds_sh() {
        assert_eq!(super::which("sh").map(|p| p.exists()), Some(true));
        assert!(super::which("definitely-not-a-binary-xyz").is_none());
    }
}
#[cfg(test)]
mod workspace_tests {
    #[test]
    fn migrates_flat_layout() {
        let dir = std::env::temp_dir().join(format!("docu-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("old-doc/assets")).unwrap();
        std::fs::write(dir.join("old-doc/document.json"), "{}").unwrap();
        super::init_workspace(&dir).unwrap();
        assert!(dir.join("documents/old-doc/document.json").exists(), "document moved into documents/");
        assert!(!dir.join("old-doc").exists());
        assert!(dir.join("workspace.json").exists());
        assert!(dir.join("diagrams").is_dir() && dir.join("files").is_dir());
        // Running it again is harmless.
        super::init_workspace(&dir).unwrap();
        assert!(dir.join("documents/old-doc/document.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
