# How to use Margin

Margin is a private room where a tough, kind reviewer reads your document before the real reviewers do. You write in the middle; the coach's notes land on your text as underlines; the chat bar at the bottom is where you talk; the status bar keeps the numbers. Nothing leaves your Mac except the text you send to the coach you chose in Settings.

This page walks through one document from blank page to "ship it". Read it once; after that the app's own hints are enough.

## 1. First run

- Install: `brew tap heapworks-in/tap && brew install --cask margin`, or download the DMG from the releases page. The build is not signed yet, so the first time right-click **Margin.app** and choose **Open**.
- The bundled coach is Claude Code. It needs Node.js and a Claude login: run `claude` in a terminal once and `/login`. No API key.
- To use another agent (Devin, Codex, Gemini CLI, Goose), open **Settings** (the gear, top right). Everything Margin finds on your PATH is listed with where it found it; pick one. There is nothing to configure unless you want a custom command.
- **No tokens at all: a local model.** Pick **Local model** in Settings. Margin talks to Ollama (or LM Studio, llama.cpp, vLLM; anything with the OpenAI chat API) on your machine, lists the models it has loaded, and nothing leaves the Mac. Use a model of about 20B parameters or more (`ollama pull gemma4:26b`, `qwen3-coder:30b`, `gpt-oss:20b`); smaller ones follow the coach's format poorly. Reviews take longer than with a hosted model: on an M1 Max, a 26B model needs a minute or two per review, the first one more while the model loads (Margin then keeps it loaded for an hour). Close memory-hungry apps first; a 26B model wants about 16 GB free. Quotes are still verified, so a weak model gives you fewer notes, not wrong ones. Making local models fast (memory, Ollama settings, llama.cpp, speculative decoding, hardware) is its own page: [Running Margin on a local model, fast](https://heapworks-in.github.io/margin/local-models.md).
- Your documents live in a **workspace** folder (`~/Documents/Margin` by default): `documents/` (one folder per document, with a Markdown copy you can read anywhere), `diagrams/`, `files/` (context shared by every document), plus `style.md` and `feedback.md`.

## 2. Start a document

- Click **Margin** (top left) for the Library. Type a title, pick a type, **Create**.
- Not sure of the type? **Which type?** asks a few yes/no questions about the reader's next action and picks for you: one-pager, design doc, RFC, ADR, API specification, technical specification, PRFAQ, tech strategy, postmortem, launch readiness, runbook, status update, blog post, LinkedIn post, X thread.
- **From documents you admire** (Settings → Document types): paste or add one to three documents your organisation considers excellent. The coach derives a document type from them (their sections, the question each answers, what reviewers check) and the style rules they share; you review both in the type editor and save. From then on the coach reviews against your house standard, not a generic one.
- Your own types: **New type…** (Library or Settings → Document types) takes a name, the reviewer personas for its panel, and one line per section: `title | the question it must answer | what reviewers check`. Or start from the open document's headings. Custom types live in the workspace's `templates.json`, so the team shares them, and appear in every picker.
- Already have something? **Import** takes Markdown, text, Word (.docx) and Confluence pages (email + API token).
- The spine on the left (the thin ticks) is every section the template expects. Hover a tick for the section name and the question it must answer. It fills in as you write and turns green when a section is ready.

## 3. Fill the Brief

Under the title: **For** who, **so that they** do what. Click **Brief ▾** for the rest: the ask in one sentence, the reader's likely objection, the decision deadline, the numbers you already know, non-goals.

The coach judges everything against this. A thin brief gets you generic feedback; a sharp brief gets you the question your VP will actually ask. Fill it before you ask for anything.

## 4. Get words on the page

Start however you like. Three ways that work:

- **Just write.** The editor behaves like Notion: type `/` for blocks (section, subheading, lists, to-do, code, quote, table, divider, diagram), Markdown shortcuts (`## `, `- `, `1. `, `[] `, `> `, three backticks), drag handles on hover, and block keys: Cmd-Shift-↑/↓ moves a block, Cmd-D duplicates, Cmd-Shift-Backspace deletes, Escape selects, Cmd-Alt-0…9 turns a block into text, section, subheading, to-do, bullets, numbers, code, quote.
- **Dump notes, let the coach sort them.** Put raw notes in the **Notes** tab, then **Interview** mode → **Sort my notes**. The coach files them into sections as drafts and tells you what is missing.
- **Draft the whole document from notes.** With notes and a Brief in place, **Interview** → **Draft the document** writes one draft for every section of the template, keeps your facts and numbers exactly, puts `[TODO: …]` where a fact is missing instead of guessing, and lists the three gaps to fill first. **Use all drafts** puts them into the document in one click (empty sections are filled; written ones get the draft appended). It runs on the deep coach.
- **No machine prose.** Every draft and fix follows a no-slop rule set: no throat-clearing, no "not X but Y", no jargon (leverage, robust, seamless, delve, crucial…), no adverb padding, no em dashes, a named actor in every sentence, specifics over vague declaratives. The status bar's **Slop** count finds those tells in your text without a model; click it to underline each one with a plain-word replacement you can Accept.
- **Talk.** Press fn twice in any text to dictate (macOS). Then **Reshape dictation** turns the rough speech into clean prose in your meaning and order.

Drafts the coach writes arrive as cards in the chat with **Insert** / **Replace**; nothing goes into your document without you clicking.

## 5. Think it through with the coach (Interview mode)

The chat bar has three modes. **Interview** is for when the thinking is not done yet.

- **Interview me** asks one question at a time about the current section, then writes the section from your answers.
- **Thesis** forces the bottom line: the point in 25 words, the ask, three claims with facts, the strongest counterargument, what would change your mind. If that cannot be written without inventing a fact, the coach says THESIS BLOCKED and asks the questions you need to answer first. Do this before you write anything long.
- **Options table** interviews you for the alternatives (including doing nothing), the criteria the reader cares about, and how each scores, then writes the comparison table with the same criteria for every option.
- **TL;DR** writes the 20-second version (three bullets) and the 2-minute version at the top. Run it again after a review and it adds "since the last review".
- **What is missing?** looks end to end and lists the gaps, most important first.

Every message in the thread carries its time; a coach reply also shows how long it took and which coach answered, so the thread reads as a log of the session. **Stop** gives up on a slow turn.

Type anything in the box too. Broken English is fine; the coach reads meaning and never comments on your chat, only on the document.

## 6. Review a section (Review mode)

Put the cursor in a section and press **Review this section**. Notes appear as underlines in the text, coloured by kind: English, missing, fact, question, structure. A small badge next to the heading shows how many are open.

- Click an underline, or just place the cursor in it, to see the note. English notes carry a fix: **Accept** applies it in place. **Resolve** clears any note.
- Click the heading badge for the section's reviewer checks (pass, partial, fail) and the section status: not started, drafted, needs work, ready.
- The status bar's **open notes** count is a button: it lists every open note by section. Underlined notes open on the text; notes about a whole section (nothing to underline) open from the section's heading badge.
- **Review what changed** (after you edit): sends only the sections you changed since the coach last saw them, and asks the coach which of its earlier notes your edits settle. Those notes resolve themselves ("2 earlier notes addressed by your edits"); the open ones stay, and nothing already fixed gets raised again. Cheaper than a full review, and the way to iterate.
- **Fix English** returns the whole section corrected, keeping your meaning and facts, plus the rules you keep hitting.
- **Lint** checks the document against your style rules (Settings → Style rules; a sensible default is built in).
- **Check facts** lists every claim a reviewer might doubt. **Claims** goes further: a ledger of every claim with its source (brief, attachment, the document, or unsupported). Unsupported claims are underlined; the status bar shows the count. In the ledger, **Evidence…** attaches the file that proves a claim (a dashboard screenshot, a CSV, an incident report): the claim becomes sourced, its underline goes, and the review packet lists the file.
- **Workspace memory**: for whole-document turns (the panel, the reader), the coach also sees the openings of your other documents in the workspace and flags contradictions with a decision, number or name recorded there ("ADR-12 chose Postgres; this design says DynamoDB").
- **30-second read**: the coach reads only the title, first paragraph and headings and says what it thinks you are asking for. If that is not your ask, the structure is wrong, and you learn it in seconds.
- **Read as my reader**: the coach becomes the reader from the Brief and reads once, top to bottom, the way they would. Reader notes land on the text in the first person: where they stop trusting a number, where they skip, where the deciding question comes up. The status bar shows what they walk away with: says yes, would ask, says no. This is the question no review note answers: will it land?
- When a section passes all its checks, the coach says so in the chat and offers the weakest remaining section with one button. Follow it until the spine is green.

Every quote the coach makes is checked against your text. A note that quotes words not in the document is sent back once for correction and dropped if it still does not match. You see "Checked quotes against the text" under each review.

## 7. Add context

**+ Context** (top bar) opens a file dialog (or drop files onto the document) to attach what the coach should know: specs, tickets, dashboards exported as CSV, PDFs, screenshots. Files in the workspace's `files/` folder can be attached to any document. Attachments go to the coach with every message; they are never shown to anyone else.

## 8. Diagrams and images

- `/diagram` opens Excalidraw inside the app. It is fully local, including its shape library (saved in the workspace). Click a diagram to edit it again.
- **Draw**: select a paragraph that describes a flow or an architecture and press **Draw** in the toolbar. The coach returns boxes and arrows from what the text says, nothing more; the app lays it out as an editable diagram.
- Paste or drop images; they are saved in the document's `assets/` folder and referenced from the Markdown copy.

## 9. Before you share: the panel

**Review document** (top right) convenes three reviewers chosen by document type, for example Skeptical VP, Staff peer and On-call SRE for a design doc. Each note names its reviewer. After the notes comes a rubric score on eight dimensions (bottom line, evidence, options, risk honesty, audience fit, structure, economy, actionability) and a verdict: **SHIP** or **REVISE**. The status bar shows "Rubric 3.4 · REVISE"; click it for the table with one line of evidence per score.

Then **Rehearse** mode. **Start the meeting**: the panel asks its hardest question. Answer in the box as you would out loud. Each answer is graded 1 to 5 with one line of why; a good answer the document does not yet say becomes a draft so it lands in the text. **Next question** until **How did it go?** gives the panel's verdict and the top three things to fix. The running score sits in the status bar. This is the review meeting, practised in private.

## 10. Learn from it

- **Lessons** (status bar, right) tallies the English rules you keep hitting across all your documents. **Teach me** on any rule gives three sentences and one exercise. The count per review is shown so you can watch it fall.
- **History** (status bar): a snapshot is taken after every review. Click one for a line diff against the current text and the rubric trend.
- **Record the real review** (Export menu, or the status-bar nudge after you ship): how it went, 1 to 5, kept with the document and shown in the Library, with the workspace average over time. It is the only honest measure of whether the coaching works.
- **Review packet** (Export menu): one Markdown file with the Brief, the ship checks, the panel rubric with evidence, the reader's verdict, the claim ledger and the open notes, followed by the document. Attach it when you send the document: reviewers see the work was done.
- **Learn from feedback** (Review mode): after the real review, paste the comments you received, or **Pull comments from Confluence** straight from the page you published to. The coach compares them with its own notes, says what it missed and what it over-flagged, and writes "Weight:" lines into the workspace's `feedback.md`. Every later panel reads that file, so the coach gets closer to your actual reviewers over time. Edit it any time in Settings.

## 11. Ship it

The status bar's **Ship n/5** is the gate: no high-severity notes open, every claim sourced, no machine-prose tells, the panel says SHIP at 4.0 or better, and your reader does not say no. Click it any time to see which are met. Copy, save and publish run the same checklist first; if something is not met you see the list and can go ahead anyway, knowingly.

**Export**: copy as Markdown, save a `.md` to Downloads, or **Publish to Confluence** (creates a page under a parent page you paste; publishing again updates the same page). Comments are never exported. Diagrams and images stay in Margin; a placeholder marks them.

## 12. Settings

- **Default data folder**: switch between workspaces or create one.
- **Coach agent**: the bundled Claude Code, anything found on your PATH, or a custom ACP command.
- **Quick coach**: conversational turns (Interview me, Thesis, Options table, TL;DR, Fix English, Lint, 30-second read) can go to a faster or free coach, while the judging turns (Review this section, Review document, Read as my reader, Rehearse, Claims, Check facts) stay on the main coach. A local model for quick and Claude for deep keeps most of a session free and fast; each reply in the thread says which coach answered. Margin also sends each turn only what it needs: section-level actions send the current section plus the other headings, reviews carry only the last exchange, and attachments travel only for fact checks or when you name one.
- **Model and skills**: the model list comes from the agent you picked (Claude Code or Devin each report their own); leave it empty for the agent's default. For Claude Code also effort (low to max; higher is slower and costlier), skills to preload, and whether to load your own `~/.claude` settings. Off keeps the coach isolated.
- **Style rules**: your house style, applied in every review and by Lint. Shared with the team through the workspace.
- **Reviewer feedback**: what Learn from feedback collected.

## 13. A good session, start to finish

1. Create the document; let **Which type?** choose. Fill the Brief, especially the objection.
2. **Thesis**. If it is blocked, answer the questions first; that is the real work.
3. Dump notes; **Sort my notes**; **Interview me** for the thin sections.
4. **Review this section** as you go. Accept the English fixes. Follow the coach to the weakest section.
5. **30-second read**, then **Claims** on the sections with numbers.
6. **Review document**. Fix the high-severity notes. Repeat until SHIP.
7. **Rehearse** the meeting once.
8. **TL;DR**, export, share. After the real review, **Learn from feedback**.

## 14. Privacy

Documents, diagrams, attachments and history are plain files in your workspace. The app makes no network requests of its own; the only traffic is the coach process you chose talking to its own service (Claude Code to Anthropic, Devin to Cognition, and so on). Excalidraw is bundled with its fonts and library and never calls home. The Confluence token is stored only if you tick Remember.
