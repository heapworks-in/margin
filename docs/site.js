// The site: a carousel of real screenshots, a playable tour, keycaps, and the manual rendered from help.md.
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const inline = s => esc(s)
  .replace(/`([^`]+)`/g, '<code>$1</code>')
  .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  .replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2">$1</a>');
const slug = s => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

/* ---------- hero: real screenshots, rotating ---------- */
const SHOTS = [
  ['review', 'Notes land on your text. Click an underline; the fix is one click away.'],
  ['reader', 'Read as my reader: what your actual reader thinks, line by line, and what they decide.'],
  ['panel', 'Review document: three reviewers, eight scores, SHIP or REVISE.'],
  ['rehearse', 'Rehearse: the panel asks, you answer, every answer is graded.'],
  ['thesis', 'Thesis: the bottom line in 25 words, before you write anything long.'],
  ['diagram', 'Draw: select a paragraph, get an editable Excalidraw diagram.'],
  ['library', 'A workspace is a folder: documents, diagrams, shared files.'],
  ['settings', 'Coaches found on your PATH: Claude Code, Devin, Codex, Gemini, Goose.'],
];
function carousel() {
  const root = document.querySelector('.shots'); if (!root) return;
  root.innerHTML = SHOTS.map(([k, cap], i) => `<figure class="${i ? '' : 'on'}" data-k="${k}"><img src="shots/${k}.png" alt="${esc(cap)}" ${i ? 'loading="lazy"' : ''} width="1440" height="900"><figcaption>${esc(cap)}</figcaption></figure>`).join('')
    + `<div class="dots">${SHOTS.map((s, i) => `<button type="button" aria-label="Screenshot ${i + 1}" class="${i ? '' : 'on'}"></button>`).join('')}</div>`;
  const figs = [...root.querySelectorAll('figure')], dots = [...root.querySelectorAll('.dots button')];
  let i = 0, timer = null;
  const show = n => { i = (n + figs.length) % figs.length; figs.forEach((f, k) => f.classList.toggle('on', k === i)); dots.forEach((d, k) => d.classList.toggle('on', k === i)); };
  const start = () => { if (still) return; stop(); timer = setInterval(() => show(i + 1), 4200); };
  const stop = () => { clearInterval(timer); timer = null; };
  dots.forEach((d, k) => d.onclick = () => { show(k); start(); });
  root.addEventListener('mouseenter', stop); root.addEventListener('mouseleave', start);
  root.addEventListener('focusin', stop); root.addEventListener('focusout', start);
  start();
}

/* ---------- the tour: one document, step by step, on the real screens ---------- */
const TOUR = [
  { t: 'Create the document', shot: 'library', press: ['Which type?', 'Create'], say: 'Type a title. If you are not sure which document this is, <b>Which type?</b> asks a few yes/no questions about the reader’s next action and picks for you.' },
  { t: 'Fill the Brief', shot: 'brief', press: ['Brief ▾'], say: 'Who reads it, what they should do after, the ask in one sentence, <em>their likely objection</em>, the deadline, the numbers you already have. The coach judges everything against this.' },
  { t: 'Get the thesis', shot: 'thesis', press: ['Interview', 'Thesis'], say: 'The bottom line in 25 words, the ask, three claims with facts, the strongest counterargument. If it cannot be written without inventing a fact, the coach says THESIS BLOCKED and asks you the questions first.' },
  { t: 'Review a section', shot: 'review', press: ['Review', 'Review this section'], say: 'Notes arrive as underlines, coloured by kind. Click one: English notes carry a fix and an <b>Accept</b> button. The badge by the heading shows the section’s checks. Every quote was verified against your text first.' },
  { t: 'Draw the flow', shot: 'diagram', press: ['select text', 'Draw'], say: 'Select the paragraph that describes the flow and press <b>Draw</b> in the toolbar. The coach returns boxes and arrows from what the text says, nothing more; click the diagram to edit it in Excalidraw.' },
  { t: 'Convene the panel', shot: 'panel', press: ['Review document'], say: 'Three reviewers chosen for the document type. Each note names who raised it. Then eight rubric scores with one line of evidence each, and a verdict: <b>SHIP</b> or <b>REVISE</b>, in the status bar.' },
  { t: 'Rehearse the meeting', shot: 'rehearse', press: ['Rehearse', 'Start the meeting'], say: 'The panel asks its hardest question. Answer as you would out loud. Each answer is graded 1 to 5; a good answer the document does not yet say becomes a draft. <b>How did it go?</b> gives the verdict.' },
  { t: 'Ship it, then learn', shot: 'export', press: ['Export', 'Publish to Confluence…'], say: 'Copy as Markdown, save a file, or publish to Confluence. After the real review, paste the comments you got into <b>Learn from feedback</b>: the coach recalibrates to your reviewers for next time.' },
];
function tour() {
  const root = document.querySelector('.tour'); if (!root) return;
  root.innerHTML = `<ol class="steps">${TOUR.map((s, i) => `<li><button type="button" class="${i ? '' : 'on'}"><span class="n">${i + 1}</span><span class="t">${esc(s.t)}</span></button></li>`).join('')}</ol>
    <div class="stage"><div class="frame"><img alt="" width="1440" height="900"></div>
    <div class="bar"><div class="press"></div><p class="say"></p><div class="nav"><button type="button" class="prev" aria-label="Previous step">←</button><span class="count"></span><button type="button" class="next" aria-label="Next step">→</button></div></div></div>`;
  const btns = [...root.querySelectorAll('.steps button')], img = root.querySelector('img'), press = root.querySelector('.press'), say = root.querySelector('.say'), count = root.querySelector('.count');
  let i = 0;
  const go = n => {
    i = (n + TOUR.length) % TOUR.length; const s = TOUR[i];
    btns.forEach((b, k) => b.classList.toggle('on', k === i));
    img.src = `shots/${s.shot}.png`; img.alt = s.t;
    press.innerHTML = 'Press ' + s.press.map(p => `<kbd class="chip">${esc(p)}</kbd>`).join('<span class="then">then</span>');
    say.innerHTML = s.say; count.textContent = `${i + 1} / ${TOUR.length}`;
  };
  btns.forEach((b, k) => b.onclick = () => go(k));
  root.querySelector('.prev').onclick = () => go(i - 1); root.querySelector('.next').onclick = () => go(i + 1);
  root.tabIndex = 0;
  root.addEventListener('keydown', e => { if (e.key === 'ArrowRight') { go(i + 1); e.preventDefault(); } if (e.key === 'ArrowLeft') { go(i - 1); e.preventDefault(); } });
  TOUR.forEach(s => { const im = new Image(); im.src = `shots/${s.shot}.png`; });
  go(0);
}

/* ---------- the manual ---------- */
function render(md) {
  const out = [], toc = [];
  for (const block of md.trim().split(/\n{2,}/)) {
    const lines = block.split('\n');
    const h = block.match(/^(#{1,3})\s+(.*)$/);
    if (h && lines.length === 1) {
      const lvl = h[1].length, text = h[2].trim(), id = slug(text);
      if (lvl === 2) toc.push({ id, text: text.replace(/^\d+\.\s*/, '') });
      out.push(`<h${lvl} id="${id}">${inline(text)}</h${lvl}>`);
    } else if (lines.every(l => /^\s*([-*]|\d+\.)\s/.test(l))) {
      const tag = /^\s*\d+\./.test(lines[0]) ? 'ol' : 'ul';
      out.push(`<${tag}>${lines.map(l => '<li>' + inline(l.replace(/^\s*([-*]|\d+\.)\s/, '')) + '</li>').join('')}</${tag}>`);
    } else {
      out.push('<p>' + inline(block).replace(/\n/g, ' ') + '</p>');
    }
  }
  return { html: out.join('\n'), toc };
}
async function manual() {
  const doc = document.querySelector('.doc'), nav = document.querySelector('.toc');
  try {
    const md = await (await fetch('help.md')).text();
    const { html, toc } = render(md);
    doc.innerHTML = html;
    nav.innerHTML = toc.map(t => `<a href="#${t.id}">${esc(t.text)}</a>`).join('');
    const links = [...nav.querySelectorAll('a')], heads = toc.map(t => document.getElementById(t.id));
    const io = new IntersectionObserver(entries => {
      for (const e of entries) if (e.isIntersecting) links.forEach(l => l.classList.toggle('on', l.getAttribute('href') === '#' + e.target.id));
    }, { rootMargin: '-70px 0px -70% 0px' });
    heads.forEach(h => h && io.observe(h));
  } catch (e) {
    doc.innerHTML = '<p>The manual could not be loaded. It is also in the app under <strong>Help</strong>.</p>';
  }
}

document.querySelectorAll('[data-copy]').forEach(b => b.addEventListener('click', async () => {
  try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = 'Copied'; setTimeout(() => { b.textContent = 'Copy'; }, 1400); } catch {}
}));
carousel(); tour(); manual();
