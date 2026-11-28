// The gateway's own page script (specs/010 research.md R5). It runs in an
// isolated world the page's scripts can't see or change, and is the only
// script ever evaluated in the agent's browser: the agent supplies none.
// It never returns a field's value (FR-012).
(function (op, args) {
  const g = globalThis;
  if (!g.__seshEls) g.__seshEls = [];
  const MAX_TEXT = 6000, MAX_ELEMENTS = 150;

  function visible(el) {
    const r = el.getBoundingClientRect();
    if (r.width < 1 || r.height < 1) return false;
    const s = getComputedStyle(el);
    return s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0.05;
  }
  function clean(s, n) {
    s = (s || '').replace(/\s+/g, ' ').trim();
    return s.length > n ? s.slice(0, n) + '…' : s;
  }
  function label(el) {
    const aria = el.getAttribute('aria-label');
    if (aria) return clean(aria, 80);
    if (el.id) {
      const l = document.querySelector('label[for="' + CSS.escape(el.id) + '"]');
      if (l) return clean(l.innerText, 80);
    }
    const wrap = el.closest('label');
    if (wrap) return clean(wrap.innerText, 80);
    if (el.tagName === 'INPUT' && ['submit', 'button', 'reset'].includes(el.type)) return clean(el.value, 80);
    return clean(el.innerText || el.getAttribute('placeholder') || el.getAttribute('name') || el.getAttribute('title') || el.getAttribute('alt') || '', 80);
  }
  function kind(el) {
    const t = el.tagName;
    if (t === 'A') return 'link';
    if (t === 'SELECT') return 'choice';
    if (t === 'TEXTAREA') return 'text field';
    if (t === 'INPUT') {
      const ty = (el.type || 'text').toLowerCase();
      if (ty === 'password') return 'password field';
      if (ty === 'file') return 'file field';
      if (ty === 'checkbox') return 'checkbox';
      if (ty === 'radio') return 'option';
      if (['submit', 'image'].includes(ty)) return 'submit button';
      if (['button', 'reset'].includes(ty)) return 'button';
      return 'text field';
    }
    if (t === 'BUTTON') return submits(el) ? 'submit button' : 'button';
    return 'button';
  }
  // A click that submits a form needs the person's approval (FR-007).
  function submits(el) {
    if (el.tagName === 'BUTTON') {
      const ty = (el.getAttribute('type') || 'submit').toLowerCase();
      return ty === 'submit' && !!(el.form || el.closest('form'));
    }
    if (el.tagName === 'INPUT') return ['submit', 'image'].includes((el.type || '').toLowerCase());
    return false;
  }
  function interactive() {
    const sel = 'a[href], button, input:not([type=hidden]), textarea, select, [role=button], [role=link], summary';
    return Array.from(document.querySelectorAll(sel)).filter(e => !e.disabled && visible(e)).slice(0, MAX_ELEMENTS);
  }
  function el(n) {
    const e = g.__seshEls[n - 1];
    if (!e || !e.isConnected) return null;
    return e;
  }
  function describe(e) {
    const k = kind(e);
    const d = { kind: k, label: label(e) };
    if (k.endsWith('field') || k === 'choice') d.filled = e.tagName === 'SELECT' ? e.selectedIndex > 0 : !!(e.value || (e.files && e.files.length));
    if (k === 'checkbox' || k === 'option') d.checked = !!e.checked;
    if (k === 'link') {
      try { d.to = new URL(e.href, location.href).host; } catch (_) {}
    }
    return d;
  }

  function snapshot() {
    g.__seshEls = interactive();
    const body = document.body ? document.body.innerText : '';
    return {
      url: location.href,
      title: clean(document.title, 200),
      text: clean(body, MAX_TEXT),
      elements: g.__seshEls.map((e, i) => Object.assign({ n: i + 1 }, describe(e))),
    };
  }

  function target(n) {
    const e = el(n);
    if (!e) return { error: 'element ' + n + " isn't on the page any more; take a new snapshot" };
    e.scrollIntoView({ block: 'center', inline: 'center' });
    const r = e.getBoundingClientRect();
    return Object.assign(describe(e), {
      submits: submits(e),
      x: r.left + r.width / 2,
      y: r.top + r.height / 2,
      host: location.host,
    });
  }

  function focus(n) {
    const e = el(n);
    if (!e) return { error: 'element ' + n + " isn't on the page any more; take a new snapshot" };
    const k = kind(e);
    if (!k.endsWith('field') || k === 'file field') return { error: 'element ' + n + " isn't a text field" };
    e.focus();
    if (typeof e.select === 'function') e.select();
    return { ok: true };
  }

  // The fields `login(site)` fills, by element number (never values).
  function loginFields() {
    g.__seshEls = interactive();
    const els = g.__seshEls;
    const idx = e => els.indexOf(e) + 1;
    const pass = els.find(e => e.tagName === 'INPUT' && e.type === 'password');
    const otp = els.find(e => e.tagName === 'INPUT' && (e.autocomplete === 'one-time-code' || /otp|totp|2fa|mfa|one.?time|verification|code/i.test((e.name || '') + ' ' + (e.id || ''))) && e.type !== 'password');
    let user = null;
    if (pass) {
      const scope = pass.form || document;
      const cands = Array.from(scope.querySelectorAll('input')).filter(e => visible(e) && ['text', 'email', 'tel', ''].includes((e.getAttribute('type') || '').toLowerCase()) && e !== otp);
      const before = cands.filter(e => e.compareDocumentPosition(pass) & Node.DOCUMENT_POSITION_FOLLOWING);
      user = before.length ? before[before.length - 1] : cands[0] || null;
      if (user && els.indexOf(user) < 0) { els.push(user); }
    }
    const submit = pass && pass.form ? Array.from(pass.form.querySelectorAll('button, input[type=submit]')).find(e => submits(e) && visible(e)) : null;
    if (submit && els.indexOf(submit) < 0) els.push(submit);
    return {
      host: location.host,
      url: location.href,
      user: user ? idx(user) : 0,
      pass: pass ? idx(pass) : 0,
      otp: otp ? idx(otp) : 0,
      submit: submit ? idx(submit) : 0,
    };
  }

  // Readable text of the main content as Markdown (specs/010 R5).
  function extract(maxBytes) {
    const skip = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'NAV', 'FOOTER', 'HEADER', 'ASIDE', 'FORM', 'SVG', 'IFRAME']);
    let root = document.querySelector('article') || document.querySelector('main') || document.querySelector('[role=main]');
    if (!root) {
      let best = document.body, score = 0;
      document.querySelectorAll('div, section').forEach(d => {
        const t = d.innerText || '';
        const ps = d.querySelectorAll('p').length;
        const s = t.length * (1 + ps);
        if (s > score && t.length < (document.body.innerText || '').length * 0.95) { best = d; score = s; }
      });
      root = best || document.body;
    }
    const out = [];
    function walk(n) {
      if (n.nodeType === 3) { out.push(n.textContent.replace(/\s+/g, ' ')); return; }
      if (n.nodeType !== 1 || skip.has(n.tagName) || !visible(n)) return;
      const t = n.tagName;
      const h = /^H([1-6])$/.exec(t);
      if (h) { out.push('\n\n' + '#'.repeat(+h[1]) + ' ' + clean(n.innerText, 300) + '\n\n'); return; }
      if (t === 'A') {
        const txt = clean(n.innerText, 200);
        if (txt) out.push('[' + txt + '](' + n.href + ')');
        return;
      }
      if (t === 'LI') out.push('\n- ');
      if (t === 'BR') out.push('\n');
      n.childNodes.forEach(walk);
      if (['P', 'DIV', 'SECTION', 'UL', 'OL', 'TABLE', 'TR', 'BLOCKQUOTE', 'PRE'].includes(t)) out.push('\n\n');
    }
    walk(root);
    let md = out.join('').replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
    let truncated = false;
    const enc = new TextEncoder();
    if (enc.encode(md).length > maxBytes) {
      while (enc.encode(md).length > maxBytes) md = md.slice(0, Math.floor(md.length * 0.9));
      truncated = true;
    }
    return { url: location.href, title: clean(document.title, 200), markdown: md, truncated };
  }

  // Puts a file into a file field (specs/010 R9); the bytes come from the
  // gateway, after the person approved the upload.
  function setFile(n, name, type, b64) {
    const e = el(n);
    if (!e || e.tagName !== 'INPUT' || e.type !== 'file') return { error: 'element ' + n + " isn't a file field" };
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const dt = new DataTransfer();
    dt.items.add(new File([bytes], name, { type }));
    e.files = dt.files;
    e.dispatchEvent(new Event('input', { bubbles: true }));
    e.dispatchEvent(new Event('change', { bubbles: true }));
    return { ok: true };
  }

  switch (op) {
    case 'ready': return { state: document.readyState, url: location.href };
    case 'snapshot': return snapshot();
    case 'target': return target(args.n);
    case 'focus': return focus(args.n);
    case 'login_fields': return loginFields();
    case 'extract': return extract(args.max_bytes);
    case 'set_file': return setFile(args.n, args.name, args.type, args.b64);
    default: return { error: 'unknown op' };
  }
})
