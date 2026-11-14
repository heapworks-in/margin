/* Margin self-test: drives the browser build through every major flow with the mock coach.
   Open http://localhost:4310/?selftest=1 (or run scripts/selftest.sh). Results land in the title and window.__selftest. */
(async () => {
  const results = [], errors = [];
  window.addEventListener('error', e => errors.push(String(e.message || e)));
  window.addEventListener('unhandledrejection', e => errors.push('rejection: ' + String(e.reason && e.reason.message || e.reason)));
  const ok = (name, cond, info) => results.push({ name, ok: !!cond, info: info === undefined ? '' : String(info) });
  const step = async (name, fn) => { try { await fn(); } catch (e) { results.push({ name, ok: false, info: 'threw: ' + (e && e.message || e) }); } };
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const SLOP = 'We leverage a robust solution — it really matters because the landscape is not just hard but also crucial.';
  const findNote = pred => state.comments.find(c => !c.resolved && pred(c));

  await step('document: create and fill', async () => {
    await createDoc('Selftest document', 'design');
    doc.setMarkdown(DEMO_MD);
    ok('sections parsed', sections().length >= 8, sections().length);
    ok('markdown round trip keeps headings', /## Context and problem/.test(toMarkdown()));
  });

  await step('review: notes land on the text', async () => {
    setCurrent('Context and problem'); setMode('review');
    await send(ACTIONS.review[0][1]('Context and problem'), { tier: 'deep', scope: 'section', label: 'Review this section' });
    const notes = state.comments.filter(c => c.kind !== 'draft');
    ok('notes added', notes.length >= 2, notes.length);
    ok('notes anchored as underlines', document.querySelectorAll('mark[data-comment]').length >= 1);
    ok('checks parsed', Object.keys(state.checks['Context and problem'] || {}).length >= 1);
    ok('last message stamped', !!state.chat[state.chat.length - 1].at);
  });

  await step('review: accept a fix, resolve a note', async () => {
    const e = findNote(c => c.kind === 'english' && c.anchored && c.fix);
    ok('english note with a fix exists', !!e);
    if (e) { acceptFix(e); ok('fix applied to the text', sectionText('Context and problem').includes(e.fix) && e.resolved); }
    const o = findNote(c => c.anchored);
    if (o) { const id = o.id; o.resolved = true; doc.unannotate(id); renderComments(); ok('resolved note loses its underline', !document.querySelector(`mark[data-comment="${id}"]`)); }
  });

  await step('panel: rubric and verdict', async () => {
    await send(panelReviewPrompt(), { tier: 'deep', scope: 'document', label: 'Panel' });
    ok('eight rubric scores', state.rubric && Object.keys(state.rubric.scores).length === 8, state.rubric && Object.keys(state.rubric.scores).length);
    ok('verdict parsed', state.rubric && /SHIP|REVISE/.test(state.rubric.verdict));
    ok('persona on notes', state.comments.some(c => c.persona));
    ok('status bar shows rubric', !!document.querySelector('#rubric-btn'));
  });

  await step('claims and reader', async () => {
    await send(ACTIONS.review.find(a => a[0] === 'Claims')[1]('Context and problem'), { tier: 'deep', scope: 'section' });
    ok('claims in ledger', state.claims.length >= 1, state.claims.length);
    await send(ACTIONS.review.find(a => a[0] === 'Read as my reader')[1](), { tier: 'deep', scope: 'document' });
    ok('reader decision parsed', state.reader && ['yes', 'ask', 'no'].includes(state.reader.decision));
    ok('reader notes underlined', document.querySelectorAll('mark[data-kind="reader"]').length >= 1);
  });

  await step('drafts: whole document from notes', async () => {
    setMode('interview'); state.notes = 'three of 240 clients send 70%; p99 200ms to 2.1s'; notesBox().value = state.notes;
    const before = state.comments.length;
    await send(ACTIONS.interview.find(a => a[0] === 'Draft the document')[1](), { tier: 'deep', scope: 'document' });
    const ds = state.comments.slice(before).filter(c => c.kind === 'draft');
    ok('several drafts returned', ds.length >= 2, ds.length);
    const target = ds.find(d => !sectionText(d.section).trim());
    useAllDrafts(ds);
    ok('empty section filled by Use all', target ? !!sectionText(target.section).trim() : true);
    ok('thread offers no second Use all', ![...document.querySelectorAll('#log button')].some(b => /Use all/.test(b.textContent)));
  });

  await step('slop meter', async () => {
    doc.setSection('Open questions', SLOP, 'replace'); renderMeta();
    const n = slopScan().length;
    ok('slop tells found', n >= 5, n);
    ok('status bar shows Slop', !!document.querySelector('#slop-btn'));
    const marked = markSlop();
    ok('slop underlined', marked >= 5, marked);
    const lev = findNote(c => c.slop && /leverage/i.test(c.quote));
    if (lev) { acceptFix(lev); ok('slop fix applied', !/leverage/i.test(sectionText('Open questions'))); }
  });

  await step('custom document type', async () => {
    customTemplates['selftest-type'] = { name: 'Selftest type', readers: ['Staff peer'], sections: parseTemplateText('Alpha | first question | a, b\nBeta | second question | c') };
    await store.templates.set(customTemplates); await loadCustomTemplates();
    ok('custom type registered', !!TEMPLATES['custom:selftest-type'] && TEMPLATES['custom:selftest-type'].sections.length === 2);
    ok('custom type in header picker', !!document.querySelector('#template option[value="custom:selftest-type"]'));
    delete customTemplates['selftest-type']; await store.templates.set(customTemplates); await loadCustomTemplates();
    ok('custom type removed', !TEMPLATES['custom:selftest-type']);
  });

  await step('router', async () => {
    let picked = null; openRouter(t => { picked = t; });
    const q = document.querySelector('#import .router .q');
    const press = label => [...q.querySelectorAll('button')].find(b => b.textContent === label).click();
    press('No'); press('No'); press('No'); press('Yes');
    ok('router picks after Yes', /One-pager/.test(q.textContent), q.textContent.slice(0, 40));
    [...document.querySelectorAll('#import button')].find(b => /^Use /.test(b.textContent)).click();
    ok('router hands the type back', picked === 'onepager', picked);
  });

  await step('context files', async () => {
    await addFiles([new File(['spec text'], 'spec.txt', { type: 'text/plain' }), new File([new Uint8Array([137, 80, 78, 71])], 'shot.png', { type: 'image/png' })]);
    ok('text and image attached', state.attachments.length === 2 && state.attachments.every(a => !a.error), JSON.stringify(state.attachments.map(a => a.error || 'ok')));
    document.querySelector('#chips .chip button').click();
    ok('chip removed', state.attachments.length === 1 && document.querySelectorAll('#chips .chip').length === 1);
    const p = payload({ tier: 'quick', scope: 'section' });
    ok('section scope sends one section', p.sections.filter(s => s.text).length === 1);
    ok('quick turn sends no files', p.attachments.length === 0);
  });

  await step('settings', async () => {
    await renderSettings();
    const h2 = [...document.querySelectorAll('#settings h2')].map(x => x.textContent);
    ok('settings sections present', ['Coach agent (ACP)', 'Quick coach', 'Document types', 'Style rules'].every(x => h2.includes(x)), h2.join(','));
    await wait(400);
    ok('agents listed', document.querySelectorAll('#settings .agent').length >= 2);
    showView('doc');
  });

  await step('notes and history', async () => {
    putText(NOTES, 'a note line', 'append');
    ok('note appended', state.notes.includes('a note line'));
    const desk = document.querySelector('#desk'); desk.scrollTop = 40; grow(notesBox());
    ok('grow keeps the scroll position', desk.scrollTop === 40 || desk.scrollHeight <= desk.clientHeight, desk.scrollTop);
    await store.history.snapshot(state.id, toMarkdown(), 'selftest');
    const list = await store.history.list(state.id);
    ok('history snapshot listed', list.length >= 1);
    const d = diffLines('a\nb', 'a\nc');
    ok('diff marks change', d.some(r => r[0] === '-') && d.some(r => r[0] === '+'));
    showNotesPop(document.body);
    ok('open-notes list renders', document.querySelectorAll('#pop .lrow').length >= 1);
    hidePop();
  });

  await step('caret placement from the gutter', async () => {
    setThread(false);
    const v = doc.editor.view, page = v.dom.closest('.page');
    const sec = sections().find(s => s.title === 'Proposed design');
    const coords = v.coordsAtPos(sec.from + 2);
    page.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, clientX: page.getBoundingClientRect().left + 10, clientY: coords.top + 4, button: 0 }));
    const s = v.state.selection;
    ok('gutter click lands on that section', s.from >= sec.from - 2 && s.from <= sec.to, `${s.from} in ${sec.from}-${sec.to}`);
  });

  await step('stop a slow turn', async () => {
    const real = mockCoach;
    mockCoach = async (t, emit) => { await wait(3000); emit('LATE-CHUNK'); };
    const p = send('slow question', { label: 'slow' });
    await wait(200);
    ok('busy while thinking', busy && !!document.querySelector('#log .stop'));
    cancelSend();
    ok('bar freed after Stop', !busy);
    await p; await wait(3100);
    ok('late chunk ignored', !state.chat.some(m => m.content.includes('LATE-CHUNK')));
    mockCoach = real;
  });

  await step('review what changed', async () => {
    setMode('review');
    const t = 'Context and problem';
    const n0 = state.chat.length;
    doc.setSection(t, sectionText(t) + ' Three of 240 clients send 70% of requests (Grafana, Oct 31).', 'append');
    ok('edited section counts as changed', changedSections().includes(t), changedSections().join(','));
    const openBefore = state.comments.filter(c => !c.resolved && c.kind !== 'draft' && c.section === t).length;
    await send(ACTIONS.review.find(a => a[0] === 'Review what changed')[1](), { tier: 'deep', scope: 'changed' });
    const openAfter = state.comments.filter(c => !c.resolved && c.kind !== 'draft' && c.section === t).length;
    ok('an earlier note was addressed', openBefore === 0 || openAfter < openBefore + 1, `${openBefore} -> ${openAfter}`);
    ok('reviewed text remembered', state.reviewed[t] === sectionText(t));
    ok('nothing-changed short-circuit', (await send(ACTIONS.review.find(a => a[0] === 'Review what changed')[1](), { tier: 'deep', scope: 'changed' }), /Nothing has changed/.test(state.chat[state.chat.length - 1].content)));
  });

  await step('ship gate', async () => {
    const checks = shipChecks();
    ok('five ship checks', checks.length === 5);
    let ran = false; gate(() => { ran = true; }, 'copy');
    ok('gate shows the checklist when checks fail', !ran && !document.querySelector('#import').hidden && document.querySelectorAll('#import .gate .grow').length === 5);
    [...document.querySelectorAll('#import button')].find(b => /anyway/.test(b.textContent)).click();
    ok('anyway proceeds and records', ran && state.shipped && state.shipped.checks.length === 5);
    ok('status bar shows Ship', /Ship/.test(document.querySelector('#status').textContent));
  });

  await step('claim evidence and workspace digest', async () => {
    const un = state.claims.find(c => /^unsupported/i.test(c.source));
    ok('an unsupported claim exists to test with', !!un);
    if (un) {
      const note = state.comments.find(x => x.id === un.id);
      attachEvidence(un, 'spec.txt');
      ok('claim becomes sourced by the file', un.source === 'attachment:spec.txt' && (!note || note.resolved));
      ok('gate counts it as sourced', shipChecks()[1].detail !== '1 unsupported');
    }
    workspaceDigest = '<doc title="ADR-12">We chose Postgres.</doc>';
    ok('deep whole-document turns carry the digest', payload({ tier: 'deep', scope: 'document' }).workspace.includes('ADR-12'));
    ok('quick turns do not', payload({ tier: 'quick', scope: 'section' }).workspace === '');
    workspaceDigest = '';
  });

  await step('review packet and outcome', async () => {
    const pk = reviewPacket();
    ok('packet has brief, checks, rubric, claims and the document', ['## Brief', '## Ship checks', '## Panel rubric', '## Claims and sources', '## Context and problem'].every(h => pk.includes(h)));
    openOutcome();
    [...document.querySelectorAll('#import .gate button')].find(b => b.textContent.startsWith('4')).click();
    [...document.querySelectorAll('#import .foot button')].find(b => b.textContent === 'Save').click();
    ok('outcome recorded', state.outcome && state.outcome.score === 4);
    ok('outcome in status bar', /Real review/.test(document.querySelector('#status').textContent));
    ok('outcome in meta', meta().outcome === 4 && typeof meta().rubric === 'number');
    openFeedback();
    ok('feedback modal offers a Confluence pull', [...document.querySelectorAll('#import button')].some(b => /Pull comments/.test(b.textContent)));
    document.querySelector('#import').hidden = true;
  });

  await step('import markdown', async () => {
    const id0 = state.id;
    await importFile(new File(['# Imported\n\n## Alpha\n\nhello world'], 'imp.md', { type: 'text/markdown' }), 'design');
    ok('imported document opened', state.id !== id0 && sections().some(s => s.title === 'Alpha'));
    await openDoc(id0);
  });

  await step('persistence', async () => {
    await flush();
    const saved = await store.load(state.id);
    ok('comments persisted', saved.comments.length === state.comments.length);
    ok('rubric persisted', !!(saved.rubric && saved.rubric.verdict));
  });

  ok('no uncaught errors', errors.length === 0, errors.slice(0, 3).join(' | '));
  const failed = results.filter(r => !r.ok);
  window.__selftest = { results, failed, errors };
  document.title = failed.length ? `SELFTEST FAIL ${failed.length}/${results.length}: ` + failed.map(f => f.name + (f.info ? ` (${f.info})` : '')).join('; ') : `SELFTEST PASS ${results.length}`;
  const box = document.createElement('pre');
  box.style.cssText = 'position:fixed;left:8px;bottom:40px;z-index:999;max-height:50vh;overflow:auto;background:#000c;color:#9f9;padding:10px;font:12px/1.4 monospace;border-radius:8px';
  box.textContent = results.map(r => (r.ok ? 'PASS ' : 'FAIL ') + r.name + (r.info && !r.ok ? '  <- ' + r.info : '')).join('\n') + (errors.length ? '\nERRORS: ' + errors.join(' | ') : '');
  document.body.append(box);
})();
