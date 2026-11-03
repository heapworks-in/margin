# Margin

A private writing coach for technical documents, as a macOS desktop app. You write on a blank dark page; the coach's comments sit in the margin beside the words they are about; the coach itself is a small box in the corner.

- **Templates (skeleton library):** one-pager, design doc, RFC, ADR, PRFAQ, tech strategy, incident postmortem, launch readiness (ORR), runbook, status update, blog post, LinkedIn post, X thread. Each section carries the question it must answer and what reviewers check. *Which type?* in the Library runs the document-type router (pick from the reader's next action).
- **Doc Brief:** under the title: the ask, the reader and what they should do, their likely objection, the decision deadline, known numbers, non-goals. The coach judges every review against it.
- **Panel review and rubric:** *Review document* convenes three reviewer personas chosen by document type (Skeptical VP, Staff peer, On-call SRE, Implementing engineer, Finance/capacity owner, Future maintainer, Dependent team lead); each note names its reviewer, and the reply scores the eight-dimension rubric (bottom line, evidence, options, risk honesty, audience fit, structure, economy, actionability) with a SHIP / REVISE verdict, shown in the status bar.
- **Style rules:** a workspace `style.md` (built-in default provided), editable in Settings, applied in every review; *Lint* checks the document against it. **Thesis** (interview mode) returns the bottom line, the ask and three claims, or THESIS BLOCKED with the questions to answer first.
- **Start anywhere:** a blank page, rough notes in the *Notes* tab, or a pasted draft. The bars in the left margin are the sections the chosen template usually needs: solid when written, dotted when not yet added; click one to add it.
- **Interview me:** the coach asks one question at a time and turns your answers into clean section drafts.
- **Coaching inside the document, Grammarly-style:** the text stays clean. Notes are underlines on the words; a small badge at the end of each heading shows the section's status and how many notes are open. Click, hover, or put the caret on an underline for that note's popup (with Accept for English fixes); click a badge for the section's goal, the reviewer checks (pass / partial / fail after a review), its section-level notes and the next action. Under the title, *For whom* and *so that they…* shape every review.
- **Lessons:** the English rules you keep hitting are tallied in the panel.
- **Review:** one request returns everything: each note pinned to the exact words it is about, with a severity and, where one exists, a replacement (`fix`). English notes show as wavy underlines; hover for the suggestion and press Accept to change the text in place. The app checks every quote against the document and sends the ones that do not match back once for correction, so notes stay grounded in your text.
- **Context:** drop or add files (notes, specs, tickets, CSV, code, PDFs, screenshots) in the panel; the coach reads them with your document. PDFs are text-extracted in the app; images go to the model as images.
- **Guide:** the *Guide* tab is the writing standard the coach reviews against (`static/guide.md`): audience first, lead with the point, numbers over adjectives, show the alternatives, be honest about risk.
- A Notion-style block editor:
  - hover a block for `+` (add below) and `⋮⋮` (drag to move; click for Turn into, Duplicate, Move, Delete)
  - `/` for blocks: text, section, subheading, lists, to-dos, code with highlighting, quote, table, divider
  - Markdown shortcuts as you type (`## `, `- `, `[ ] `, ```` ``` ````, `> `, `**bold**`), and pasted Markdown becomes blocks
  - keyboard: `⌘⇧↑/↓` move block, `⌘D` duplicate, `⌘⇧⌫` delete, `⌘⌥0-9` turn into, `Esc` select block then `↑/↓`, `⌘K` link
  - select text for a toolbar: turn into, bold, italic, underline, strike, code, link
- No side panels: the coach is a chat bar at the bottom (the conversation folds up above it and shows the current section's status), a Word-style status bar at the very bottom carries the insights (page, words, reading time, sections written and ready, open notes, last review, Lessons), and context files live under **+ Context** in the top bar or by dropping files onto the document. `⌘\` hides the coach and the bars. Select any sentence and press *✦ Ask coach* to send just that part.
- Click a draft to put it into your doc (with undo). *Export* copies Markdown or saves a `.md` file to Downloads.

## Install

```bash
brew tap heapworks-in/tap
brew install --cask margin
```

Or download the `.dmg` from the [releases page](https://github.com/heapworks-in/margin/releases). Needs macOS on
Apple Silicon, Node.js, and Claude Code logged in (`claude`, then `/login`) for the bundled coach. The build is not
signed yet: the cask clears the quarantine flag; with the DMG, right-click the app and choose *Open* the first time.

## Workspaces

A workspace is a folder (default `~/Documents/Margin` (an existing `~/Documents/Margin` keeps being used)) with `workspace.json`, `documents/`, `diagrams/` and
`files/`. You can have several and switch between them from the Library (the Margin name or the workspace name
in the header takes you there). Each document is a folder under `documents/` holding `document.json` (text,
comments, check verdicts, chat, audience), `document.md` (a Markdown copy kept current) and `assets/` (its images
and diagrams). `diagrams/` holds standalone Excalidraw drawings (`.svg` + `.excalidraw`) you can edit and insert
into any document; `files/` holds shared context any document can attach.

## Import

From the Library, *Import…* creates a document from a Markdown, text or HTML file, a Word `.docx` (converted in
the app with mammoth), or a Confluence Cloud page by link with your Atlassian email and an API token (fetched on
the Rust side; page text, headings, lists, tables and code come across, images stay in Confluence). Pasting from
Confluence or Word into the editor also works.

## Diagrams and images

Type `/diagram` (or pick Diagram from the `/` menu) to draw in Excalidraw inside the app. *Insert into document*
places it as an SVG block; double-click it to edit again. Paste or drop image files into the text to insert them.
Images and diagrams are saved under the document's `assets/` folder and referenced from the Markdown copy.

## Settings (⚙ in the header)

- **Default data folder**: which workspace the app opens with.
- **Coach agent**: Claude Code (bundled, uses your Claude login) or any other ACP agent by command and arguments
  (for example `npx -y @agentclientprotocol/codex-acp`; Devin once Cognition ships an ACP agent).
- **Model** (`claude-opus-5-5` by default), **effort** (low … max), **skills** to preload (Claude Code skill names),
  and whether to load your own Claude Code user settings (off by default: the coach stays isolated).

## Offline

Everything in the app is local except the coach itself, which talks to the agent you configured: the UI fonts,
Excalidraw and its fonts, the Word converter and the writing guide all ship inside the app. Excalidraw's shape
library is stored in the workspace (`diagrams/library.json`); load `.excalidrawlib` files from disk through its
library menu. The link to the public library site is removed.

## How it talks to Claude

No API key. The Rust backend (`src-tauri/src/acp.rs`) runs Claude Code's [Agent Client Protocol](https://agentclientprotocol.com)
adapter (`@agentclientprotocol/claude-agent-acp`, bundled in the app) and uses your Claude Code login. Each coach reply
runs in a fresh session with no tools, an empty working folder, and none of your Claude Code settings, hooks or CLAUDE.md files.

## Needs

- Node.js (found through your login shell, so nvm works; or set `DOCU_COACH_NODE=/path/to/node`)
- Claude Code logged in: run `claude`, then `/login`

## Develop and build

```bash
npm install                 # the editor's build tools
(cd agent && npm install)   # the ACP adapter, bundled into the app
npm run build               # editor (editor/main.js) and the Excalidraw modal (editor/diagram.jsx) -> static/vendor/
cd src-tauri
cargo tauri dev             # run with live reload of static/index.html
cargo tauri build           # -> target/release/bundle/macos/Margin.app and a .dmg under bundle/dmg/
cargo test                  # prompt tests; `cargo test -- --include-ignored` also runs a real Claude round trip
cargo test grounding -- --ignored --nocapture   # real review of a sample doc: every quote must exist in the text
```

Layout: `static/index.html` is the UI; `editor/main.js` is the document editor (Tiptap), bundled into `static/vendor/editor.js`. `src-tauri/src/` has `main.rs` (commands), `acp.rs` (ACP client),
`prompt.rs` and `coach_prompt.md` (what the coach is told).

The document and chat are saved in the app's local storage.
