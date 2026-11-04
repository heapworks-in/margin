// Renders help.md into the manual and builds the side index. Small on purpose: headings, lists, paragraphs, inline code and bold.
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const inline = s => esc(s)
  .replace(/`([^`]+)`/g, '<code>$1</code>')
  .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  .replace(/\[([^\]]+)\]\((https?:[^)]+)\)/g, '<a href="$2">$1</a>');
const slug = s => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');

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

async function main() {
  const doc = document.querySelector('.doc'), nav = document.querySelector('.toc');
  try {
    const md = await (await fetch('help.md')).text();
    const { html, toc } = render(md);
    doc.innerHTML = html;
    nav.innerHTML = toc.map(t => `<a href="#${t.id}">${esc(t.text)}</a>`).join('');
    const links = [...nav.querySelectorAll('a')], heads = toc.map(t => document.getElementById(t.id));
    const io = new IntersectionObserver(entries => {
      for (const e of entries) if (e.isIntersecting) { links.forEach(l => l.classList.toggle('on', l.getAttribute('href') === '#' + e.target.id)); }
    }, { rootMargin: '-70px 0px -70% 0px' });
    heads.forEach(h => h && io.observe(h));
  } catch (e) {
    doc.innerHTML = '<p>The manual could not be loaded. It is also in the app under <strong>Help</strong>.</p>';
  }
}
document.querySelectorAll('[data-copy]').forEach(b => b.addEventListener('click', async () => {
  try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = 'Copied'; setTimeout(() => { b.textContent = 'Copy'; }, 1400); } catch {}
}));
main();
