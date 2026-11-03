// The document editor: Tiptap with Notion-style block behaviour, Markdown, slash
// commands, a block menu and a selection toolbar. Bundled into static/vendor/editor.js
// by `npm run build:editor`. Exposes window.DocEditor.
import { Editor, Extension, Mark } from '@tiptap/core'
import StarterKit from '@tiptap/starter-kit'
import { TaskList, TaskItem } from '@tiptap/extension-list'
import { TableKit } from '@tiptap/extension-table'
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight'
import Suggestion, { exitSuggestion } from '@tiptap/suggestion'
import { Markdown } from '@tiptap/markdown'
import Image from '@tiptap/extension-image'
import { Plugin, NodeSelection, TextSelection } from '@tiptap/pm/state'
import { Decoration, DecorationSet } from '@tiptap/pm/view'
import { createLowlight, common } from 'lowlight'

// A "section" is a level-2 heading plus everything until the next one.
const isSection = n => n.type.name === 'heading' && n.attrs.level <= 2
const norm = s => (s || '').trim().toLowerCase()
const countWords = t => (t.trim().match(/\S+/g) || []).length
const isItem = n => n.type.name === 'listItem' || n.type.name === 'taskItem'

/* ---------- block types ---------- */
// Strip the current block back to a paragraph so any type can be applied to it.
function clearBlock(c, editor) {
  if (editor.isActive('bulletList')) c.toggleBulletList()
  if (editor.isActive('orderedList')) c.toggleOrderedList()
  if (editor.isActive('taskList')) c.toggleTaskList()
  if (editor.isActive('blockquote')) c.lift('blockquote')
  if (editor.isActive('codeBlock')) c.toggleCodeBlock()
  if (editor.isActive('heading')) c.setParagraph()
  return c
}
const BLOCKS = [
  { title: 'Text', about: 'Plain text. Just start writing.', icon: 'T', keys: '', key: '0', turn: true, run: c => c.setParagraph() },
  { title: 'Section', about: 'A section of the document.', icon: 'H2', keys: '##', key: '1', turn: true, run: c => c.setNode('heading', { level: 2 }) },
  { title: 'Subheading', about: 'A smaller heading inside a section.', icon: 'H3', keys: '###', key: '2', turn: true, run: c => c.setNode('heading', { level: 3 }) },
  { title: 'Bulleted list', about: 'A simple list.', icon: '•', keys: '-', key: '5', turn: true, run: c => c.toggleBulletList() },
  { title: 'Numbered list', about: 'A list with numbers.', icon: '1.', keys: '1.', key: '6', turn: true, run: c => c.toggleOrderedList() },
  { title: 'To-do list', about: 'Track tasks with checkboxes.', icon: '☑', keys: '[]', key: '4', turn: true, run: c => c.toggleTaskList() },
  { title: 'Code', about: 'A code block with syntax colours.', icon: '&lt;/&gt;', keys: '```', key: '8', turn: true, run: c => c.setCodeBlock() },
  { title: 'Quote', about: 'Capture a quote.', icon: '❝', keys: '>', key: '9', turn: true, run: c => c.setBlockquote() },
  { title: 'Table', about: 'A table with a header row.', icon: '▦', keys: '', run: c => c.insertTable({ rows: 3, cols: 3, withHeaderRow: true }) },
  { title: 'Divider', about: 'A line between blocks.', icon: '—', keys: '---', run: c => c.setHorizontalRule() },
  { title: 'Diagram', about: 'Draw a diagram with Excalidraw.', icon: '◇', keys: '', run: (c, editor) => { setTimeout(() => editor.storage.pasteMarkdown.onDiagram?.(), 0); return c } },
]
const applyBlock = (editor, block, chain = editor.chain().focus()) => block.run(clearBlock(chain, editor), editor).run()

/* ---------- block commands (the unit Notion treats as a block: a list item, else a top-level node) ---------- */
function blockAt(state) {
  const sel = state.selection
  if (sel instanceof NodeSelection) return { node: sel.node, pos: sel.from }
  const $from = sel.$from
  for (let d = $from.depth; d >= 1; d--) {
    const n = $from.node(d)
    if (d === 1 || isItem(n)) return { node: n, pos: $from.before(d) }
  }
  return null
}
function restore(tr, state, pos) {
  const sel = state.selection instanceof NodeSelection
    ? NodeSelection.create(tr.doc, pos)
    : TextSelection.near(tr.doc.resolve(Math.min(pos + (state.selection.from - blockAt(state).pos), tr.doc.content.size)))
  return tr.setSelection(sel).scrollIntoView()
}
function moveBlock(editor, dir) {
  const { state, view } = editor
  const b = blockAt(state)
  if (!b) return false
  const $ = state.doc.resolve(b.pos), idx = $.index(), parent = $.parent
  if (dir < 0 && idx === 0) return true
  if (dir > 0 && idx === parent.childCount - 1) return true
  const sib = parent.child(idx + dir), end = b.pos + b.node.nodeSize
  const tr = state.tr.delete(b.pos, end)
  const to = dir < 0 ? b.pos - sib.nodeSize : b.pos + sib.nodeSize
  tr.insert(to, b.node)
  view.dispatch(restore(tr, state, to))
  return true
}
function duplicateBlock(editor) {
  const { state, view } = editor
  const b = blockAt(state)
  if (!b) return false
  const at = b.pos + b.node.nodeSize
  view.dispatch(restore(state.tr.insert(at, b.node), state, at))
  return true
}
function deleteBlock(editor) {
  const { state, view } = editor
  const b = blockAt(state)
  if (!b) return false
  const tr = state.tr.delete(b.pos, b.pos + b.node.nodeSize)
  if (tr.doc.childCount === 0) tr.insert(0, state.schema.nodes.paragraph.create())
  view.dispatch(tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(b.pos, tr.doc.content.size)), -1)).scrollIntoView())
  return true
}
function selectBlock(editor) {
  const { state, view } = editor
  if (state.selection instanceof NodeSelection) {
    view.dispatch(state.tr.setSelection(TextSelection.near(state.doc.resolve(state.selection.from + 1))))
    return true
  }
  const b = blockAt(state)
  if (!b) return false
  view.dispatch(state.tr.setSelection(NodeSelection.create(state.doc, b.pos)))
  return true
}
function walkBlocks(editor, dir) {
  const { state, view } = editor
  if (!(state.selection instanceof NodeSelection)) return false
  const $ = state.doc.resolve(state.selection.from), idx = $.index(), parent = $.parent
  const next = idx + dir
  if (next < 0 || next >= parent.childCount) return true
  const pos = dir < 0 ? state.selection.from - parent.child(next).nodeSize : state.selection.to
  view.dispatch(state.tr.setSelection(NodeSelection.create(state.doc, pos)).scrollIntoView())
  return true
}
// Put a text cursor (or node selection, for atoms) inside the block at `pos`.
function enter(editor, pos, node) {
  const { state, view } = editor
  const sel = node.isAtom || !node.isTextblock && node.content.size === 0
    ? NodeSelection.create(state.doc, pos)
    : TextSelection.near(state.doc.resolve(pos + 1))
  view.dispatch(state.tr.setSelection(sel))
  view.focus()
}

const BlockKeys = Extension.create({
  name: 'blockKeys',
  addKeyboardShortcuts() {
    const keys = {
      'Mod-Shift-ArrowUp': () => moveBlock(this.editor, -1),
      'Mod-Shift-ArrowDown': () => moveBlock(this.editor, 1),
      'Mod-d': () => duplicateBlock(this.editor),
      'Mod-Shift-Backspace': () => deleteBlock(this.editor),
      Escape: () => selectBlock(this.editor),
      // Notion: Enter at the start of a heading pushes it down and leaves an empty line above.
      Enter: () => {
        const { state, view } = this.editor, { $from, empty } = state.selection
        if (!empty || $from.parent.type.name !== 'heading' || $from.parentOffset !== 0 || $from.parent.content.size === 0) return false
        const at = $from.before()
        const tr = state.tr.insert(at, state.schema.nodes.paragraph.create())
        view.dispatch(tr.setSelection(TextSelection.create(tr.doc, at + 3)).scrollIntoView())
        return true
      },
      // Notion: Backspace at the start of a heading turns it into text before it can merge upward.
      Backspace: () => {
        const { $from, empty } = this.editor.state.selection
        if (!empty || $from.parent.type.name !== 'heading' || $from.parentOffset !== 0) return false
        return this.editor.commands.setParagraph()
      },
      ArrowUp: () => walkBlocks(this.editor, -1),
      ArrowDown: () => walkBlocks(this.editor, 1),
    }
    for (const b of BLOCKS) if (b.key) keys[`Mod-Alt-${b.key}`] = () => applyBlock(this.editor, b)
    return keys
  },
})

/* ---------- menus ---------- */
function popup(anchor, rows, onPick, onClose, side) {
  const el = document.createElement('div')
  el.className = 'slash'
  let index = rows.findIndex(r => r.block || r.action), buttons = []
  const draw = () => {
    el.replaceChildren(); buttons = []
    rows.forEach((r, i) => {
      if (r.head) { const h = document.createElement('div'); h.className = 'slash-head'; h.textContent = r.head; el.append(h); return }
      if (r.sep) { const s = document.createElement('div'); s.className = 'slash-sep'; el.append(s); return }
      const b = document.createElement('button')
      b.type = 'button'
      b.className = 'slash-item' + (i === index ? ' on' : '') + (r.small ? ' small' : '')
      b.innerHTML = r.block
        ? `<i class="slash-icon">${r.block.icon}</i><span class="slash-text"><b>${r.block.title}</b><small>${r.block.about}</small></span>${r.block.keys ? `<kbd>${r.block.keys}</kbd>` : ''}`
        : `<span class="slash-text"><b>${r.label}</b></span>${r.keys ? `<kbd>${r.keys}</kbd>` : ''}`
      b.onmousedown = e => { e.preventDefault(); e.stopPropagation(); pick(r) }
      el.append(b); buttons[i] = b
    })
    buttons[index]?.scrollIntoView({ block: 'nearest' })
  }
  const close = () => { el.remove(); document.removeEventListener('keydown', onKey, true); document.removeEventListener('mousedown', onOutside, true); onClose?.() }
  const pick = r => { close(); onPick(r) }
  const step = dir => { const n = rows.length; let i = index; do { i = (i + dir + n) % n } while (!(rows[i].block || rows[i].action)); index = i; draw() }
  const onKey = e => {
    if (e.key === 'ArrowDown') step(1)
    else if (e.key === 'ArrowUp') step(-1)
    else if (e.key === 'Enter') pick(rows[index])
    else if (e.key === 'Escape') close()
    else return
    e.preventDefault(); e.stopPropagation()
  }
  const onOutside = e => { if (!el.contains(e.target)) close() }
  document.addEventListener('keydown', onKey, true)
  document.addEventListener('mousedown', onOutside, true)
  document.body.append(el)
  draw()
  const r = anchor.getBoundingClientRect ? anchor.getBoundingClientRect() : anchor
  const clamp = (v, max) => Math.max(8, Math.min(v, max - 8))
  if (side) {
    el.style.left = clamp(r.right + 8, innerWidth - el.offsetWidth) + 'px'
    el.style.top = clamp(r.top - 4, innerHeight - el.offsetHeight) + 'px'
  } else {
    el.style.left = clamp(r.left, innerWidth - el.offsetWidth) + 'px'
    el.style.top = (r.bottom + 6 + el.offsetHeight > innerHeight ? r.top - 6 - el.offsetHeight : r.bottom + 6) + 'px'
  }
  return close
}
const turnRows = () => BLOCKS.filter(b => b.turn).map(b => ({ block: b }))
function blockMenu(editor, anchor, block, onClose) {
  enter(editor, block.pos, block.node)
  const rows = [
    { head: 'Turn into' }, ...turnRows(), { sep: true },
    { action: 'duplicate', label: 'Duplicate', keys: '⌘D' },
    { action: 'up', label: 'Move up', keys: '⌘⇧↑' },
    { action: 'down', label: 'Move down', keys: '⌘⇧↓' },
    { action: 'delete', label: 'Delete', keys: '⌘⇧⌫' },
  ]
  popup(anchor, rows, r => {
    if (r.block) return applyBlock(editor, r.block)
    if (r.action === 'duplicate') duplicateBlock(editor)
    if (r.action === 'up') moveBlock(editor, -1)
    if (r.action === 'down') moveBlock(editor, 1)
    if (r.action === 'delete') deleteBlock(editor)
  }, onClose, true)
}

function slashMenu() {
  let el = null, items = [], index = 0, pick = null
  const draw = () => {
    el.replaceChildren()
    const head = document.createElement('div'); head.className = 'slash-head'; head.textContent = 'Blocks'; el.append(head)
    if (!items.length) { el.innerHTML = '<div class="slash-empty">No matching block</div>'; return }
    items.forEach((it, i) => {
      const b = document.createElement('button')
      b.type = 'button'
      b.className = 'slash-item' + (i === index ? ' on' : '')
      b.innerHTML = `<i class="slash-icon">${it.icon}</i><span class="slash-text"><b>${it.title}</b><small>${it.about}</small></span>${it.keys ? `<kbd>${it.keys}</kbd>` : ''}`
      b.onmousedown = e => { e.preventDefault(); pick(it) }
      el.append(b)
    })
    el.querySelector('.on')?.scrollIntoView({ block: 'nearest' })
  }
  // Position from the editor's own caret coordinates (Suggestion's clientRect can be stale).
  const place = p => {
    if (!el) return
    const r = p.editor.view.coordsAtPos(p.range.from)
    el.style.left = r.left + 'px'
    el.style.top = r.bottom + 6 + 'px'
    if (r.bottom + 6 + el.offsetHeight > innerHeight) el.style.top = r.top - 6 - el.offsetHeight + 'px'
  }
  const sync = p => { items = p.items; index = 0; pick = p.command; draw(); place(p) }
  return {
    onStart: p => { el = document.createElement('div'); el.className = 'slash'; document.body.append(el); sync(p) },
    onUpdate: sync,
    onKeyDown: ({ event, view }) => {
      const n = Math.max(items.length, 1)
      if (event.key === 'ArrowDown') { index = (index + 1) % n; draw(); return true }
      if (event.key === 'ArrowUp') { index = (index - 1 + n) % n; draw(); return true }
      if (event.key === 'Enter') { if (items[index]) pick(items[index]); return true }
      if (event.key === 'Escape') { exitSuggestion(view); return true }
      return false
    },
    onExit: () => { el?.remove(); el = null },
  }
}

const SlashCommands = Extension.create({
  name: 'slashCommands',
  addProseMirrorPlugins() {
    return [Suggestion({
      editor: this.editor,
      char: '/',
      allow: ({ editor }) => !editor.isActive('codeBlock'),
      items: ({ query }) => BLOCKS.filter(b => norm(b.title + ' ' + b.about).includes(norm(query))),
      command: ({ editor, range, props }) => applyBlock(editor, props, editor.chain().focus().deleteRange(range)),
      render: slashMenu,
    })]
  },
})

// Pasted plain text that looks like Markdown becomes blocks.
const MD_HINT = /^(#{1,6} |[-*+] |\d+\. |```|> |\|.*\|)|(\*\*|`)[^\n]+\2/m
const PasteMarkdown = Extension.create({
  name: 'pasteMarkdown',
  addStorage() { return { onImage: null, onDiagram: null } },
  addProseMirrorPlugins() {
    const editor = this.editor
    return [new Plugin({
      props: {
        handleDrop: (view, event) => {
          const files = [...(event.dataTransfer?.files || [])].filter(f => f.type.startsWith('image/'))
          if (!files.length || !editor.storage.pasteMarkdown?.onImage) return false
          event.preventDefault()
          const pos = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos
          for (const f of files) editor.storage.pasteMarkdown.onImage(f, pos)
          return true
        },
        handlePaste: (view, event) => {
          const files = [...(event.clipboardData?.files || [])].filter(f => f.type.startsWith('image/'))
          if (files.length && editor.storage.pasteMarkdown?.onImage) { event.preventDefault(); for (const f of files) editor.storage.pasteMarkdown.onImage(f); return true }
          if (editor.isActive('codeBlock') || event.clipboardData.getData('text/html')) return false
          const text = event.clipboardData.getData('text/plain')
          if (!text || !MD_HINT.test(text)) return false
          editor.commands.insertContent(text, { contentType: 'markdown' })
          return true
        },
      },
    })]
  },
})

// Section highlight and placeholders, drawn from the state being rendered.
// (Tiptap's Placeholder reads editor.state, which lags one transaction behind
// during setContent and throws "position out of range".)
const Guides = hintFor => Extension.create({
  name: 'guides',
  addProseMirrorPlugins() {
    return [new Plugin({
      props: {
        decorations: state => {
          const at = state.selection.from, decos = []
          let current = null, prev = null
          const hint = (pos, node, text) => text && decos.push(Decoration.node(pos, pos + node.nodeSize, { class: 'is-empty' + (at >= pos && at <= pos + node.nodeSize ? ' has-cursor' : ''), 'data-placeholder': text }))
          state.doc.forEach((node, pos) => {
            if (isSection(node) && pos < at) current = { node, pos }
            const empty = node.content.size === 0
            const hasCursor = at >= pos && at <= pos + node.nodeSize
            if (empty && node.type.name === 'heading') hint(pos, node, node.attrs.level <= 2 ? 'Section' : 'Heading')
            else if (empty && node.type.name === 'paragraph') {
              if (state.doc.childCount === 1) hint(pos, node, "Start writing, or type '/' for a block. Press a bar on the left to add a section.")
              else if (prev && isSection(prev)) hint(pos, node, hintFor(prev.textContent) || (hasCursor ? "Type '/' for blocks" : ''))
              else if (hasCursor) hint(pos, node, "Type '/' for blocks")
            }
            prev = node
          })
          if (current) decos.push(Decoration.node(current.pos, current.pos + current.node.nodeSize, { class: 'current-section' }))
          return DecorationSet.create(state.doc, decos)
        },
      },
    })]
  },
})

/* ---------- section coach: widgets under each heading and at each section's end ---------- */
// The app supplies `render(section) -> { top: {sig, dom}|null, bottom: {sig, dom}|null }`.
// DOM is cached by signature so typing never rebuilds a widget (and never steals focus from it).
const coachState = { render: null, cache: new Map(), version: 0 }
const widgetSpec = key => ({ side: 1, key, ignoreSelection: true, stopEvent: () => true })
const SectionCoach = Extension.create({
  name: 'sectionCoach',
  addProseMirrorPlugins() {
    return [new Plugin({
      props: {
        decorations: state => {
          if (!coachState.render) return DecorationSet.empty
          const tops = []; state.doc.forEach((node, pos) => tops.push({ node, pos }))
          const decos = []
          tops.forEach((h, i) => {
            if (!isSection(h.node)) return
            let j = i + 1
            while (j < tops.length && !isSection(tops[j].node)) j++
            const title = h.node.textContent.trim() || 'Untitled section'
            const end = j < tops.length ? tops[j].pos : state.doc.content.size
            const out = coachState.render({ title, words: countWords(state.doc.textBetween(h.pos + h.node.nodeSize, end, ' ')) })
            for (const [slot, at] of [['tail', h.pos + h.node.nodeSize - 1], ['top', h.pos + h.node.nodeSize], ['bottom', end]]) {
              const w = out && out[slot]
              if (!w) continue
              const key = `${slot}:${title}:${w.sig}`
              let dom = coachState.cache.get(key)
              if (!dom) { dom = w.dom(); dom.contentEditable = 'false'; dom.className = (dom.className ? dom.className + ' ' : '') + (slot === 'tail' ? 'coach-inline' : 'coach-widget'); coachState.cache.set(key, dom) }
              decos.push(Decoration.widget(at, dom, widgetSpec(key)))
            }
          })
          if (coachState.cache.size > 200) coachState.cache.clear()
          return DecorationSet.create(state.doc, decos)
        },
      },
    })]
  },
})

/* ---------- images and diagrams ---------- */
// An image block. A diagram is an image that also carries its Excalidraw scene, so it can be edited again.
const Figure = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      scene: { default: null, parseHTML: el => el.getAttribute('data-scene'), renderHTML: a => a.scene ? { 'data-scene': a.scene } : {} },
      file: { default: null, parseHTML: el => el.getAttribute('data-file'), renderHTML: a => a.file ? { 'data-file': a.file } : {} },
    }
  },
  renderMarkdown: node => `![${(node.attrs.alt || (node.attrs.scene ? 'diagram' : 'image')).replace(/[\[\]]/g, '')}](${node.attrs.file ? 'assets/' + node.attrs.file : node.attrs.src})`,
})
const svgDataUrl = svg => 'data:image/svg+xml;base64,' + btoa(unescape(encodeURIComponent(svg)))

/* ---------- coach comments: a mark on the quoted words ---------- */
const CommentMark = Mark.create({
  name: 'comment',
  inclusive: false,
  excludes: '',
  addAttributes() { return { id: { default: null }, kind: { default: 'structure' } } },
  parseHTML() { return [{ tag: 'mark[data-comment]', getAttrs: el => ({ id: el.getAttribute('data-comment'), kind: el.getAttribute('data-kind') }) }] },
  renderHTML({ HTMLAttributes }) { return ['mark', { 'data-comment': HTMLAttributes.id, 'data-kind': HTMLAttributes.kind, class: 'c-mark' }, 0] },
  renderMarkdown: (node, h) => h.renderChildren(node),
})
const squash = t => t.toLowerCase().replace(/[“”"'’‘`]/g, '').replace(/\s+/g, ' ').trim()
// Finds `quote` inside one text block between from and to. Returns {from, to} in document positions.
function findText(state, quote, from, to) {
  const q = squash(quote)
  if (!q) return null
  let hit = null
  state.doc.nodesBetween(from, to, (node, pos) => {
    if (hit || !node.isTextblock) return
    // Map squashed text back to positions, character by character.
    const map = []; let text = ''
    node.forEach((child, off) => {
      if (!child.isText) return
      const base = pos + 1 + off
      for (let i = 0; i < child.text.length; i++) { text += child.text[i]; map.push(base + i) }
    })
    const lower = text.toLowerCase().replace(/[“”"'’‘`]/g, c => ' '.repeat(c.length))
    // collapse whitespace while keeping an index map
    let norm = '', idx = []
    for (let i = 0; i < lower.length; i++) {
      const c = /\s/.test(lower[i]) ? ' ' : lower[i]
      if (c === ' ' && (norm.endsWith(' ') || !norm)) continue
      norm += c; idx.push(i)
    }
    const at = norm.indexOf(q)
    if (at >= 0) hit = { from: map[idx[at]], to: map[idx[at + q.length - 1]] + 1 }
    return false
  })
  return hit
}

/* ---------- selection toolbar ---------- */
function toolbar() {
  const el = document.createElement('div')
  el.className = 'bubble'
  const row = document.createElement('div'); row.className = 'bubble-row'
  const mk = (label, cls, title) => { const b = document.createElement('button'); b.type = 'button'; b.textContent = label; b.className = cls; b.title = title || label; return b }
  const ask = mk('✦ Ask coach', 'b-ask', 'Send this part to the coach')
  const turn = mk('Text', 'b-turn', 'Turn into')
  const marks = [
    ['B', 'bold', 'Bold ⌘B', c => c.toggleBold()],
    ['I', 'italic', 'Italic ⌘I', c => c.toggleItalic()],
    ['U', 'underline', 'Underline ⌘U', c => c.toggleUnderline()],
    ['S', 'strike', 'Strikethrough ⌘⇧X', c => c.toggleStrike()],
    ['</>', 'code', 'Code ⌘E', c => c.toggleCode()],
  ].map(([label, name, title, run]) => { const b = mk(label, 'b-' + name, title); b.dataset.name = name; b._run = run; return b })
  const link = mk('Link', 'b-link', 'Link ⌘K'); link.dataset.name = 'link'
  row.append(ask, turn, ...marks, link)
  const linkRow = document.createElement('form'); linkRow.className = 'bubble-link'; linkRow.hidden = true; linkRow.noValidate = true
  const input = document.createElement('input'); input.type = 'text'; input.placeholder = 'Paste a link and press Enter'; input.spellcheck = false
  const remove = mk('Remove', 'b-remove')
  linkRow.append(input, remove)
  el.append(row, linkRow)
  return { el, turn, marks, link, ask, linkRow, input, remove }
}

/** Empty skeleton: one section heading and an empty paragraph per title. */
const skeleton = titles => ({
  type: 'doc',
  content: titles.flatMap(t => [{ type: 'heading', attrs: { level: 2 }, content: [{ type: 'text', text: t }] }, { type: 'paragraph' }]),
})

/* ---------- block handle: + and ⋮⋮ in the gutter of the hovered block ---------- */
// Resolves the block under the mouse with ProseMirror's own coordinate lookup, so it
// also works for the gutter beside a block, and treats list items as blocks.
function blockAtCoords(view, x, y) {
  const box = view.dom.getBoundingClientRect()
  const inside = { left: Math.min(Math.max(x, box.left + 4), box.right - 4), top: y }
  const hit = view.posAtCoords(inside)
  if (!hit) return null
  // Nearest text position first (it sits inside the list item); the enclosing node only for atoms.
  for (const at of [hit.pos, hit.inside]) {
    if (at < 0) continue
    const $ = view.state.doc.resolve(at)
    for (let d = $.depth; d >= 1; d--) {
      const n = $.node(d)
      if (d === 1 || isItem(n)) return { node: n, pos: $.before(d) }
    }
  }
  if (hit.inside >= 0) { const n = view.state.doc.nodeAt(hit.inside); if (n) return { node: n, pos: hit.inside } }
  return null
}
function handle(editor, host) {
  const el = document.createElement('div')
  el.className = 'handle'
  const plus = document.createElement('button'); plus.type = 'button'; plus.className = 'plus'; plus.title = 'Add a block below'; plus.textContent = '+'
  const grip = document.createElement('button'); grip.type = 'button'; grip.className = 'grip'; grip.title = 'Click for options, drag to move'; grip.textContent = '⋮⋮'; grip.draggable = true
  el.append(plus, grip)
  host.append(el)
  let hovered = null, pinned = false
  const show = b => {
    hovered = b
    const dom = editor.view.nodeDOM(b.pos)
    if (!(dom instanceof HTMLElement)) { el.hidden = true; return }
    const r = dom.getBoundingClientRect(), h = host.getBoundingClientRect()
    const line = Math.min(parseFloat(getComputedStyle(dom).lineHeight) || 24, r.height)
    el.style.top = r.top - h.top + (line - 24) / 2 + 'px'
    el.style.left = r.left - h.left - 46 + 'px'
    el.hidden = false
  }
  const scroller = host.closest('.scroll') || host.parentElement
  scroller.addEventListener('mousemove', e => {
    if (pinned || el.contains(e.target)) return
    const b = blockAtCoords(editor.view, e.clientX, e.clientY)
    if (b) show(b); else { el.hidden = true; hovered = null }
  })
  scroller.addEventListener('mouseleave', () => { if (!pinned) { el.hidden = true; hovered = null } })
  editor.on('update', () => { el.hidden = true; hovered = null })
  return { el, plus, grip, hovered: () => hovered, show, pin: v => { pinned = v }, locate: blockAtCoords }
}

const blockLabel = editor => {
  if (editor.isActive('heading', { level: 2 })) return 'Section'
  if (editor.isActive('heading', { level: 3 })) return 'Subheading'
  if (editor.isActive('taskList')) return 'To-do'
  if (editor.isActive('bulletList')) return 'Bulleted list'
  if (editor.isActive('orderedList')) return 'Numbered list'
  if (editor.isActive('codeBlock')) return 'Code'
  if (editor.isActive('blockquote')) return 'Quote'
  return 'Text'
}

export function create({ element, content, hintFor, onChange, onSelect, onAsk, onCommentClick, onImage, onDiagramClick, onDiagram }) {
  const bar = toolbar()
  const editor = new Editor({
    element,
    extensions: [
      StarterKit.configure({ codeBlock: false, heading: { levels: [1, 2, 3, 4] }, link: { openOnClick: false, autolink: true } }),
      CodeBlockLowlight.configure({ lowlight: createLowlight(common) }),
      TaskList,
      TaskItem.configure({ nested: true }),
      TableKit.configure({ table: { resizable: false } }),
      Markdown,
      CommentMark,
      SectionCoach,
      Figure.configure({ inline: false, allowBase64: true }),
      SlashCommands,
      PasteMarkdown,
      BlockKeys,
      Guides(hintFor),
    ],
    content: content || skeleton([]),
    onUpdate: () => onChange(),
    onSelectionUpdate: () => onSelect(),
    onTransaction: () => {
      bar.marks.forEach(b => b.classList.toggle('on', editor.isActive(b.dataset.name)))
      bar.link.classList.toggle('on', editor.isActive('link'))
      bar.turn.textContent = blockLabel(editor) + ' ▾'
    },
  })
  // Toolbar over the selection, from the selection's own coordinates.
  document.body.append(bar.el)
  bar.el.hidden = true
  const placeBar = () => {
    const { state } = editor
    const show = editor.isFocused && !state.selection.empty && !(state.selection instanceof NodeSelection) && !editor.isActive('codeBlock')
    if (!show) { bar.el.hidden = true; bar.linkRow.hidden = true; return }
    const a = editor.view.coordsAtPos(state.selection.from), b = editor.view.coordsAtPos(state.selection.to)
    const top = Math.min(a.top, b.top), mid = a.top === b.top ? (a.left + b.right) / 2 : a.left + 120
    bar.el.hidden = false
    bar.el.style.left = Math.max(8, Math.min(mid - bar.el.offsetWidth / 2, innerWidth - bar.el.offsetWidth - 8)) + 'px'
    bar.el.style.top = Math.max(8, top - 8 - bar.el.offsetHeight) + 'px'
  }
  editor.on('selectionUpdate', placeBar)
  editor.on('focus', placeBar)
  editor.on('blur', () => { if (!bar.el.contains(document.activeElement)) setTimeout(() => { if (!bar.el.contains(document.activeElement)) { bar.el.hidden = true; bar.linkRow.hidden = true } }, 0) })
  ;(element.closest('.scroll') || window).addEventListener('scroll', () => { if (!bar.el.hidden) placeBar() }, { passive: true })
  bar.marks.forEach(b => { b.onmousedown = e => { e.preventDefault(); b._run(editor.chain().focus()).run() } })
  bar.turn.onmousedown = e => {
    e.preventDefault()
    popup(bar.turn, [{ head: 'Turn into' }, ...turnRows()], r => applyBlock(editor, r.block))
  }
  const openLink = () => {
    if (editor.state.selection.empty) return
    placeBar()
    bar.linkRow.hidden = false
    bar.input.value = editor.getAttributes('link').href || ''
    requestAnimationFrame(() => { bar.input.focus(); bar.input.select() })
  }
  bar.link.onmousedown = e => { e.preventDefault(); openLink() }
  bar.ask.onmousedown = e => { e.preventDefault(); const { from, to } = editor.state.selection; onAsk?.(editor.state.doc.textBetween(from, to, ' ')) }
  bar.linkRow.onsubmit = e => {
    e.preventDefault()
    const href = bar.input.value.trim()
    const c = editor.chain().focus().extendMarkRange('link')
    ;(href ? c.setLink({ href: /^[a-z]+:/i.test(href) ? href : 'https://' + href }) : c.unsetLink()).run()
    bar.linkRow.hidden = true
  }
  bar.remove.onmousedown = e => { e.preventDefault(); editor.chain().focus().extendMarkRange('link').unsetLink().run(); bar.linkRow.hidden = true }
  bar.input.onkeydown = e => { if (e.key === 'Escape') { bar.linkRow.hidden = true; editor.commands.focus() } }
  editor.view.dom.addEventListener('keydown', e => { if ((e.metaKey || e.ctrlKey) && e.key === 'k' && !editor.state.selection.empty) { e.preventDefault(); openLink() } })

  element.addEventListener('click', e => { const m = e.target.closest('mark[data-comment]'); if (m) onCommentClick?.(m.dataset.comment) })
  editor.storage.pasteMarkdown.onImage = onImage || null
  editor.storage.pasteMarkdown.onDiagram = onDiagram || null
  element.addEventListener('dblclick', e => { const img = e.target.closest('img[data-scene]'); if (img) { const pos = editor.view.posAtDOM(img, 0); onDiagramClick?.(pos, img.getAttribute('data-scene')) } })
  const grab = handle(editor, element)
  grab.plus.onmousedown = e => {
    e.preventDefault()
    const b = grab.hovered()
    if (!b) return
    const at = b.pos + b.node.nodeSize
    editor.chain().insertContentAt(at, { type: 'paragraph' }).setTextSelection(at + 1).focus().insertContent('/').run()
  }
  grab.grip.onmousedown = e => e.preventDefault()
  grab.grip.onclick = e => {
    const b = grab.hovered()
    if (!b) return
    e.preventDefault()
    grab.pin(true)
    blockMenu(editor, grab.grip, b, () => grab.pin(false))
  }
  // Dragging the grip hands the block to ProseMirror's own drag and drop.
  grab.grip.addEventListener('dragstart', e => {
    const b = grab.hovered()
    if (!b) return e.preventDefault()
    const { view } = editor
    view.dispatch(view.state.tr.setSelection(NodeSelection.create(view.state.doc, b.pos)))
    view.dragging = { slice: view.state.selection.content(), move: true }
    const dom = view.nodeDOM(b.pos)
    if (dom instanceof HTMLElement) e.dataTransfer.setDragImage(dom, 0, 0)
    e.dataTransfer.effectAllowed = 'copyMove'
    e.dataTransfer.setData('text/plain', '')
  })

  const tops = () => { const out = []; editor.state.doc.forEach((node, pos) => out.push({ node, pos })); return out }
  const ranges = () => {
    const t = tops(), res = []
    t.forEach((h, i) => {
      if (!isSection(h.node)) return
      let j = i + 1
      while (j < t.length && !isSection(t[j].node)) j++
      res.push({
        title: h.node.textContent.trim() || 'Untitled section',
        headingPos: h.pos,
        from: h.pos + h.node.nodeSize,
        to: j < t.length ? t[j].pos : editor.state.doc.content.size,
        nodes: t.slice(i + 1, j).map(x => x.node),
      })
    })
    return res
  }
  const md = nodes => editor.markdown.serialize({ type: 'doc', content: nodes.map(n => n.toJSON()) }).trim()
  const find = title => ranges().find(r => norm(r.title) === norm(title))

  return {
    editor,
    hovered: () => grab.hovered(),
    toolbar: bar.el,
    /** Replace the whole document with Markdown or HTML (imports). */
    setMarkdown(md) { editor.commands.setContent(md, { contentType: 'markdown' }) },
    setHTML(html) { editor.commands.setContent(html, { contentType: 'html' }) },
    insertImage({ src, file, alt }, at) { const chain = editor.chain().focus(); (at == null ? chain.insertContent({ type: 'image', attrs: { src, file, alt } }) : chain.insertContentAt(at, { type: 'image', attrs: { src, file, alt } })).run() },
    insertDiagram(svg, scene, file) { editor.chain().focus().insertContent({ type: 'image', attrs: { src: svgDataUrl(svg), scene, file, alt: 'diagram' } }).run() },
    /** Replace the diagram at `pos` with a new drawing. */
    updateDiagram(pos, svg, scene, file) {
      const node = editor.state.doc.nodeAt(pos)
      if (!node || node.type.name !== 'image') return false
      editor.view.dispatch(editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, src: svgDataUrl(svg), scene, file }))
      return true
    },
    /** Install the per-section widget renderer; call refreshCoach() when its data changes. */
    setCoach(render) { coachState.render = render; coachState.cache.clear(); editor.view.dispatch(editor.state.tr.setMeta('coach', true)) },
    refreshCoach() { editor.view.dispatch(editor.state.tr.setMeta('coach', true)) },
    /** Highlight the quoted words (inside the section when it exists). True when found. */
    annotate(id, kind, quote, section) {
      const r = section ? find(section) : null
      const { state, view } = editor
      const range = findText(state, quote, r ? r.from : 0, r ? r.to : state.doc.content.size) || (r && findText(state, quote, 0, state.doc.content.size))
      if (!range) return false
      view.dispatch(state.tr.addMark(range.from, range.to, state.schema.marks.comment.create({ id, kind })).setMeta('addToHistory', false))
      return true
    },
    unannotate(id) {
      const { state, view } = editor
      const tr = state.tr
      state.doc.descendants((node, pos) => {
        if (!node.isText) return
        for (const m of node.marks) if (m.type.name === 'comment' && m.attrs.id === id) tr.removeMark(pos, pos + node.nodeSize, m)
      })
      if (tr.steps.length) view.dispatch(tr.setMeta('addToHistory', false))
    },
    /** Where each comment mark starts: id -> {top (viewport px), pos}. */
    anchors() {
      const out = {}
      editor.state.doc.descendants((node, pos) => {
        if (!node.isText) return
        for (const m of node.marks) if (m.type.name === 'comment' && !(m.attrs.id in out)) out[m.attrs.id] = { pos, top: editor.view.coordsAtPos(pos).top }
      })
      return out
    },
    headingTop(title) { const r = find(title); return r ? editor.view.coordsAtPos(r.headingPos + 1).top : null },
    goTo(id) {
      const a = this.anchors()[id]
      if (!a) return false
      editor.chain().focus().setTextSelection(a.pos).run()
      const dom = editor.view.domAtPos(a.pos).node
      ;(dom.nodeType === 1 ? dom : dom.parentElement)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
      return true
    },
    /** Replace the highlighted words of a comment with new text (accepting a suggestion). */
    replaceAnnotated(id, text) {
      const { state, view } = editor
      let range = null
      state.doc.descendants((node, pos) => {
        if (!node.isText || range && range.to !== pos) return
        if (node.marks.some(m => m.type.name === 'comment' && m.attrs.id === id)) range = range ? { from: range.from, to: pos + node.nodeSize } : { from: pos, to: pos + node.nodeSize }
      })
      if (!range) return false
      const tr = state.tr.insertText(text, range.from, range.to)
      const mark = state.schema.marks.comment
      tr.removeMark(range.from, range.from + text.length, mark)
      view.dispatch(tr.scrollIntoView())
      return true
    },
    /** The comment whose words the caret is in, if any. */
    commentAtCursor() { const { $from, empty } = editor.state.selection; if (!empty) return null; const m = [...$from.marks(), ...($from.nodeAfter ? $from.nodeAfter.marks : [])].find(x => x.type.name === 'comment'); return m ? m.attrs.id : null },
    setActiveComment(id) { element.querySelectorAll('mark[data-comment]').forEach(m => m.classList.toggle('active', m.dataset.comment === id)) },
    hoverAt: (x, y) => { const b = grab.locate(editor.view, x, y); if (b) grab.show(b); return b },
    blocks: { move: dir => moveBlock(editor, dir), duplicate: () => duplicateBlock(editor), remove: () => deleteBlock(editor), select: () => selectBlock(editor), turn: title => applyBlock(editor, BLOCKS.find(b => b.title === title)) },
    json: () => editor.getJSON(),
    markdown: () => editor.markdown.serialize(editor.getJSON()).trim(),
    isEmpty: () => countWords(editor.state.doc.textContent) === 0,
    setSkeleton: titles => editor.commands.setContent(skeleton(titles)),
    sections: () => ranges().map(r => ({ title: r.title, text: md(r.nodes), words: countWords(editor.state.doc.textBetween(r.from, r.to, ' ')), from: r.headingPos, to: r.to })),
    current: () => {
      const at = editor.state.selection.from
      return ranges().filter(r => r.headingPos < at).pop()?.title || null
    },
    sectionMarkdown: title => { const r = find(title); return r ? md(r.nodes) : '' },
    /** Replace or extend a section with Markdown. Creates the section if it does not exist. */
    setSection(title, markdown, how) {
      const r = find(title)
      const body = markdown.trim() ? markdown : null
      const insert = (at, value) => body
        ? editor.chain().insertContentAt(at, value, { contentType: 'markdown' }).run()
        : editor.chain().insertContentAt(at, { type: 'paragraph' }).run()
      if (!r) return insert(editor.state.doc.content.size, `## ${title}\n\n${markdown}`)
      return how === 'replace' ? insert({ from: r.from, to: r.to }, body) : insert(r.to, body)
    },
    /** Put the cursor in the body of the first section (used when leaving the title). */
    focusBody() {
      const r = ranges()[0]
      if (!r) return editor.commands.focus('end')
      if (r.to === r.from) editor.chain().insertContentAt(r.from, { type: 'paragraph' }).run()
      editor.chain().focus().setTextSelection(r.from + 1).run()
    },
    reveal(title) {
      const r = find(title)
      if (!r) return null
      editor.chain().focus().setTextSelection(r.to > r.from ? r.from + 1 : r.from).run()
      const dom = editor.view.nodeDOM(r.headingPos)
      dom?.scrollIntoView({ block: 'center', behavior: 'smooth' })
      return dom
    },
  }
}

window.DocEditor = { create }
