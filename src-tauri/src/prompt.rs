//! Builds the one self-contained prompt the coach reads: past chat, a fresh
//! document snapshot, then the new message.
use serde_json::Value;

pub const SYSTEM: &str = concat!(include_str!("coach_prompt.md"), include_str!("../../static/guide.md"));

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
    let sections: Vec<String> = body["sections"].as_array().unwrap_or(&none).iter()
        .map(|x| format!("<section title=\"{}\">\n{}\n</section>", s(&x["title"]), or_empty(s(&x["text"]))))
        .collect();
    let title = match s(&body["title"]) { "" => "Untitled", t => t };
    let mode = match s(&body["mode"]) { "" => "review", m => m };
    let b = &body["brief"];
    let audience = format!(
        "<brief>\nThe ask: {}\nReader: {}\nAfter reading they should: {}\nTheir likely objection: {}\nDecision deadline: {}\nKnown numbers: {}\nNon-goals: {}\n</brief>\n<style_rules>\n{}\n</style_rules>",
        or_empty(s(&b["ask"])), or_empty(s(&body["audience"])), or_empty(s(&body["purpose"])), or_empty(s(&b["objection"])),
        or_empty(s(&b["deadline"])), or_empty(s(&b["numbers"])), or_empty(s(&b["nongoals"])), or_empty(s(&body["style"]))
    );
    format!(
        "<template name=\"{}\">\n{}\n</template>\n{}\n<document title=\"{}\">\n<raw_notes>\n{}\n</raw_notes>\n{}\n</document>\n<current_section>{}</current_section>\n<mode>{}</mode>",
        s(&tpl["name"]), guide.join("\n"), audience, title, or_empty(s(&body["notes"])), sections.join("\n"), s(&body["current"]), mode
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
    format!(
        "<conversation_so_far>\n{}\n</conversation_so_far>\n{}{}\n\n<writer_message>\n{}\n</writer_message>",
        convo.join("\n"), render_attachments(body), render_context(body), last
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
