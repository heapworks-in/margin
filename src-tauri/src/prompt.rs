//! Builds the one self-contained prompt the coach reads: past chat, a fresh
//! document snapshot, then the new message.
use serde_json::Value;

pub const SYSTEM: &str = concat!(include_str!("coach_prompt.md"), include_str!("../../static/guide.md"));
const COACH: &str = include_str!("coach_prompt.md");
const GUIDE: &str = include_str!("../../static/guide.md");
const GUIDE_SHORT: &str = include_str!("guide-short.md");

/// The system prompt a turn actually needs: the coach's core, the rules for this mode only, the tag formats this
/// mode emits, and the guide (full for a hosted review, short otherwise). Half the tokens of the whole prompt.
pub fn system_for(mode: &str, scope: &str, local: bool) -> String {
    let cut = |from: &str, to: &str| -> String {
        let a = COACH.find(from).unwrap_or(0);
        let b = COACH[a..].find(to).map(|i| a + i).unwrap_or(COACH.len());
        COACH[a..b].to_string()
    };
    let core = cut("", "MODE interview:");
    let (rules, tags) = match mode {
        "interview" => (cut("MODE interview:", "MODE rehearse"), format!("Tag formats (use them exactly, never inside code fences):\n{}", cut("<draft section=\"Exact section title\">", "Outside tags"))),
        "rehearse" => (cut("MODE rehearse", "MODE review:"), format!("Tag formats (use them exactly, never inside code fences):\n{}{}", cut("<grade value=\"1-5\">One line", "<diagram title="), cut("<draft section=\"Exact section title\">", "Outside tags"))),
        _ => (cut("MODE review:", "Tag formats"), cut("Tag formats", "----- THE GUIDE")),
    };
    let scope_note = if scope == "section" || scope == "changed" { "\nOnly the current section's text is in <document>; the other sections exist (their titles are listed, marked omitted) but were not sent. Do not call them empty or missing, and do not quote them.\n" } else { "" };
    let guide = if mode == "review" && !local { GUIDE } else { GUIDE_SHORT };
    format!("{core}{rules}\n{tags}\n{}\n{scope_note}\n----- THE GUIDE -----\n{guide}", cut("Outside tags", "----- THE GUIDE"))
}
/// For local models: the same coach, with the guide condensed (fewer tokens to chew through per review).
pub const SYSTEM_LOCAL: &str = concat!(include_str!("coach_prompt.md"), include_str!("guide-short.md"));

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn or_empty(t: &str) -> &str {
    let t = t.trim();
    if t.is_empty() { "(empty)" } else { t }
}

fn render_context(body: &Value) -> String {
    let tpl = &body["template"];
    let none = vec![];
    let guide: Vec<String> = tpl["sections"].as_array().unwrap_or(&none).iter()
        .map(|x| format!("- {}: {} Reviewers check: {}", s(&x["title"]), s(&x["hint"]), s(&x["checks"])))
        .collect();
    let scope = s(&body["scope"]);
    let section_only = scope == "section";
    let current = s(&body["current"]);
    let included = |x: &Value| if scope == "changed" { !s(&x["text"]).trim().is_empty() } else if section_only { s(&x["title"]) == current } else { true };
    let sections: Vec<String> = body["sections"].as_array().unwrap_or(&none).iter()
        .map(|x| if !included(x) { format!("<section title=\"{}\" omitted=\"yes\"/>", s(&x["title"])) } else { format!("<section title=\"{}\">\n{}\n</section>", s(&x["title"]), or_empty(s(&x["text"]))) })
        .collect();
    let title = match s(&body["title"]) { "" => "Untitled", t => t };
    let mode = match s(&body["mode"]) { "" => "review", m => m };
    let b = &body["brief"];
    let audience = format!(
        "<brief>\nThe ask: {}\nReader: {}\nAfter reading they should: {}\nTheir likely objection: {}\nDecision deadline: {}\nKnown numbers: {}\nNon-goals: {}\n</brief>\n<style_rules>\n{}\n</style_rules>",
        or_empty(s(&b["ask"])), or_empty(s(&body["audience"])), or_empty(s(&body["purpose"])), or_empty(s(&b["objection"])),
        or_empty(s(&b["deadline"])), or_empty(s(&b["numbers"])), or_empty(s(&b["nongoals"])), or_empty(s(&body["style"]))
    );
    let earlier: Vec<String> = body["earlier"].as_array().unwrap_or(&none).iter()
        .map(|n| format!("<note id=\"{}\" type=\"{}\" section=\"{}\" quote=\"{}\">{}</note>", s(&n["id"]), s(&n["kind"]), s(&n["section"]), s(&n["quote"]), s(&n["body"])))
        .collect();
    let earlier = if earlier.is_empty() { String::new() } else { format!("\n<earlier_notes>\n{}\n</earlier_notes>", earlier.join("\n")) };
    let ws = s(&body["workspace"]).trim();
    let earlier = if ws.is_empty() { earlier } else { format!("{earlier}\n<workspace>\n{}\n</workspace>", ws.chars().take(9000).collect::<String>()) };
    format!(
        "<template name=\"{}\">\n{}\n</template>\n{}\n<document title=\"{}\">\n<raw_notes>\n{}\n</raw_notes>\n{}\n</document>{}\n<current_section>{}</current_section>\n<mode>{}</mode>",
        s(&tpl["name"]), guide.join("\n"), audience, title, or_empty(s(&body["notes"])), sections.join("\n"), earlier, s(&body["current"]), mode
    )
}

/// Text attachments the writer added as context (images travel separately, as image blocks).
fn render_attachments(body: &Value) -> String {
    let none = vec![];
    let items: Vec<String> = body["attachments"].as_array().unwrap_or(&none).iter()
        .filter(|a| s(&a["kind"]) != "image")
        .map(|a| {
            let text = s(&a["text"]);
            let cut: String = text.chars().take(60_000).collect();
            let more = if text.chars().count() > 60_000 { "\n[... truncated ...]" } else { "" };
            format!("<attachment name=\"{}\">\n{}{}\n</attachment>", s(&a["name"]), cut.trim(), more)
        })
        .collect();
    if items.is_empty() { String::new() } else { format!("<attachments>\n{}\n</attachments>\n", items.join("\n")) }
}

/// Images the writer attached: (mime type, base64 data).
pub fn images(body: &Value) -> Vec<(String, String)> {
    let none = vec![];
    body["attachments"].as_array().unwrap_or(&none).iter()
        .filter(|a| s(&a["kind"]) == "image" && !s(&a["data"]).is_empty())
        .map(|a| (s(&a["mime"]).to_string(), s(&a["data"]).to_string()))
        .collect()
}

pub fn build(body: &Value) -> String {
    let none = vec![];
    let history = body["history"].as_array().unwrap_or(&none);
    let (last, past) = history.split_last().map(|(l, p)| (s(&l["content"]), p)).unwrap_or(("", &[]));
    let convo: Vec<String> = past.iter().map(|m| {
        let tag = if s(&m["role"]) == "user" { "writer" } else { "coach" };
        format!("<{tag}>\n{}\n</{tag}>", s(&m["content"]))
    }).collect();
    let feedback = s(&body["feedback"]).trim();
    let past = if feedback.is_empty() { String::new() } else { format!("<past_feedback>\n{}\n</past_feedback>\n", feedback.chars().take(20_000).collect::<String>()) };
    format!(
        "<conversation_so_far>\n{}\n</conversation_so_far>\n{}{}{}\n\n<writer_message>\n{}\n</writer_message>",
        convo.join("\n"), past, render_attachments(body), render_context(body), last
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn system_prompt_per_mode() {
        let i = system_for("interview", "section", true);
        let r = system_for("review", "document", false);
        assert!(i.contains("MODE interview") && !i.contains("MODE review") && i.contains("<draft section=") && !i.contains("<score dimension"));
        assert!(r.contains("MODE review") && !r.contains("MODE interview") && r.contains("<score dimension") && r.contains("## 1. Decide who it is for"));
        assert!(i.contains("marked omitted") && !r.contains("marked omitted"));
        assert!(i.len() < SYSTEM.len() / 2, "interview prompt should be under half the full one: {} vs {}", i.len(), SYSTEM.len());
        let body = json!({"scope": "section", "current": "B", "sections": [{"title": "A", "text": "aaa"}, {"title": "B", "text": "bbb"}], "template": {"name": "t", "sections": []}, "history": [{"role": "user", "content": "x"}]});
        let p = build(&body);
        assert!(p.contains("<section title=\"A\" omitted=\"yes\"/>") && p.contains("bbb") && !p.contains("aaa"));
    }

    #[test]
    fn changed_scope_sends_edited_sections_and_earlier_notes() {
        let body = json!({"scope": "changed", "current": "B", "sections": [{"title": "A", "text": ""}, {"title": "B", "text": "new text"}], "template": {"name": "t", "sections": []},
            "earlier": [{"id": "n1", "kind": "fact", "section": "B", "quote": "old words", "body": "needs a number"}], "history": [{"role": "user", "content": "Review what changed."}]});
        let p = build(&body);
        assert!(p.contains("<section title=\"A\" omitted=\"yes\"/>") && p.contains("new text"));
        assert!(p.contains("<earlier_notes>") && p.contains("id=\"n1\""));
        assert!(system_for("review", "changed", false).contains("marked omitted"));
    }

    #[test]
    fn workspace_digest_is_rendered_when_present() {
        let body = json!({"workspace": "<doc title=\"ADR-12\">We chose Postgres.</doc>", "current": "A", "sections": [{"title": "A", "text": "x"}], "template": {"name": "t", "sections": []}, "history": [{"role": "user", "content": "x"}]});
        assert!(build(&body).contains("<workspace>\n<doc title=\"ADR-12\">"));
        let none = json!({"current": "A", "sections": [], "template": {"name": "t", "sections": []}, "history": [{"role": "user", "content": "x"}]});
        assert!(!build(&none).contains("<workspace>"));
    }

    #[test]
    fn builds_prompt() {
        let body = json!({
            "mode": "review", "current": "Context and problem", "title": "Rate limiter", "notes": "clients spam api",
            "template": {"name": "Design doc", "sections": [{"title": "Context and problem", "hint": "h", "checks": "c"}]},
            "sections": [{"title": "Context and problem", "text": ""}],
            "history": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "hello"},
                {"role": "user", "content": "Review the section."}
            ]
        });
        let p = build(&body);
        assert!(p.starts_with("<conversation_so_far>\n<writer>\nhi\n</writer>\n<coach>\nhello\n</coach>\n</conversation_so_far>"));
        assert!(p.contains("<mode>review</mode>"));
        assert!(p.contains("<brief>\nThe ask: (empty)\nReader: (empty)"));
        assert!(p.contains("<style_rules>\n(empty)\n</style_rules>"));
        assert!(p.contains("<current_section>Context and problem</current_section>"));
        assert!(p.contains("clients spam api") && p.contains("(empty)"));
        assert!(p.ends_with("<writer_message>\nReview the section.\n</writer_message>"));
        assert_eq!(p.matches("Review the section.").count(), 1);
        assert!(!p.contains("<attachments>"));
        assert!(SYSTEM.contains("THE GUIDE") && SYSTEM.contains("Lead with the point"));
    }

    #[test]
    fn attachments_render() {
        let body = json!({
            "template": {"name": "RFC", "sections": []}, "sections": [], "history": [{"role": "user", "content": "hi"}],
            "attachments": [
                {"name": "notes.txt", "kind": "text", "text": "  latency 2s  "},
                {"name": "pic.png", "kind": "image", "mime": "image/png", "data": "AAAA"}
            ]
        });
        let p = build(&body);
        assert!(p.contains("<attachment name=\"notes.txt\">\nlatency 2s\n</attachment>"));
        assert!(!p.contains("AAAA"));
        assert_eq!(images(&body), vec![("image/png".to_string(), "AAAA".to_string())]);
    }
}

#[cfg(test)]
mod sizes {
    /// `cargo test sizes -- --nocapture`: words in the system prompt per mode (a proxy for tokens).
    #[test]
    fn print_sizes() {
        let w = |s: &str| s.split_whitespace().count();
        println!("full (old, every turn): {}", w(super::SYSTEM));
        for (m, sc, l) in [("interview", "section", true), ("interview", "section", false), ("review", "section", false), ("review", "document", false), ("review", "section", true), ("rehearse", "document", false)] {
            println!("{m:9} {sc:8} local={l:5}: {}", w(&super::system_for(m, sc, l)));
        }
    }
}
