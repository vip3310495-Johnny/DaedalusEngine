/* =====================================================================
 * 電路模擬器 — Simulation engine core
 * Original author: Johnny.ch.Tsai
 * MNA transient solver (trapezoidal + backward-Euler damping),
 * PWL switch iteration with switching-instant interpolation,
 * breakpoint-aligned sub-stepping, control-block scheduler, AC analysis.
 * This file has no DOM dependency: it runs in a Web Worker, the main
 * thread, or Node.js.
 * ===================================================================== */
'use strict';
var SIM = (typeof SIM !== 'undefined') ? SIM : {};
(function (S) {

  /* ------------------------------------------------------------------
   * 1. Numbers and expressions
   * ------------------------------------------------------------------ */
  const SUFFIX = { G: 1e9, M: 1e6, k: 1e3, K: 1e3, m: 1e-3, u: 1e-6, 'µ': 1e-6, 'μ': 1e-6, n: 1e-9, p: 1e-12, f: 1e-15 };
  const FUNCS = {
    sqrt: 'Math.sqrt', sin: 'Math.sin', cos: 'Math.cos', tan: 'Math.tan', asin: 'Math.asin', acos: 'Math.acos',
    atan: 'Math.atan', atan2: 'Math.atan2', exp: 'Math.exp', log: 'Math.log', ln: 'Math.log', log10: 'Math.log10',
    abs: 'Math.abs', sign: '_sign', floor: 'Math.floor', ceil: 'Math.ceil', round: 'Math.round', int: 'Math.trunc',
    trunc: 'Math.trunc', min: 'Math.min', max: 'Math.max', pow: 'Math.pow', sinh: 'Math.sinh', cosh: 'Math.cosh',
    tanh: 'Math.tanh', mod: '_mod', fmod: '_mod', sat: '_sat', step: '_step'
  };
  const HELPERS = 'const _sign=x=>x>0?1:(x<0?-1:0);const _mod=(a,b)=>a%b;const _sat=(x,lo,hi)=>x<lo?lo:(x>hi?hi:x);const _step=x=>x>=0?1:0;';

  class ExprError extends Error { }
  S.ExprError = ExprError;

  function tokenize(src) {
    const toks = []; let i = 0; const s = String(src);
    while (i < s.length) {
      const c = s[i];
      if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue; }
      const sp = i > 0 && /\s/.test(s[i - 1]);
      const rest = s.slice(i);
      let m = /^(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?/.exec(rest);
      if (m) {
        let v = parseFloat(m[0]); i += m[0].length;
        const r2 = s.slice(i);
        const mm = /^meg/i.exec(r2);
        if (mm) { v *= 1e6; i += 3; }
        else if (r2.length && SUFFIX[r2[0]] !== undefined) { v *= SUFFIX[r2[0]]; i++; }
        const um = /^[A-Za-zΩ]+/.exec(s.slice(i));
        if (um) i += um[0].length;
        toks.push({ t: 'num', v, sp, spA: /\s/.test(s[i] || '') });
        continue;
      }
      m = /^[A-Za-z_][A-Za-z0-9_]*/.exec(rest);
      if (m) { toks.push({ t: 'id', v: m[0], sp }); i += m[0].length; continue; }
      const two = rest.slice(0, 2);
      if (['<=', '>=', '==', '!=', '&&', '||', '**'].includes(two)) { toks.push({ t: 'op', v: two, sp, spA: /\s/.test(s[i + 2] || '') }); i += 2; continue; }
      if ('+-*/^(),<>!?:'.includes(c)) { toks.push({ t: 'op', v: c, sp, spA: /\s/.test(s[i + 1] || '') }); i++; continue; }
      throw new ExprError('無法辨識的字元 "' + c + '"');
    }
    return toks;
  }

  // Parser producing JS source. resolve(nameLower) -> js string or number, or null.
  function Parser(toks, resolve, listMode) { this.t = toks; this.i = 0; this.res = resolve; this.list = listMode; }
  Parser.prototype.peek = function () { return this.t[this.i]; };
  Parser.prototype.next = function () { return this.t[this.i++]; };
  Parser.prototype.isOp = function (v) { const k = this.t[this.i]; return k && k.t === 'op' && k.v === v; };
  Parser.prototype.expect = function (v) { if (!this.isOp(v)) throw new ExprError('缺少 "' + v + '"'); this.i++; };
  Parser.prototype.parseExpr = function () { return this.parseTernary(); };
  Parser.prototype.parseTernary = function () {
    const c = this.parseOr();
    if (this.isOp('?')) { this.i++; const a = this.parseTernary(); this.expect(':'); const b = this.parseTernary(); return '((' + c + ')?(' + a + '):(' + b + '))'; }
    return c;
  };
  Parser.prototype.parseOr = function () { let a = this.parseAnd(); while (this.isOp('||')) { this.i++; const b = this.parseAnd(); a = '((' + a + ')||(' + b + ')?1:0)'; } return a; };
  Parser.prototype.parseAnd = function () { let a = this.parseCmp(); while (this.isOp('&&')) { this.i++; const b = this.parseCmp(); a = '((' + a + ')&&(' + b + ')?1:0)'; } return a; };
  Parser.prototype.parseCmp = function () {
    let a = this.parseAdd();
    for (;;) {
      const k = this.peek();
      if (k && k.t === 'op' && ['<', '>', '<=', '>=', '==', '!='].includes(k.v)) { this.i++; const b = this.parseAdd(); const o = k.v === '==' ? '===' : (k.v === '!=' ? '!==' : k.v); a = '((' + a + ')' + o + '(' + b + ')?1:0)'; }
      else return a;
    }
  };
  Parser.prototype.parseAdd = function () {
    let a = this.parseMul();
    for (;;) {
      const k = this.peek();
      if (k && k.t === 'op' && (k.v === '+' || k.v === '-')) {
        if (this.list && k.sp && !k.spA) return a; // "1 -2" in a list => two items
        this.i++; const b = this.parseMul(); a = '(' + a + k.v + b + ')';
      } else return a;
    }
  };
  Parser.prototype.parseMul = function () {
    let a = this.parseUnary();
    for (;;) {
      const k = this.peek();
      if (k && k.t === 'op' && (k.v === '*' || k.v === '/')) { this.i++; const b = this.parseUnary(); a = '(' + a + k.v + b + ')'; }
      else return a;
    }
  };
  Parser.prototype.parseUnary = function () {
    const k = this.peek();
    if (k && k.t === 'op' && (k.v === '-' || k.v === '+')) { this.i++; const a = this.parseUnary(); return k.v === '-' ? '(-' + a + ')' : a; }
    if (k && k.t === 'op' && k.v === '!') { this.i++; const a = this.parseUnary(); return '((' + a + ')?0:1)'; }
    return this.parsePow();
  };
  Parser.prototype.parsePow = function () {
    const a = this.parsePrimary();
    if (this.isOp('^') || this.isOp('**')) { this.i++; const b = this.parseUnary(); return 'Math.pow(' + a + ',' + b + ')'; }
    return a;
  };
  Parser.prototype.parsePrimary = function () {
    const k = this.next();
    if (!k) throw new ExprError('運算式不完整');
    if (k.t === 'num') return '(' + numStr(k.v) + ')';
    if (k.t === 'op' && k.v === '(') { const a = this.parseExpr(); this.expect(')'); return '(' + a + ')'; }
    if (k.t === 'id') {
      const name = k.v.toLowerCase();
      if (this.isOp('(')) {
        this.i++;
        const f = FUNCS[name]; if (!f) throw new ExprError('不支援的函數 ' + k.v + '()');
        const args = [];
        if (!this.isOp(')')) { args.push(this.parseExpr()); while (this.isOp(',')) { this.i++; args.push(this.parseExpr()); } }
        this.expect(')');
        return f + '(' + args.join(',') + ')';
      }
      if (name === 'pi') return '(Math.PI)';
      const r = this.res(name);
      if (r === null || r === undefined) throw new ExprError('未定義的名稱 "' + k.v + '"');
      return typeof r === 'number' ? '(' + numStr(r) + ')' : r;
    }
    throw new ExprError('語法錯誤，靠近 "' + k.v + '"');
  };
  function numStr(v) { if (!isFinite(v)) return v > 0 ? 'Infinity' : (v < 0 ? '-Infinity' : 'NaN'); const s = String(v); return s; }

  function compileToJs(src, resolve) {
    const toks = tokenize(src);
    if (!toks.length) throw new ExprError('空白運算式');
    const p = new Parser(toks, resolve, false);
    const js = p.parseExpr();
    if (p.i < toks.length) throw new ExprError('多餘的內容，靠近 "' + toks[p.i].v + '"');
    return js;
  }

  // Evaluate a constant expression with a global symbol table {lowername: value}.
  function evalConst(src, syms) {
    const s = String(src == null ? '' : src).trim();
    if (s === '') throw new ExprError('數值不可空白');
    if (/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(s)) return parseFloat(s);
    const js = compileToJs(s, n => (syms && Object.prototype.hasOwnProperty.call(syms, n)) ? syms[n] : null);
    const v = Function(HELPERS + 'return (' + js + ');')();
    if (typeof v !== 'number' || Number.isNaN(v)) throw new ExprError('結果不是有效數值');
    return v;
  }
  S.evalConst = evalConst;

  function evalList(src, syms) {
    const s = String(src == null ? '' : src).trim();
    if (s === '') return [];
    const toks = tokenize(s);
    const out = [];
    const p = new Parser(toks, n => (syms && Object.prototype.hasOwnProperty.call(syms, n)) ? syms[n] : null, true);
    while (p.i < toks.length) {
      if (p.isOp(',') || p.isOp(';')) { p.i++; continue; }
      const js = p.parseExpr();
      out.push(Function(HELPERS + 'return (' + js + ');')());
    }
    return out;
  }
  S.evalList = evalList;

  // "(t1,v1) (t2,v2) ..." -> [[t,v],...]
  function evalPairs(src, syms) {
    const s = String(src || '');
    const re = /\(([^()]*)\)/g; let m; const out = [];
    while ((m = re.exec(s))) {
      const parts = evalList(m[1], syms);
      if (parts.length !== 2) throw new ExprError('每一組需要 (時間, 數值)');
      out.push(parts);
    }
    if (!out.length) throw new ExprError('至少需要一組 (時間, 數值)');
    return out;
  }
  S.evalPairs = evalPairs;

  // Compile f(args) where args are variable names (case-insensitive). syms: constants.
  function compileFn(src, argNames, syms) {
    const map = {};
    argNames.forEach((a, i) => { map[a.toLowerCase()] = '_a' + i; });
    const js = compileToJs(src, n => {
      if (map[n]) return map[n];
      if (syms && Object.prototype.hasOwnProperty.call(syms, n)) return syms[n];
      return null;
    });
    const params = argNames.map((_, i) => '_a' + i).join(',');
    return Function(HELPERS + 'return function(' + params + '){return (' + js + ');};')();
  }
  S.compileFn = compileFn;

  // Parameter file text: "name = expr" lines, % comments.
  function parseParamText(text) {
    const syms = {}; const errors = [];
    String(text || '').split(/\r?\n/).forEach((line, li) => {
      const l = line.replace(/[%#].*$/, '').replace(/\/\/.*$/, '').trim();
      if (!l) return;
      if (/^limit\s/i.test(l)) return;
      const m = /^([A-Za-z_][A-Za-z0-9_]*)\s*(=|\s)\s*(.+)$/.exec(l);
      if (!m) { errors.push('第 ' + (li + 1) + ' 行格式錯誤：' + line); return; }
      try { syms[m[1].toLowerCase()] = evalConst(m[3], syms); }
      catch (e) { errors.push('第 ' + (li + 1) + ' 行：' + e.message); }
    });
    return { syms, errors };
  }
  S.parseParamText = parseParamText;

  /* ------------------------------------------------------------------
   * 2. Complex LU (AC analysis); the real sparse LU lives in sparse_lu.js
   * ------------------------------------------------------------------ */
  // real LU: see sparse_lu.js (S.LU, S.Pattern)

  class CLU {
    constructor(n) { this.n = n; this.re = new Float64Array(n * n); this.im = new Float64Array(n * n); this.p = new Int32Array(n); this.bad = -1; }
    factor(Ar, Ai) {
      const n = this.n, re = this.re, im = this.im, p = this.p; re.set(Ar); im.set(Ai); this.bad = -1;
      const sc = new Float64Array(n);
      for (let i = 0; i < n; i++) { p[i] = i; let m = 0; for (let j = 0; j < n; j++) { const v = Math.hypot(re[i * n + j], im[i * n + j]); if (v > m) m = v; } if (m === 0) { this.bad = i; return false; } sc[i] = 1 / m; }
      for (let k = 0; k < n; k++) {
        let piv = k, best = -1;
        for (let i = k; i < n; i++) { const v = Math.hypot(re[i * n + k], im[i * n + k]) * sc[i]; if (v > best) { best = v; piv = i; } }
        if (!(best > 1e-15)) { this.bad = p[piv]; return false; }
        if (piv !== k) {
          for (let j = 0; j < n; j++) { let t = re[k * n + j]; re[k * n + j] = re[piv * n + j]; re[piv * n + j] = t; t = im[k * n + j]; im[k * n + j] = im[piv * n + j]; im[piv * n + j] = t; }
          let t = sc[k]; sc[k] = sc[piv]; sc[piv] = t; t = p[k]; p[k] = p[piv]; p[piv] = t;
        }
        const pr = re[k * n + k], pi = im[k * n + k], d = pr * pr + pi * pi;
        const ir = pr / d, ii = -pi / d;
        for (let i = k + 1; i < n; i++) {
          const ar = re[i * n + k], ai = im[i * n + k];
          if (ar === 0 && ai === 0) continue;
          const fr = ar * ir - ai * ii, fi = ar * ii + ai * ir;
          re[i * n + k] = fr; im[i * n + k] = fi;
          for (let j = k + 1; j < n; j++) {
            const br = re[k * n + j], bi = im[k * n + j];
            re[i * n + j] -= fr * br - fi * bi; im[i * n + j] -= fr * bi + fi * br;
          }
        }
      }
      return true;
    }
    solve(br, bi, xr, xi) {
      const n = this.n, re = this.re, im = this.im, p = this.p;
      const yr = new Float64Array(n), yi = new Float64Array(n);
      for (let i = 0; i < n; i++) {
        let sr = br[p[i]], si = bi[p[i]];
        for (let j = 0; j < i; j++) { const ar = re[i * n + j], ai = im[i * n + j]; sr -= ar * yr[j] - ai * yi[j]; si -= ar * yi[j] + ai * yr[j]; }
        yr[i] = sr; yi[i] = si;
      }
      for (let i = n - 1; i >= 0; i--) {
        let sr = yr[i], si = yi[i];
        for (let j = i + 1; j < n; j++) { const ar = re[i * n + j], ai = im[i * n + j]; sr -= ar * xr[j] - ai * xi[j]; si -= ar * xi[j] + ai * xr[j]; }
        const dr = re[i * n + i], di = im[i * n + i], d = dr * dr + di * di;
        xr[i] = (sr * dr + si * di) / d; xi[i] = (si * dr - sr * di) / d;
      }
    }
  }
  S.CLU = CLU;

  /* ------------------------------------------------------------------
   * 3. Model registry and element parameter access
   * ------------------------------------------------------------------ */
  S.MODELS = {};
  class SimError extends Error { constructor(msg, elId) { super(msg); this.elId = elId; } }
  S.SimError = SimError;

  // Parameter accessor bound to one element
  function Params(el, syms) { this.el = el; this.p = el.params || {}; this.syms = syms; }
  Params.prototype.raw = function (k, d) { const v = this.p[k]; return (v === undefined || v === null || v === '') ? d : String(v); };
  Params.prototype.num = function (k, d) {
    const v = this.p[k];
    if (v === undefined || v === null || String(v).trim() === '') { if (d !== undefined) return d; throw new SimError(this.el.name + '：參數 ' + k + ' 未設定', this.el.id); }
    try { return evalConst(v, this.syms); } catch (e) { throw new SimError(this.el.name + '：參數 ' + k + ' = "' + v + '" ' + e.message, this.el.id); }
  };
  Params.prototype.int = function (k, d) { return Math.round(this.num(k, d)); };
  Params.prototype.pos = function (k, d) { const v = this.num(k, d); if (!(v > 0)) throw new SimError(this.el.name + '：參數 ' + k + ' 必須大於 0', this.el.id); return v; };
  Params.prototype.list = function (k, d) {
    const v = this.p[k];
    if (v === undefined || String(v).trim() === '') { if (d !== undefined) return d; throw new SimError(this.el.name + '：參數 ' + k + ' 未設定', this.el.id); }
    try { return evalList(v, this.syms); } catch (e) { throw new SimError(this.el.name + '：參數 ' + k + ' ' + e.message, this.el.id); }
  };
  Params.prototype.pairs = function (k) {
    try { return evalPairs(this.p[k], this.syms); } catch (e) { throw new SimError(this.el.name + '：參數 ' + k + ' ' + e.message, this.el.id); }
  };
  Params.prototype.fn = function (k, args) {
    const v = this.p[k];
    if (v === undefined || String(v).trim() === '') throw new SimError(this.el.name + '：運算式 ' + k + ' 未設定', this.el.id);
    try { return compileFn(v, args, this.syms); } catch (e) { throw new SimError(this.el.name + '：運算式 "' + v + '" ' + e.message, this.el.id); }
  };
  S.Params = Params;

  /* ------------------------------------------------------------------
   * 4. Stamping helpers
   * ------------------------------------------------------------------ */
  // A: Float64Array n*n row-major. Indices -1 mean ground and are skipped.
  function aA(M, i, j, v) { if (i >= 0 && j >= 0) { const k = i * M.n + j; M.A[k] += v; if (!M.pm[k]) M.pat.add(k); } }
  function aG(M, a, b, g) { aA(M, a, a, g); aA(M, b, b, g); aA(M, a, b, -g); aA(M, b, a, -g); }
  function aB(b, i, v) { if (i >= 0) b[i] += v; }
  // current source of value I flowing from node a through the element to node b
  function aI(b, a, k, I) { if (a >= 0) b[a] -= I; if (k >= 0) b[k] += I; }
  // branch variable k with current flowing a -> b through the branch (leaving node a)
  function aBr(M, a, b, k) { aA(M, a, k, 1); aA(M, b, k, -1); }
  // branch equation v_a - v_b  (row k)
  function aV(M, k, a, b, c) { aA(M, k, a, c); aA(M, k, b, -c); }
  S.st = { aA, aG, aB, aI, aBr, aV };
  function vx(x, i) { return i >= 0 ? x[i] : 0; }
  S.vx = vx;

  /* ------------------------------------------------------------------
   * 5. Builder context used by model setup
   * ------------------------------------------------------------------ */
  class Builder {
    constructor(sim) { this.sim = sim; }
  }

  /* ------------------------------------------------------------------
   * 6. The simulator
   * ------------------------------------------------------------------ */
  class Sim {
    constructor(net, opts) {
      this.net = net; this.o = Object.assign({ interp: true, damp: true, refine: true, lte: true, lteTol: 1e-3, lteAbsV: 1e-6, lteAbsI: 1e-9, lteMaxDepth: 4, beSplit: true, refineEvt: true }, opts || {});
      this.dt = this.o.dt;
      this.warnings = [];
      this.pEls = []; this.cBlks = []; this.probes = []; this.acProbes = []; this.gated = []; this.swEls = []; this.nlEls = []; this.timedSw = []; this.bpEls = []; this.dynEls = []; this.sources = {};
      this.nNode = 0; this.nVar = 0; this.nC = 1; // control node 0 = ground
      this._preq = []; this._creq = [];
      this.cache = new Map();
      this.build();
    }

    warn(msg) { if (this.warnings.length < 200 && !this.warnings.includes(msg)) this.warnings.push(msg); }

    /* ---------- construction ---------- */
    build() {
      const net = this.net;
      const syms = (net.paramText ? parseParamText(net.paramText).syms : {});
      this.syms = syms;
      // node maps
      this.pnode = new Int32Array(net.nets.length).fill(-1);
      this.cnode = new Int32Array(net.nets.length).fill(-1);
      net.nets.forEach((nt, i) => {
        if (nt.kind === 'P') this.pnode[i] = this.nNode++;
        else if (nt.kind === 'C') this.cnode[i] = this.nC++;
        else if (nt.kind === 'G') { this.pnode[i] = -1; this.cnode[i] = 0; }
      });
      this.nExtNode = this.nNode;
      // device probes name the switch they look into; the switch then adds a terminal ammeter
      this.devs = {}; this.devWant = new Set();
      for (const el of net.elements) if (el.type === 'DEVP') this.devWant.add(String(el.params.el || '').trim());
      // instantiate
      const insts = [];
      for (const el of net.elements) {
        const mk = S.MODELS[el.type];
        if (!mk) continue; // non-simulated element (text, sim control...)
        const P = new Params(el, syms);
        const pins = el.pins.map(ni => ({ net: ni, kind: ni < 0 ? 'N' : net.nets[ni].kind, p: ni < 0 ? -1 : this.pnode[ni], c: ni < 0 ? -1 : this.cnode[ni] }));
        const ctx = new ElCtx(this, el, pins, P);
        let made;
        try { made = mk(ctx); } catch (e) { if (e instanceof SimError) throw e; throw new SimError(el.name + '：' + e.message, el.id); }
        if (!made) continue;
        (Array.isArray(made) ? made : [made]).forEach(m => { m.el = el; insts.push(m); });
      }
      this.insts = insts;
      for (const m of insts) if (m.link) m.link(this);
      for (const m of insts) {
        if (m.setup) m.setup(this);
        if (m.isP) this.pEls.push(m);
        if (m.isC) this.cBlks.push(m);
        if (m.sw) this.swEls.push(m);
        if (m.gn !== undefined && m.gn >= 0) this.gated.push(m);
        if (m.nl) this.nlEls.push(m);
        if (m.timed) this.timedSw.push(m);
        if (m.dyn) this.dynEls.push(m);
        if (m.nextBreak) this.bpEls.push(m);
        if (m.probe) this.probes.push(m);
        if (m.acProbe) this.acProbes.push(m);
        if (m.srcName) this.sources[m.srcName.toLowerCase()] = m;
      }
      this.groundIslands();
      this.n = this.nNode + this.nVar;
      // state buffers
      let np = 0; this._preq.forEach(r => { r.off = np; np += r.len; });
      this.PS = new Float64Array(Math.max(1, np));
      this._preq.forEach(r => { r.obj[r.key] = this.PS.subarray(r.off, r.off + r.len); });
      let nc = 0; this._creq.forEach(r => { r.off = nc; nc += r.len; });
      this.CS = new Float64Array(Math.max(1, nc));
      this._creq.forEach(r => { r.obj[r.key] = this.CS.subarray(r.off, r.off + r.len); });
      // vectors
      const n = this.n;
      this.A = new Float64Array(n * n); this.pat = new S.Pattern(n); this.M = { A: this.A, n, pat: this.pat, pm: this.pat.mask };
      this.b = new Float64Array(n); this.b0 = new Float64Array(n); this.x = new Float64Array(n); this.xP = new Float64Array(n); this.xPP = new Float64Array(n); this.xT = new Float64Array(n); this.r = new Float64Array(n); this.d = new Float64Array(n);
      this.xPPP = new Float64Array(n); this.x4 = new Float64Array(n); this.xScale = new Float64Array(n);
      this.lteW = new Float64Array(n).fill(1); for (const i of this.lteSkip) this.lteW[i] = 0; this.scV = 0; this.scI = 0; this.hP = 0; this.hPP = 0; this.hPPP = 0; this.histN = 0; this.lastBE = true; this.predOk = false; this.linCache = new Map();
      this.cv = new Float64Array(this.nC); this.cvP = new Float64Array(this.nC); this.cev = new Float64Array(this.nC).fill(NaN);
      this.cvec = new Array(this.nC).fill(null);
      this.orderControl();
      this.hasNL = this.nlEls.length > 0;
      this.hasDyn = this.dynEls.length > 0;
      this.swVer = 0; this._kH = NaN; this.t = 0; this.be = true; this.events = 0; this.nonConv = 0; this.evtRefined = 0; this.refined = 0; this.refineMax = 0; this.steps = 0; this._lteDepth = 0;
    }

    reqP(obj, key, len) { this._preq.push({ obj, key, len }); }
    reqC(obj, key, len) { this._creq.push({ obj, key, len }); }
    newNode() { return this.nNode++; }
    newCNode() { return this.nC++; }

    // Branch variables are allocated after all nodes are known: vars live at indices nNode..nNode+nVar-1.
    // Models call sim.var(obj,key) during setup; we resolve after setup.
    var(obj, key) { (this._vreq || (this._vreq = [])).push({ obj, key }); }

    groundIslands() {
      // resolve branch variables
      const vreq = this._vreq || [];
      vreq.forEach(r => { r.obj[r.key] = this.nNode + this.nVar; this.nVar++; });
      // branch variables that are internal bookkeeping rather than physical quantities are left out of the step-error check
      this.lteSkip = []; vreq.forEach(r => { if (r.obj.lteSkip && r.obj.lteSkip.includes(r.key)) this.lteSkip.push(r.obj[r.key]); });
      // Union-find over power nodes; elements report node groups via m.conn (array of arrays)
      const N = this.nNode; const par = new Int32Array(N + 1); for (let i = 0; i <= N; i++) par[i] = i;
      const G = N; // ground index in UF
      const f = i => { while (par[i] !== i) { par[i] = par[par[i]]; i = par[i]; } return i; };
      const u = (a, b) => { a = a < 0 ? G : a; b = b < 0 ? G : b; const ra = f(a), rb = f(b); if (ra !== rb) par[ra] = rb; };
      for (const m of this.pEls) { const groups = m.conn ? m.conn() : []; for (const g of groups) for (let i = 1; i < g.length; i++) u(g[0], g[i]); }
      const seen = new Set(); this.ties = [];
      for (let i = 0; i < N; i++) { const r = f(i); if (r === f(G) || seen.has(r)) continue; seen.add(r); this.ties.push(i); }
    }

    orderControl() {
      // driver map
      const drv = new Array(this.nC).fill(null);
      for (const b of this.cBlks) for (const o of (b.outs || [])) {
        if (o === 0) throw new SimError(b.el.name + '：輸出端不可直接接地', b.el.id);
        if (o < 0) continue;
        if (drv[o]) throw new SimError('控制節點有多個輸出元件驅動：' + drv[o].el.name + ' 與 ' + b.el.name, b.el.id);
        drv[o] = b;
      }
      this.drv = drv;
      // check consumers
      const consumers = [];
      for (const b of this.cBlks) for (const i of (b.ins || [])) if (i > 0 && !drv[i]) throw new SimError(b.el.name + '：輸入端沒有連接到任何訊號來源', b.el.id);
      for (const m of this.pEls) for (const i of (m.cins || [])) if (i > 0 && !drv[i]) throw new SimError(m.el.name + '：控制/閘極輸入端沒有訊號來源', m.el.id);
      for (const m of this.probes) for (const i of (m.cins || [])) if (i > 0 && !drv[i]) throw new SimError(m.el.name + '：量測的控制節點沒有訊號來源', m.el.id);
      // graph: edge drv(in) -> b, for ft blocks
      const blks = this.cBlks; const idx = new Map(); blks.forEach((b, i) => idx.set(b, i));
      const adj = blks.map(() => []);
      blks.forEach((b, i) => { if (b.ft === false) return; for (const inp of (b.ins || [])) { const d = inp > 0 ? drv[inp] : null; if (d) adj[idx.get(d)].push(i); } });
      // Tarjan
      let index = 0; const ix = new Int32Array(blks.length).fill(-1), low = new Int32Array(blks.length), on = new Uint8Array(blks.length); const st = []; const sccs = [];
      const strong = (v0) => {
        const stack = [[v0, 0]]; ix[v0] = low[v0] = index++; st.push(v0); on[v0] = 1;
        while (stack.length) {
          const top = stack[stack.length - 1]; const v = top[0];
          if (top[1] < adj[v].length) {
            const w = adj[v][top[1]++];
            if (ix[w] < 0) { ix[w] = low[w] = index++; st.push(w); on[w] = 1; stack.push([w, 0]); }
            else if (on[w]) low[v] = Math.min(low[v], ix[w]);
          } else {
            stack.pop();
            if (stack.length) { const u2 = stack[stack.length - 1][0]; low[u2] = Math.min(low[u2], low[v]); }
            if (low[v] === ix[v]) { const comp = []; let w; do { w = st.pop(); on[w] = 0; comp.push(w); } while (w !== v); sccs.push(comp); }
          }
        }
      };
      for (let i = 0; i < blks.length; i++) if (ix[i] < 0) strong(i);
      sccs.reverse(); // Tarjan yields reverse topological order
      this.cGroups = sccs.map(c => {
        const loop = c.length > 1 || adj[c[0]].includes(c[0]);
        return { blks: c.map(i => blks[i]), loop };
      });
      if (this.cGroups.some(g => g.loop)) this.warn('控制電路含有代數迴路，已以逐步疊代求解。');
    }

    /* ---------- matrix assembly and solve ---------- */
    key(h, be) {
      // rebuilt only when the step, the integration rule or a switch state changed (string building is costly per step)
      if (h === this._kH && be === this._kBe && this.swVer === this._kVer) return this._kStr;
      let k = (be ? 'B' : 'T') + h;
      for (const s of this.swEls) k += s.skey();
      this._kH = h; this._kBe = be; this._kVer = this.swVer; this._kStr = k;
      return k;
    }
    assemble(h, be, init) {
      const A = this.A; const pl = this.pat.list; for (let e = 0; e < pl.length; e++) A[pl[e]] = 0; // positions outside the pattern are never written
      const M = this.M;
      if (!init && this.hasNL) {
        // the linear part only depends on (h, be, switch states): keep it per key and scatter it back
        const k = this.key(h, be); const c = this.linCache.get(k);
        if (c) { for (let e = 0; e < c.length; e++) A[pl[e]] = c[e]; }
        else {
          for (const m of this.pEls) if (m.stampG && !m.dyn) m.stampG(M, h, be);
          for (const t of this.ties) aA(M, t, t, 1e-3);
          const pl2 = this.pat.list; const v = new Float64Array(pl2.length); for (let e = 0; e < pl2.length; e++) v[e] = A[pl2[e]];
          if (this.linCache.size > 96) this.linCache.clear();
          this.linCache.set(k, v);
        }
        for (const m of this.dynEls) if (m.stampG) m.stampG(M, h, be);
        return;
      }
      for (const m of this.pEls) { if (init && m.stampInitG) m.stampInitG(M); else if (m.stampG) m.stampG(M, h, be); }
      for (const t of this.ties) aA(M, t, t, 1e-3);
    }
    rhs(t, h, be, init, b) {
      b = b || this.b; b.fill(0);
      for (const m of this.pEls) { if (init && m.stampInitB) m.stampInitB(b, t); else if (m.stampB) m.stampB(b, t, h, be); }
    }
    getLU(h, be) {
      if (this.hasNL || this.hasDyn) return this.factorNow(h, be);
      const k = this.key(h, be); const c = this.cache;
      let lu = c.get(k);
      if (lu) { c.delete(k); c.set(k, lu); } // least recently used eviction order
      else {
        lu = this.factorNow(h, be);
        if (c.size >= 96) c.delete(c.keys().next().value);
        c.set(k, lu);
      }
      return lu;
    }
    factorNow(h, be, init) {
      this.assemble(h, be, init);
      this.pat.commit();
      const sym = this._sym && this._sym.ver === this.pat.ver && this._sym.nnz === this.pat.list.length ? this._sym : null;
      const lu = new S.LU(this.n, sym);
      if (!lu.factor(this.A, this.pat)) throw new SimError(this.singularMsg(lu.bad));
      this._sym = lu.sym;
      lu.keep(this.A, this.pat);
      return lu;
    }
    singularMsg(row) {
      return '電路矩陣奇異（無法求解）。常見原因：電壓源或電感形成迴路、電流源串聯、節點浮接或元件短路。' + (row >= 0 ? '（第 ' + row + ' 列）' : '');
    }
    solveLin(lu, x) {
      lu.solve(this.b, x);
      if (this.o.refine) {
        // one step of iterative refinement (sparse residual on the kept matrix values)
        const n = this.n, r = this.r, d = this.d;
        if (lu.residual(this.b, x, r)) { lu.solve(r, d); for (let i = 0; i < n; i++) x[i] += d[i]; }
      }
    }
    // Solve the power network at time t over interval h with the present switch states.
    solveAt(t, h, be, x) {
      if (!this.hasNL) { const lu = this.getLU(h, be); this.rhs(t, h, be); this.solveLin(lu, x); return; }
      // Newton iteration for nonlinear elements.
      // Initial guess: linear extrapolation of the two last accepted solutions (SPICE predictor) when the
      // step is a regular continuation; this only changes the starting point, the converged solution and
      // the convergence test are the same. Otherwise the previous solution.
      const n = this.n, xP = this.xP;
      let pred = this.predOk && !be && !this.firstStep && !this.topo && this.hP > 0;
      if (pred) { const f = h / this.hP, xPP = this.xPP; for (let i = 0; i < n; i++) x[i] = xP[i] + (xP[i] - xPP[i]) * f; }
      else x.set(xP);
      const nl = this.nlEls;
      const xn = this.xT, b = this.b, b0 = this.b0;
      this.rhs(t, h, be, false, b0); // the linear right-hand side does not depend on x
      const lu = this._nlu || (this._nlu = new S.LU(n));
      let errP = Infinity, errPP = Infinity;
      for (let it = 0, lim = pred ? 12 : 100; it < lim; it++) {
        if (it === 0) for (const m of nl) m.nlGuess && m.nlGuess(x, pred ? xP : x); // junction limiting is referenced to the last accepted solution, not the predictor
        this.assemble(h, be);
        for (const m of nl) m.stampNL(this.M, x, h, be);
        b.set(b0);
        for (const m of nl) m.stampNLB(b, x, t, h, be);
        this.pat.commit();
        if (!lu.factor(this.A, this.pat)) throw new SimError(this.singularMsg(lu.bad));
        lu.keep(this.A, this.pat); this.solveLin(lu, xn);
        let err = 0, sc = 0;
        for (let i = 0; i < n; i++) { const d = Math.abs(xn[i] - x[i]); if (d > err) err = d; const a = Math.abs(xn[i]); if (a > sc) sc = a; }
        x.set(xn);
        for (const m of nl) m.nlClamp && m.nlClamp(x);
        // converged, or stalled at the round-off floor of a badly scaled matrix (the update no longer shrinks
        // but is already below 1e-6 relative): both count as converged
        const stall = err <= 1e-6 * Math.max(1, sc) + 1e-9 && err > 0.5 * errP && errP > 0.5 * errPP;
        if (err <= 1e-9 * Math.max(1, sc) + 1e-12 || stall) { let ok = true; for (const m of nl) if (m.nlOk && !m.nlOk()) { ok = false; break; } if (ok) return; }
        errPP = errP; errP = err;
        // the extrapolated start did not converge quickly: restart from the previous solution (the plain scheme)
        if (pred && it === lim - 1) { pred = false; lim = 100; it = -1; x.set(xP); errP = errPP = Infinity; }
      }
      this.nonConv++;
      this.warn('非線性元件在 t=' + t.toExponential(4) + ' s 附近未完全收斂。');
    }

    // Apply latched gate values from the control circuit (except forced ones)
    applyGates(t) {
      let ch = false;
      for (const m of this.timedSw) { const st = m.stateAt(t + this.epsT); if (st !== m.s[0]) { m.s[0] = st; ch = true; } }
      for (const m of this.gated) {
        if (m.s[m.fIdx] > t + this.epsT) continue; // forced until this time
        const g = this.cv[m.gn] > 0.5 ? 1 : 0;
        if (g !== m.s[m.gIdx]) { m.s[m.gIdx] = g; if (m.onGate(g, this.xP)) ch = true; }
      }
      if (ch) { this.be = true; this.topo = true; this.swVer++; }
    }

    // Power solution over [tA,tB] with state consistency and event localization.
    solvePower(tA, tB, depth, noSplit) {
      const h = tB - tA; const x = this.x;
      let be = this.o.damp ? this.be : false;
      if (this.firstStep) be = true;
      // damping after a switching event: a short backward-Euler step (h/8) is enough to reset the derivative
      // history that makes the trapezoidal rule ring; the rest of the interval is trapezoidal again, so the
      // first-order error of the damping step is 64x smaller than with a full backward-Euler step
      if (be && !noSplit && this.o.beSplit !== false && depth < 12 && h > 32 * this.epsT) {
        // two short backward-Euler steps (h/8 each) damp a stiff mode by 1/(1 + h·lambda/8)^2, better than one
        // full step 1/(1 + h·lambda) for h·lambda > 60, with a far smaller first-order error on the smooth part
        const t1 = tA + h / 8, t2 = tA + h / 4;
        this.solvePower(tA, t1, depth + 1, true);
        this.be = true; this.solvePower(t1, t2, depth + 1, true);
        this.solvePower(t2, tB, depth + 1);
        return;
      }
      this.solveAt(tB, h, be, x);
      if (!this.swEls.length) { this.accept(tB, h, be); return; }
      let ch = this.changes(x, tB, depth);
      const topo = this.topo; this.topo = false;
      if (!ch.length) { this.accept(tB, h, be); return; }
      if (this.o.interp && !topo && depth < 12 && h > this.epsT * 4) {
        let tmin = 2;
        for (const c of ch) { const tau = c.m.tau ? c.m.tau(this.xP, x, c.ns) : NaN; c.tau = tau; if (tau > 1e-6 && tau < 1 - 1e-6 && tau < tmin) tmin = tau; }
        if (tmin < 1) {
          let te = tA + tmin * h;
          this.solveAt(te, te - tA, be, x);
          // refine the switching instant: the linear interpolation above is exact only when the switching
          // quantity varies linearly over the step; re-evaluate it at te and apply secant corrections until
          // the crossing sits at te (within 1e-4 of the step), each correction costing one more solve
          const lead = []; let tNext = tB; for (const c of ch) { if (Math.abs(c.tau - tmin) < 1e-6) { if (c.m.qq) lead.push(c); } else if (c.tau > tmin && c.tau < 1) tNext = Math.min(tNext, tA + c.tau * h); }
          if (this.o.refineEvt !== false && lead.length) {
            for (let it = 0; it < 4; it++) {
              let f = NaN;
              for (const c of lead) { const q = c.m.qq(this.xP, x, c.ns); if (!q) continue; const [qa, qb] = q; if (qa === qb) continue; const fr = qa / (qa - qb); if (!(fr > 0)) continue; if (!(fr < f)) { if (f !== f) f = fr; } else f = fr; }
              if (!(f > 0) || Math.abs(f - 1) < 1e-4) break;
              const tn = tA + Math.min(f, 1.5) * (te - tA);
              if (tn <= tA + 2 * this.epsT || tn >= tNext - 2 * this.epsT) break;
              te = tn; this.solveAt(te, te - tA, be, x); this.evtRefined++;
            }
          }
          this.accept(te, te - tA, be);
          this.firstStep = false;
          let any = false;
          for (const c of ch) if (Math.abs(c.tau - tmin) < 1e-6) { c.m.setState(c.ns); any = true; }
          this.swVer++;
          if (any) { this.be = true; this.topo = true; this.events++; }
          this.solvePower(te, tB, depth + 1);
          return;
        }
      }
      // plain consistency iteration (switch at tA)
      let it = 0;
      while (ch.length && it < 40) {
        for (const c of ch) c.m.setState(c.ns);
        this.swVer++; be = this.o.damp ? true : be;
        this.solveAt(tB, h, be, x);
        ch = this.changes(x, tB, depth); it++;
      }
      if (ch.length) { this.nonConv++; this.warn('開關狀態在 t=' + tB.toExponential(4) + ' s 附近未收斂（可能振盪），請縮小時間步長。'); }
      // states changed at tA: redo the interval with the short damping step instead of one full backward-Euler step
      if (it > 0 && be && !noSplit && this.o.beSplit !== false && depth < 12 && h > 32 * this.epsT) { this.be = true; this.topo = true; this.solvePower(tA, tB, depth + 1); return; }
      this.accept(tB, h, be);
    }
    changes(x, t, depth) {
      // pooled per recursion depth (no allocation in the common no-change case)
      const pool = this._chPool || (this._chPool = []); depth = depth || 0;
      let out = pool[depth]; if (!out) { out = pool[depth] = []; out.objs = []; }
      out.length = 0;
      for (const m of this.swEls) { const ns = m.desired(x, t); if (ns !== m.state()) { const o = out.objs[out.length] || (out.objs[out.length] = { m: null, ns: 0, tau: NaN }); o.m = m; o.ns = ns; o.tau = NaN; out.push(o); } }
      return out;
    }
    accept(t, h, be) {
      const x = this.x;
      for (const m of this.pEls) if (m.accept) m.accept(x, t, h, be);
      this.x4.set(this.xPPP); this.hPPP = this.hPP;
      this.xPPP.set(this.xPP); this.hPP = this.hP;
      this.xPP.set(this.xP); this.hP = h; this.predOk = !be && !this.firstStep;
      this.xP.set(x);
      if (this.o.lte) { const sc = this.xScale, nN = this.nNode; let gV = this.scV, gI = this.scI; for (let i = 0, n = this.n; i < n; i++) { const a = x[i] < 0 ? -x[i] : x[i]; if (a > sc[i]) { sc[i] = a; if (i < nN) { if (a > gV) gV = a; } else if (a > gI) gI = a; } } this.scV = gV; this.scI = gI; }
      this.lastBE = be || this.firstStep; this.histN = this.lastBE ? 0 : this.histN + 1;
      this.be = false; this.firstStep = false;
    }

    /* ---------- control circuit ---------- */
    evalControl(t, h) {
      const cev = this.cev; cev.fill(NaN);
      for (const g of this.cGroups) {
        if (!g.loop) { for (const b of g.blks) b.eval(t, h, this); continue; }
        const outs = []; g.blks.forEach(b => (b.outs || []).forEach(o => { if (o > 0) outs.push(o); }));
        const n = outs.length, prev = new Float64Array(n);
        let ok = false;
        // 1) Gauss-Seidel sweeps (converge quickly when the loop gain per step is small)
        for (let it = 0; it < 40; it++) {
          outs.forEach((o, i) => { prev[i] = this.cv[o]; });
          for (const b of g.blks) b.eval(t, h, this);
          let err = 0; outs.forEach((o, i) => { const d = Math.abs(this.cv[o] - prev[i]); if (d > 1e-12 * Math.max(1, Math.abs(prev[i]))) err = 1; });
          if (!err && it > 0) { ok = true; break; }
        }
        // 2) Newton iteration on the loop variables with a finite-difference Jacobian
        if (!ok && n <= 60) ok = this.loopNewton(g, outs, t, h);
        if (!ok) { this.nonConv++; this.warn('控制電路代數迴路在 t=' + t.toExponential(4) + ' s 未收斂。'); }
      }
    }
    // g(z): evaluate every block of the loop with all loop variables held at z (Jacobi form)
    loopMap(g, outs, z, t, h, out) {
      const cv = this.cv; const idx = new Map(); outs.forEach((o, i) => idx.set(o, i));
      for (const b of g.blks) {
        outs.forEach((o, i) => { cv[o] = z[i]; });
        b.eval(t, h, this);
        (b.outs || []).forEach(o => { const i = idx.get(o); if (i !== undefined) out[i] = cv[o]; });
      }
    }
    loopNewton(g, outs, t, h) {
      const n = outs.length; const z = new Float64Array(n), gz = new Float64Array(n), gp = new Float64Array(n);
      outs.forEach((o, i) => { z[i] = this.cv[o]; });
      const J = new Float64Array(n * n), r = new Float64Array(n), dz = new Float64Array(n);
      for (let it = 0; it < 30; it++) {
        this.loopMap(g, outs, z, t, h, gz);
        let err = 0; for (let i = 0; i < n; i++) { r[i] = gz[i] - z[i]; err = Math.max(err, Math.abs(r[i]) / Math.max(1, Math.abs(z[i]))); }
        if (err < 1e-12) { this.loopMap(g, outs, gz, t, h, gp); outs.forEach((o, i) => { this.cv[o] = gz[i]; }); return true; }
        for (let j = 0; j < n; j++) {
          const d = 1e-7 * Math.max(1, Math.abs(z[j])); const zj = z[j]; z[j] = zj + d;
          this.loopMap(g, outs, z, t, h, gp); z[j] = zj;
          for (let i = 0; i < n; i++) J[i * n + j] = (gp[i] - gz[i]) / d - (i === j ? 1 : 0);
        }
        const lu = new S.LU(n); if (!lu.factor(J)) return false;
        for (let i = 0; i < n; i++) r[i] = -r[i];
        lu.solve(r, dz); for (let i = 0; i < n; i++) z[i] += dz[i];
      }
      return false;
    }
    commitControl(t, h) {
      for (const b of this.cBlks) if (b.commit) b.commit(t, h, this);
      // a NaN in a control signal (0/0, log of a negative number, ...) would silently poison every integrator
      // downstream: say which block produced it, once
      if (!this._nanSeen) { const cv = this.cv; for (let i = 1, n = this.nC; i < n; i++) if (cv[i] !== cv[i]) { this._nanSeen = true; const d = this.drv && this.drv[i]; this.warn('控制訊號在 t=' + t.toExponential(4) + ' s 出現 NaN（非數值）' + (d && d.el ? '，來源：' + d.el.name : '') + '。常見原因：除以 0（例如 x/Vd 在 Vd = 0 時）、負數開根號或取對數；請加 max()/min() 保護。'); break; } }
      this.cvP.set(this.cv);
    }

    /* ---------- interval and stepping ---------- */
    snapshot(depth) {
      // one reusable buffer set per recursion depth (no allocation per step)
      const pool = this._snaps || (this._snaps = []);
      let sn = pool[depth];
      if (!sn) sn = pool[depth] = { PS: new Float64Array(this.PS.length), x: new Float64Array(this.n), xP: new Float64Array(this.n), xPP: new Float64Array(this.n), xPPP: new Float64Array(this.n), x4: new Float64Array(this.n), cv: new Float64Array(this.nC), be: false, fs: false, topo: false, hP: 0, hPP: 0, hPPP: 0, histN: 0, lastBE: true, predOk: false };
      sn.PS.set(this.PS); sn.x.set(this.x); sn.xP.set(this.xP); sn.xPP.set(this.xPP); sn.xPPP.set(this.xPPP); sn.x4.set(this.x4); sn.cv.set(this.cvP); sn.be = this.be; sn.fs = this.firstStep; sn.topo = this.topo;
      sn.hP = this.hP; sn.hPP = this.hPP; sn.hPPP = this.hPPP; sn.histN = this.histN; sn.lastBE = this.lastBE; sn.predOk = this.predOk;
      return sn;
    }
    restore(sn) {
      this.PS.set(sn.PS); this.x.set(sn.x); this.xP.set(sn.xP); this.xPP.set(sn.xPP); this.xPPP.set(sn.xPPP); this.x4.set(sn.x4); this.cv.set(sn.cv); this.be = sn.be; this.firstStep = sn.fs; this.topo = sn.topo;
      this.hP = sn.hP; this.hPP = sn.hPP; this.hPPP = sn.hPPP; this.histN = sn.histN; this.lastBE = sn.lastBE; this.predOk = sn.predOk; this.swVer++;
    }
    // local truncation error of the trapezoidal step that produced this.x, from the third divided difference of
    // the last four accepted points (SPICE's estimate), as the largest ratio to the tolerance over all unknowns
    lteRatio() {
      // called after accept(): xP = x(tB), xPP = x(tA), xPPP and x4 the two points before; hP is the step just taken
      const n = this.n, x = this.xP, x1 = this.xPP, x2 = this.xPPP, x3 = this.x4, h = this.hP, h1 = this.hPP, h2 = this.hPPP;
      const rel = this.o.lteTol, nN = this.nNode, sc = this.xScale, w = this.lteW;
      // absolute floor: the user's absolute tolerance, or 1e-3 of the relative tolerance applied to the largest
      // voltage / current anywhere in the circuit (a branch that carries ~0 A next to 100 A branches is not
      // refined down to nano-amps)
      const aV = this.o.lteAbsV + 1e-3 * rel * this.scV, aI = this.o.lteAbsI + 1e-3 * rel * this.scI;
      const c = h * h * h / 2; let worst = 0;
      for (let i = 0; i < n; i++) {
        const d1a = (x[i] - x1[i]) / h, d1b = (x1[i] - x2[i]) / h1, d1c = (x2[i] - x3[i]) / h2;
        const d2a = (d1a - d1b) / (h + h1), d2b = (d1b - d1c) / (h1 + h2);
        const d3 = (d2a - d2b) / (h + h1 + h2);
        // tolerance relative to the scale of the waveform (largest magnitude seen so far), as PSIM-style users
        // judge accuracy against the waveform, not against a momentary value near zero
        const err = Math.abs(c * d3) * w[i]; const tol = rel * sc[i] + (i < nN ? aV : aI);
        const r = err / tol; if (r > worst) worst = r;
      }
      return worst;
    }
    interval(tA, tB, depth) {
      const h = tB - tA;
      const lteOn = this.o.lte && this._lteDepth < this.o.lteMaxDepth && h > 32 * this.epsT;
      const needSnap = ((this.o.interp && this.gated.length) || lteOn) && depth < 10;
      const sn = needSnap ? this.snapshot(depth) : null;
      this.applyGates(tA);
      this.solvePower(tA, tB, 0);
      this.evalControl(tB, h);
      if (depth === 0) this.steps++;
      if (lteOn && sn && !this.lastBE && this.histN >= 5 && this.lteRatio() > 1) {
        // the step is too long for the requested accuracy: redo it as two halves (recursively, up to 2^lteMaxDepth)
        this.restore(sn); this.refined++; this._lteDepth++;
        const tm = tA + h / 2;
        this.interval(tA, tm, depth + 1); this.interval(tm, tB, depth + 1);
        this._lteDepth--;
        return;
      }
      if (!lteOn && this.o.lte && this._lteDepth >= this.o.lteMaxDepth && !this.lastBE && this.histN >= 5 && this.lteRatio() > 1) this.refineMax++;
      if (needSnap && (this.o.interp && this.gated.length)) {
        let te = Infinity; const lo = tA + this.epsT, hi = tB - this.epsT;
        for (const m of this.gated) {
          if (m.s[m.fIdx] > tA + this.epsT) continue;
          const g = this.cv[m.gn] > 0.5 ? 1 : 0;
          if (g === m.s[m.gIdx]) continue;
          const e = this.cev[m.gn];
          if (e > lo && e < hi && e < te) te = e;
        }
        if (te < Infinity) {
          const tol = Math.max(this.epsT, 1e-9 * h);
          const forced = [];
          for (const m of this.gated) {
            if (m.s[m.fIdx] > tA + this.epsT) continue;
            const g = this.cv[m.gn] > 0.5 ? 1 : 0;
            if (g !== m.s[m.gIdx] && Math.abs(this.cev[m.gn] - te) <= tol) forced.push([m, g]);
          }
          this.restore(sn);
          this.interval(tA, te, depth + 1);
          let ch = false;
          for (const [m, g] of forced) {
            if (m.s[m.gIdx] !== g) { m.s[m.gIdx] = g; if (m.onGate(g, this.xP)) ch = true; }
            m.s[m.fIdx] = tB;
          }
          if (ch) { this.be = true; this.topo = true; this.swVer++; }
          this.events++;
          this.interval(te, tB, depth + 1);
          return;
        }
      }
      this.commitControl(tB, h);
      this.t = tB;
    }
    nextBreak(t) {
      let tb = Infinity;
      for (const m of this.bpEls) { const v = m.nextBreak(t + this.epsT, this); if (v < tb) tb = v; }
      return tb;
    }
    jumpAt(t) { for (const m of this.bpEls) if (m.jumpAt && m.jumpAt(t)) return true; return false; }

    // Advance from this.t to tEnd (a grid point), splitting at breakpoints.
    advanceTo(tEnd) {
      let t = this.t;
      let guard = 0;
      while (t < tEnd - this.epsT) {
        const bp = this.nextBreak(t);
        let tn = tEnd;
        if (bp < tEnd - this.epsT && bp > t + this.epsT) tn = bp;
        this.interval(t, tn, 0);
        const atBp = tn !== tEnd ? true : Math.abs(bp - tEnd) <= this.epsT;
        if (atBp) { if (this.jumpAt(tn)) this.be = true; this.histN = 0; } // a source corner: the step-error history is not smooth across it
        t = tn; this.t = t;
        if (++guard > 1e6) throw new SimError('單一步長內的斷點過多，請檢查訊號源頻率。');
      }
      this.t = tEnd;
    }
    /* ---------- initialisation ---------- */
    init() {
      this.epsT = this.dt * 1e-6;
      for (const m of this.pEls) if (m.init) m.init(this);
      for (const b of this.cBlks) if (b.init) b.init(this);
      // initial power solution (capacitors as voltage sources, inductors as current sources)
      let x0 = new Float64Array(this.n); let ok = true;
      try {
        this.initSolve(x0, false);
      } catch (e) {
        if (!(e instanceof SimError)) throw e;
        // second attempt: inductors whose initial current is 0 are taken as DC shorts (an inductor in series with a
        // voltage source, or between nodes that only current sources touch, has no consistent current-source solution)
        try {
          this.initSolve(x0, true);
          // report only the inductors whose DC-short current actually differs from the requested 0
          const named = [];
          for (const m of this.pEls) if (m.initAsI && !m.initAsI(true) && m.L && Math.abs(x0[m.k]) > 1e-2) named.push(m.el.name + ' = ' + x0[m.k].toPrecision(4) + ' A');
          if (named.length) this.warn('t = 0 的初始條件以直流穩態求得（電感與電壓源串聯或節點僅由電感連接時，初始電流為 0 的電感視為短路）：' + named.join('、') + '，與設定的 i0 = 0 不同。');
        } catch (e2) {
          if (!(e2 instanceof SimError)) throw e2;
          this.warn('初始條件無法一致求解（例如電容與電壓源並聯但初始電壓不同），t=0 的輸出以近似值顯示。');
          x0 = this.initFallback(); ok = false; this.initCurrents = null;
        }
      }
      this.x.set(x0); this.xP.set(x0);
      if (ok) for (const m of this.pEls) if (m.acceptInit) m.acceptInit(x0, this);
      // control at t=0
      this.evalControl(0, 0);
      this.commitControl(0, 0);
      let ch = false;
      for (const m of this.gated) { const g = this.cv[m.gn] > 0.5 ? 1 : 0; m.s[m.gIdx] = g; m.s[m.fIdx] = -1; if (m.onGate(g, this.xP, true)) ch = true; }
      for (const m of this.timedSw) { const st = m.stateAt(this.epsT); if (st !== m.s[0]) { m.s[0] = st; ch = true; } }
      this.swVer++; this.t = 0; this.be = !ok || ch; this.firstStep = !ok || ch;
    }
    initSolve(x0, relax) {
      // extended system: add one variable per capacitor for its init voltage constraint
      const extra = []; for (const m of this.pEls) if (m.initVar) extra.push(m);
      const n0 = this.n, n = n0 + extra.length;
      extra.forEach((m, i) => { m._iv = n0 + i; });
      const A = new Float64Array(n * n); const pat = new S.Pattern(n); const M = { A, n, pat, pm: pat.mask }; const b = new Float64Array(n);
      const doAssemble = () => {
        A.fill(0); b.fill(0);
        for (const m of this.pEls) {
          if (m.stampInitG) m.stampInitG(M, relax); else if (m.stampG) m.stampG(M, this.dt, true);
          if (m.stampInitB) m.stampInitB(b, relax); else if (m.stampB) m.stampB(b, 0, this.dt, true);
        }
        for (const t of this.ties) aA(M, t, t, 1e-3);
      };
      const xs = new Float64Array(n); const xo = new Float64Array(n);
      // devices solved by Newton at t = 0 (true DC operating point, charges open)
      const nli = this.nlEls.filter(m => m.nlInit);
      for (const m of nli) m.nlGuess && m.nlGuess(xs);
      let nlIt = 0;
      for (let it = 0; it < 400; it++) {
        doAssemble();
        for (const m of nli) { m.stampNL(M, xs, this.dt, true, true); m.stampNLB(b, xs, 0, this.dt, true, true); }
        // a node that only inductors (current sources at t = 0) and leakage-level conductances touch has no
        // defined voltage in this pass: not a usable operating point, ask for the relaxed pass (inductors as shorts)
        if (it === 0 && !relax && this.weakInitNode(A, n)) throw new SimError('浮接節點');
        const lu = this._ilu || (this._ilu = new S.LU(n)); pat.commit();
        if (!lu.factor(A, pat)) throw new SimError(this.singularMsg(lu.bad));
        xo.set(xs);
        lu.solve(b, xs);
        if (this.o.refine) { const r = new Float64Array(n), d = new Float64Array(n); lu.keep(A, pat); if (lu.residual(b, xs, r)) { lu.solve(r, d); for (let i = 0; i < n; i++) xs[i] += d[i]; } }
        x0.set(xs.subarray(0, n0));
        if (nli.length) {
          let err = 0, sc = 0; for (let i = 0; i < n0; i++) { err = Math.max(err, Math.abs(xs[i] - xo[i])); sc = Math.max(sc, Math.abs(xs[i])); }
          const lim = nli.some(m => m.nlOk && !m.nlOk());
          if (++nlIt < 300 && (it === 0 || lim || err > 1e-9 * Math.max(1, sc) + 1e-12)) continue;
          if (nlIt >= 300) this.warn('t = 0 的非線性工作點未完全收斂，初始值可能有誤差。');
          nlIt = 0;
        }
        if (!this.swEls.length) break;
        const ch = this.changes(x0, 0, 15);
        if (!ch.length) break;
        for (const c of ch) c.m.setState(c.ns);
        this.swVer++;
      }

      this.initCurrents = xs;
    }
    weakInitNode(A, n) {
      // a node is defined at t = 0 when a chain of real conductances (≥ 1 nS) and voltage-type branches (sources,
      // ammeters, capacitors) connects it to ground; inductors (current sources in this pass) and leakage do not count
      const nN = this.nNode, G = nN; const par = new Int32Array(nN + 1); for (let i = 0; i <= nN; i++) par[i] = i;
      const find = a => { while (par[a] !== a) { par[a] = par[par[a]]; a = par[a]; } return a; };
      const uni = (a, b) => { a = find(a); b = find(b); if (a !== b) par[a] = b; };
      const TH = 1e-9;
      for (let i = 0; i < nN; i++) {
        let off = 0;
        for (let j = 0; j < nN; j++) { if (j === i) continue; const v = Math.abs(A[i * n + j]); if (v >= TH) { uni(i, j); off += v; } }
        if (Math.abs(A[i * n + i]) - off >= TH) uni(i, G); // net conductance to ground
      }
      for (let k = nN; k < n; k++) {
        let first = -1, cnt = 0;
        for (let j = 0; j < nN; j++) if (A[k * n + j] !== 0) { cnt++; if (first < 0) first = j; else uni(first, j); }
        if (cnt === 1) uni(first, G); // branch between a node and ground
      }
      const g = find(G);
      for (let i = 0; i < nN; i++) if (find(i) !== g) return true;
      return false;
    }
    initFallback() {
      const x0 = new Float64Array(this.n);
      const h = this.dt * 1e-3;
      try { const lu = this.factorNow(h, true); this.rhs(0, h, true); lu.solve(this.b, x0); } catch (e) { /* ignore */ }
      this.cache.clear();
      return x0;
    }

    findSource(name) { return this.sources[String(name || '').toLowerCase()] || null; }

    /* ---------- full state save / restore (AC sweep) ---------- */
    saveFull() {
      const ex = []; for (const b of this.cBlks) ex.push(b.saveX ? b.saveX() : null);
      for (const m of this.pEls) ex.push(m.saveX ? m.saveX() : null);
      return { PS: this.PS.slice(), CS: this.CS.slice(), x: this.x.slice(), xP: this.xP.slice(), cv: this.cv.slice(), cvP: this.cvP.slice(), t: this.t, be: this.be, ex };
    }
    restoreFull(s) {
      this.PS.set(s.PS); this.CS.set(s.CS); this.x.set(s.x); this.xP.set(s.xP); this.cv.set(s.cv); this.cvP.set(s.cvP); this.t = s.t; this.be = s.be; this.firstStep = false; this.predOk = false; this.swVer++;
      let i = 0; for (const b of this.cBlks) { if (b.restoreX) b.restoreX(s.ex[i]); i++; }
      for (const m of this.pEls) { if (m.restoreX) m.restoreX(s.ex[i]); i++; }
    }
  }
  S.Sim = Sim;

  /* ------------------------------------------------------------------
   * 7. Element context passed to model factories
   * ------------------------------------------------------------------ */
  class ElCtx {
    constructor(sim, el, pins, P) { this.sim = sim; this.el = el; this.pins = pins; this.P = P; }
    // power node of pin i (must be power or ground)
    p(i) {
      const pn = this.pins[i];
      if (!pn || pn.kind === 'N') throw new SimError(this.el.name + '：第 ' + (i + 1) + ' 個接腳未連接', this.el.id);
      if (pn.kind === 'C') throw new SimError(this.el.name + '：此接腳必須接到功率電路，卻接到控制電路', this.el.id);
      return pn.p;
    }
    // control node of pin i (control or ground)
    c(i) {
      const pn = this.pins[i];
      if (!pn || pn.kind === 'N') throw new SimError(this.el.name + '：第 ' + (i + 1) + ' 個接腳未連接', this.el.id);
      if (pn.kind === 'P') throw new SimError(this.el.name + '：此接腳必須接到控制電路，卻接到功率電路', this.el.id);
      return pn.c;
    }
    // optional control pin: returns -1 when unconnected
    cOpt(i) { const pn = this.pins[i]; if (!pn || pn.kind === 'N') return -1; return this.c(i); }
    kind(i) { const pn = this.pins[i]; return pn ? pn.kind : 'N'; }
    isCtl(i) { const k = this.kind(i); return k === 'C'; }
    connected(i) { const k = this.kind(i); return k !== 'N'; }
    // any-kind pin: returns {p} or {c}
    any(i) { const pn = this.pins[i]; if (!pn || pn.kind === 'N') throw new SimError(this.el.name + '：第 ' + (i + 1) + ' 個接腳未連接', this.el.id); return pn; }
    err(msg) { return new SimError(this.el.name + '：' + msg, this.el.id); }
    get name() { return this.el.name; }
  }
  S.ElCtx = ElCtx;

  /* ------------------------------------------------------------------
   * 8. Transient runner (chunked so it can report progress)
   * ------------------------------------------------------------------ */
  class TransientRunner {
    constructor(net, sc) {
      const dt = sc.dt, tT = sc.tTotal;
      if (!(dt > 0)) throw new SimError('時間步長必須大於 0');
      if (!(tT > 0)) throw new SimError('總時間必須大於 0');
      this.sc = sc;
      this.sim = new Sim(net, { dt, interp: sc.interp !== false, damp: sc.damp !== false, refine: sc.refine !== false, lte: sc.lte !== false, lteTol: sc.lteTol > 0 ? sc.lteTol : 1e-3, beSplit: sc.beSplit !== false, refineEvt: sc.refineEvt !== false });
      this.N = Math.max(1, Math.round(tT / dt));
      if (Math.abs(this.N * dt - tT) > dt * 1e-6) this.N = Math.ceil(tT / dt - 1e-9);
      this.ps = Math.max(1, Math.round(sc.printStep || 1));
      this.nPrint0 = Math.max(0, Math.ceil((sc.tPrint || 0) / dt - 1e-9));
      const nRec = Math.floor((this.N - this.nPrint0) / this.ps) + 1;
      const probes = this.sim.probes;
      if (!probes.length) throw new SimError('電路中沒有任何量測探棒（Probe），請放置電壓/電流探棒以觀察波形。');
      const bytes = nRec * probes.length * 8;
      if (bytes > 1.2e9) throw new SimError('輸出資料量過大（約 ' + (bytes / 1e6).toFixed(0) + ' MB）。請加大 Print step 或延後 Print time。');
      this.nRec = nRec;
      this.data = probes.map(() => new Float64Array(nRec));
      this.names = probes.map(p => p.pname);
      this.rec = 0; this.n = 0; this.started = false;
    }
    record() {
      if (this.n >= this.nPrint0 && (this.n - this.nPrint0) % this.ps === 0 && this.rec < this.nRec) {
        const pr = this.sim.probes;
        for (let i = 0; i < pr.length; i++) this.data[i][this.rec] = pr[i].value(this.sim);
        this.rec++;
      }
    }
    // run for up to `ms` milliseconds. returns true when done.
    run(ms) {
      const sim = this.sim; const t0 = Date.now();
      if (!this.started) { sim.init(); this.started = true; this.record(); }
      const dt = sim.dt;
      while (this.n < this.N) {
        const n = this.n + 1;
        sim.advanceTo(n * dt);
        this.n = n;
        this.record();
        if ((n & 63) === 0 && Date.now() - t0 > ms) return false;
      }
      if (!this.finished) { this.finished = true; for (const m of sim.pEls) if (m.finish) m.finish(sim.t); }
      return true;
    }
    progress() { return this.n / this.N; }
    result() {
      const dtOut = this.sim.dt * this.ps;
      const sim = this.sim;
      return { kind: 'tran', t0: this.nPrint0 * sim.dt, dt: dtOut, n: this.rec, names: this.names, data: this.data.map(d => d.subarray(0, this.rec)), warnings: sim.warnings, events: sim.events,
        stats: { steps: sim.steps, refined: sim.refined, refineMax: sim.refineMax, evtRefined: sim.evtRefined, nonConv: sim.nonConv, lte: !!sim.o.lte } };
    }
  }
  S.TransientRunner = TransientRunner;

  /* ------------------------------------------------------------------
   * 9. AC sweep
   * ------------------------------------------------------------------ */
  function sweepFreqs(ac) {
    const f0 = ac.fStart, f1 = ac.fEnd, N = Math.max(1, Math.round(ac.nPts));
    if (!(f0 > 0) || !(f1 > f0)) throw new SimError('AC 掃頻：結束頻率必須大於起始頻率，且起始頻率需大於 0');
    const fs = [];
    for (let i = 0; i < N; i++) {
      const r = N === 1 ? 0 : i / (N - 1);
      fs.push(ac.flag === 1 ? f0 + (f1 - f0) * r : f0 * Math.pow(f1 / f0, r));
    }
    (ac.extra || []).forEach(f => { if (f > 0) fs.push(f); });
    fs.sort((a, b) => a - b);
    return fs.filter((f, i) => i === 0 || Math.abs(f - fs[i - 1]) > 1e-9 * f);
  }
  S.sweepFreqs = sweepFreqs;

  function ampAt(ac, f) {
    const a0 = ac.aStart, a1 = ac.aEnd;
    if (ac.fEnd <= ac.fStart || a0 === a1) return a0;
    let r = ac.flag === 1 ? (f - ac.fStart) / (ac.fEnd - ac.fStart) : Math.log(f / ac.fStart) / Math.log(ac.fEnd / ac.fStart);
    r = Math.max(0, Math.min(1, r));
    if (ac.flag !== 1 && a0 > 0 && a1 > 0) return a0 * Math.pow(a1 / a0, r);
    return a0 + (a1 - a0) * r;
  }

  // Injection (time-domain) AC sweep runner
  class ACInjectRunner {
    constructor(net, sc, ac) {
      this.sc = sc; this.ac = ac;
      const o = { dt: sc.dt, interp: sc.interp !== false, damp: sc.damp !== false, refine: sc.refine !== false };
      this.A = new Sim(net, o);
      if (!this.A.acProbes.length) throw new SimError('AC 掃頻：電路中沒有 AC Sweep 探棒（AC Sweep Probe）。');
      this.src = this.A.findSource(ac.src);
      if (!this.src || !this.src.setInj) throw new SimError('AC 掃頻：找不到名稱為「' + ac.src + '」的正弦波激勵源（需為 Sine 電壓/電流源）。');
      this.useB = ac.baseline !== 0;
      if (this.useB) { this.B = new Sim(net, o); this.srcB = this.B.findSource(ac.src); }
      this.freqs = sweepFreqs(ac);
      this.res = this.A.acProbes.map(p => ({ name: p.pname, re: [], im: [], loop: p.loop }));
      this.fi = 0; this.phase = 'pre'; this.preN = Math.max(1, Math.round((ac.tPre > 0 ? ac.tPre : sc.tTotal) / sc.dt)); this.n = 0;
      this.warn = [];
      this.fDone = [];
    }
    progress() {
      const nf = this.freqs.length;
      const pre = 0.15;
      if (this.phase === 'pre') return pre * this.n / this.preN;
      return pre + (1 - pre) * (this.fi + (this.cur ? Math.min(0.99, this.cur.cyc / Math.max(1, this.cur.maxCyc)) : 0)) / nf;
    }
    startFreq() {
      const f = this.freqs[this.fi], ac = this.ac, dt = this.sc.dt;
      if (f * dt > 0.05) this.warn.push('頻率 ' + fmtF(f) + ' 的每週期取樣點不足 20 點，結果可能不準確，請縮小時間步長。');
      this.A.restoreFull(this.snap);
      if (this.useB) this.B.restoreFull(this.snap);
      const amp = ampAt(ac, f);
      const t0 = this.A.t;
      const T = 1 / f;
      const settle = Math.max(ac.settle * T, ac.tSettle || 0);
      const Tr = Math.min(2 * T, 0.5 * settle);
      this.src.setInj(amp, f, t0, Tr);
      if (this.useB) this.srcB.setInj(0, f, t0, Tr);
      this.cur = { f, amp, t0, T, w: 2 * Math.PI * f, tw0: t0 + settle, cyc: 0, nm: Math.max(1, Math.round(ac.mcyc)), maxCyc: Math.max(ac.maxCyc, ac.settle + 2 * ac.mcyc), win: null, last: null, prevVals: null, settleCyc: ac.settle };
      this.cur.maxCyc = Math.max(this.cur.maxCyc, Math.ceil(settle / T) + 2 * this.cur.nm);
      this.newWindow(this.cur.tw0);
      this.prevY = this.probeVals();
      this.prevT = t0;
    }
    newWindow(ts) {
      const c = this.cur; const np = this.A.acProbes.length;
      const te = ts + c.nm * c.T;
      c.win = { ts, te, tm: 0.5 * (ts + te), sr: new Float64Array(np * 2), si: new Float64Array(np * 2), s1: new Float64Array(np * 2), gr: 0, gi: 0 };
    }
    probeVals() {
      const pr = this.A.acProbes; const out = new Float64Array(pr.length * 2);
      for (let i = 0; i < pr.length; i++) {
        const a = pr[i].acVals(this.A);
        out[2 * i] = a[0]; out[2 * i + 1] = a[1];
        if (this.useB) { const b = this.B.acProbes[i].acVals(this.B); out[2 * i] -= b[0]; out[2 * i + 1] -= b[1]; }
      }
      return out;
    }
    // integrate segment [ta,tb] (linear interpolation of y) of y*exp(-jw(t-t0)) into window
    accum(ta, ya, tb, yb) {
      const c = this.cur, w = c.win;
      const lo = Math.max(ta, w.ts), hi = Math.min(tb, w.te);
      if (hi <= lo) return;
      const L = tb - ta;
      const n = ya.length;
      const pa = c.w * (lo - c.t0), pb = c.w * (hi - c.t0);
      const ca = Math.cos(pa), sa = -Math.sin(pa), cb = Math.cos(pb), sb = -Math.sin(pb);
      const fa = (lo - ta) / L, fb = (hi - ta) / L, dh = (hi - lo) / 2;
      const ga = lo - w.tm, gb = hi - w.tm;
      w.gr += dh * (ga * ca + gb * cb); w.gi += dh * (ga * sa + gb * sb);
      for (let i = 0; i < n; i++) {
        const y1 = ya[i] + (yb[i] - ya[i]) * fa, y2 = ya[i] + (yb[i] - ya[i]) * fb;
        w.sr[i] += dh * (y1 * ca + y2 * cb);
        w.si[i] += dh * (y1 * sa + y2 * sb);
        w.s1[i] += dh * (y1 * ga + y2 * gb);
      }
    }
    finishWindow() {
      const c = this.cur, w = c.win; const Tw = w.te - w.ts; const scale = 2 / Tw;
      // remove a linear trend (slow transients) from each signal before using its Fourier coefficient
      // joint least-squares fit of  a + b(t-tm) + c cos + d sin  (orthogonality of the sinusoids assumed)
      const q = Tw * Tw * Tw / 12, den = q - (2 / Tw) * (w.gr * w.gr + w.gi * w.gi);
      for (let i = 0; i < w.sr.length; i++) {
        const b = den > 0 ? (w.s1[i] - (2 / Tw) * (w.gr * w.sr[i] + w.gi * w.si[i])) / den : 0;
        w.sr[i] -= b * w.gr; w.si[i] -= b * w.gi;
      }
      const np = this.A.acProbes.length; const vals = [];
      // injection phasor: amp*sin -> -j*amp
      for (let i = 0; i < np; i++) {
        const p = this.A.acProbes[i];
        const Ar = w.sr[2 * i] * scale, Ai = w.si[2 * i] * scale, Br = w.sr[2 * i + 1] * scale, Bi = w.si[2 * i + 1] * scale;
        let hr, hi;
        if (p.loop) {
          // H = V(other)/V(dot)  -- values: [dot, other]
          const d = Ar * Ar + Ai * Ai;
          if (d === 0) { hr = NaN; hi = NaN; } else { hr = (Br * Ar + Bi * Ai) / d; hi = (Bi * Ar - Br * Ai) / d; }
          if (p.neg) { hr = -hr; hi = -hi; }
        } else {
          // Y / (-j amp) = j Y / amp
          hr = -Ai / c.amp; hi = Ar / c.amp;
        }
        vals.push([hr, hi]);
      }
      return vals;
    }
    run(ms) {
      const t0 = Date.now(); const dt = this.sc.dt;
      if (this.phase === 'pre') {
        if (!this.started) { this.src.setInj(0, 1, 0); this.A.init(); if (this.useB) { this.srcB.setInj(0, 1, 0); this.B.init(); } this.started = true; }
        while (this.n < this.preN) {
          this.n++; this.A.advanceTo(this.n * dt);
          if ((this.n & 63) === 0 && Date.now() - t0 > ms) return false;
        }
        this.snap = this.A.saveFull();
        this.phase = 'sweep'; this.fi = 0; this.startFreq();
      }
      while (this.fi < this.freqs.length) {
        const c = this.cur;
        for (let k = 0; k < 64; k++) {
          const tn = this.A.t + dt;
          // keep the grid aligned to integer multiples of dt
          const tg = Math.round(tn / dt) * dt;
          this.A.advanceTo(tg); if (this.useB) this.B.advanceTo(tg);
          const y = this.probeVals();
          this.accum(this.prevT, this.prevY, tg, y);
          this.prevT = tg; this.prevY = y;
          c.cyc = (tg - c.t0) / c.T;
          if (tg >= c.win.te - 1e-12 * c.T) {
            const vals = this.finishWindow();
            let conv = false;
            if (c.last) {
              conv = vals.every((v, i) => {
                const a = c.last[i]; const ma = Math.hypot(a[0], a[1]), mb = Math.hypot(v[0], v[1]);
                if (!(ma > 0) || !(mb > 0)) return ma === mb;
                const dm = Math.abs(mb / ma - 1); const dp = Math.abs(Math.atan2(a[0] * v[1] - a[1] * v[0], a[0] * v[0] + a[1] * v[1]));
                return dm < (this.ac.tol || 1e-3) && dp < (this.ac.tol || 1e-3);
              });
            }
            c.last = vals;
            const over = c.cyc + c.nm > c.maxCyc;
            if (conv || over) {
              if (!conv) this.warn.push('頻率 ' + fmtF(c.f) + ' 在 ' + Math.round(c.cyc) + ' 個週期內未達穩態，結果可能有誤差。');
              vals.forEach((v, i) => { this.res[i].re.push(v[0]); this.res[i].im.push(v[1]); });
              this.fDone.push(c.f);
              this.fi++;
              if (this.fi < this.freqs.length) this.startFreq();
              break;
            }
            this.newWindow(c.win.te);
          }
        }
        if (Date.now() - t0 > ms) return this.fi >= this.freqs.length;
      }
      return true;
    }
    result() { return makeBode(this.fDone, this.res, this.warn.concat(this.A.warnings)); }
  }
  S.ACInjectRunner = ACInjectRunner;

  function fmtF(f) { return f >= 1e6 ? (f / 1e6).toPrecision(4) + ' MHz' : f >= 1e3 ? (f / 1e3).toPrecision(4) + ' kHz' : f.toPrecision(4) + ' Hz'; }

  function makeBode(freqs, res, warnings) {
    const curves = res.map(r => {
      const mag = [], ph = [];
      let prev = null;
      for (let i = 0; i < r.re.length; i++) {
        const m = Math.hypot(r.re[i], r.im[i]);
        mag.push(20 * Math.log10(m));
        let p = Math.atan2(r.im[i], r.re[i]) * 180 / Math.PI;
        if (prev !== null) { while (p - prev > 180) p -= 360; while (p - prev < -180) p += 360; }
        ph.push(p); prev = p;
      }
      // keep the phase near the principal range at the first point
      if (ph.length) { const k = Math.round(ph[0] / 360); if (k) for (let i = 0; i < ph.length; i++) ph[i] -= 360 * k; }
      return { name: r.name, mag, ph, re: r.re.slice(), im: r.im.slice(), loop: !!r.loop };
    });
    return { kind: 'ac', freqs: freqs.slice(), curves, warnings };
  }

  // Linear frequency-domain AC analysis
  class ACLinearRunner {
    constructor(net, sc, ac) {
      this.ac = ac;
      this.sim = new Sim(net, { dt: sc.dt });
      const sim = this.sim;
      if (!sim.acProbes.length) throw new SimError('AC 掃頻：電路中沒有 AC Sweep 探棒。');
      this.src = sim.findSource(ac.src);
      if (!this.src || !this.src.setInj) throw new SimError('AC 掃頻：找不到名稱為「' + ac.src + '」的正弦波激勵源。');
      for (const m of sim.pEls) if (!m.stampAC) throw new SimError(m.el.name + '：此元件為非線性或切換元件，無法使用線性頻域分析，請改用小訊號注入法（Method = 0）。', m.el.id);
      for (const b of sim.cBlks) if (!b.stampAC) throw new SimError(b.el.name + '：此控制元件為非線性或離散元件，無法使用線性頻域分析，請改用小訊號注入法（Method = 0）。', b.el.id);
      this.freqs = sweepFreqs(ac); this.i = 0;
      this.N = sim.n + sim.nC - 1;
      this.res = sim.acProbes.map(p => ({ name: p.pname, re: [], im: [], loop: p.loop }));
      sim.epsT = sc.dt * 1e-6;
      for (const m of sim.pEls) if (m.init) m.init(sim);
      for (const b of sim.cBlks) if (b.init) b.init(sim);
    }
    progress() { return this.i / this.freqs.length; }
    run(ms) {
      const t0 = Date.now(); const sim = this.sim, N = this.N, n = sim.n;
      const Ar = new Float64Array(N * N), Ai = new Float64Array(N * N), br = new Float64Array(N), bi = new Float64Array(N), xr = new Float64Array(N), xi = new Float64Array(N);
      const ci = c => c > 0 ? n + c - 1 : -1; // control node index in complex system
      while (this.i < this.freqs.length) {
        const f = this.freqs[this.i], w = 2 * Math.PI * f;
        Ar.fill(0); Ai.fill(0); br.fill(0); bi.fill(0);
        const C = {
          N, w, ci, src: this.src,
          a(i, j, re, im) { if (i >= 0 && j >= 0) { Ar[i * N + j] += re; Ai[i * N + j] += im || 0; } },
          b(i, re, im) { if (i >= 0) { br[i] += re; bi[i] += im || 0; } },
          g(a, b2, re, im) { this.a(a, a, re, im); this.a(b2, b2, re, im); this.a(a, b2, -re, -(im || 0)); this.a(b2, a, -re, -(im || 0)); }
        };
        for (const m of sim.pEls) m.stampAC(C, sim);
        for (const b of sim.cBlks) b.stampAC(C, sim);
        for (const t of sim.ties) C.a(t, t, 1e-3, 0);
        // undriven control nodes -> 0
        for (let c = 1; c < sim.nC; c++) if (!sim.drv[c]) C.a(ci(c), ci(c), 1, 0);
        const lu = new CLU(N);
        if (!lu.factor(Ar, Ai)) throw new SimError('AC 分析矩陣奇異（f=' + fmtF(f) + '）。');
        lu.solve(br, bi, xr, xi);
        sim.acProbes.forEach((p, k) => {
          const v = p.acPhasor(xr, xi, ci, sim);
          let hr, hi;
          if (p.loop) {
            const [Ar_, Ai_, Br_, Bi_] = v; const d = Ar_ * Ar_ + Ai_ * Ai_;
            hr = (Br_ * Ar_ + Bi_ * Ai_) / d; hi = (Bi_ * Ar_ - Br_ * Ai_) / d;
            if (p.neg) { hr = -hr; hi = -hi; }
          } else {
            // excitation phasor -j  => H = Y / (-j) = j Y
            hr = -v[1]; hi = v[0];
          }
          this.res[k].re.push(hr); this.res[k].im.push(hi);
        });
        this.i++;
        if (Date.now() - t0 > ms) return this.i >= this.freqs.length;
      }
      return true;
    }
    result() { return makeBode(this.freqs.slice(0, this.i), this.res, this.sim.warnings.filter(w => !/代數迴路/.test(w))); }
  }
  S.ACLinearRunner = ACLinearRunner;

  /* ------------------------------------------------------------------
   * 10. Common waveform helpers used by source models
   * ------------------------------------------------------------------ */
  // next multiple of period after t of phase offset ph (times ph + k*T)
  function nextPeriodic(t, T, offs) {
    // offs: array of offsets within [0,T)
    let best = Infinity;
    const k0 = Math.floor(t / T);
    for (let k = k0; k <= k0 + 1; k++) for (const o of offs) { const tt = k * T + o; if (tt > t && tt < best) best = tt; }
    return best;
  }
  S.nextPeriodic = nextPeriodic;

  // simple deterministic PRNG for random sources
  S.rng = function (seed) { let s = (seed >>> 0) || 0x9e3779b9; return function () { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s / 4294967296; }; };

})(SIM);
if (typeof module !== 'undefined') module.exports = SIM;

/* =====================================================================
 * Sparse LU for circuit matrices (dense-stamped A, sparse factorization)
 *
 * The simulator stamps into a dense n×n Float64Array (simple and fast for
 * the elements). Circuit matrices are very sparse, so the factorization
 * works on the structural pattern only:
 *   - symbolic phase (rare): Markowitz pivot selection with threshold
 *     partial pivoting on the actual values, recording the row/column
 *     permutation, the fill-in and a flat list of elimination operations;
 *   - numeric phase (every factorization): gather the pattern values and
 *     replay the recorded operations — O(nnz(L+U)) instead of O(n³).
 * Each numeric factorization checks every pivot against the column
 * (|pivot| ≥ 1e-3·max), and asks for a new symbolic phase when a pivot
 * has become too small, so the result is as accurate as partial pivoting.
 * The solution is polished with one step of iterative refinement by the
 * caller (sparse residual), which removes the remaining rounding error.
 * ===================================================================== */
'use strict';
(function (S) {
  const PIV_SEL = 1e-2;   // threshold for choosing a pivot (symbolic phase)
  const PIV_OK = 1e-3;    // threshold for keeping a recorded pivot (numeric phase)

  // pattern of the stamped matrix: linear indices i*n+j that may be nonzero
  class Pattern {
    constructor(n) { this.n = n; this.mask = new Uint8Array(n * n); this.list = []; this.ver = 0; this.dirty = false; }
    add(k) { if (!this.mask[k]) { this.mask[k] = 1; this.list.push(k); this.dirty = true; } }
    // all positions that are nonzero now (generic use: a matrix with no stamping history)
    scan(A) { const n = this.n; for (let k = 0; k < n * n; k++) if (A[k] !== 0) this.add(k); }
    commit() { if (this.dirty) { this.dirty = false; this.ver++; } }
  }
  S.Pattern = Pattern;

  class LU {
    constructor(n, sym) { this.n = n; this.sym = null; this.bad = -1; this.v = null; this.av = null; if (sym) this.useSym(sym); }
    // share a symbolic analysis made for another factorization of the same pattern (the pivot check guards it)
    useSym(sym) { this.sym = sym; this.v = new Float64Array(sym.ns); this.y = new Float64Array(this.n); this.xq = new Float64Array(this.n); }

    /* ---------- symbolic phase: Markowitz with threshold pivoting on the dense values ---------- */
    symbolic(A, pat) {
      const n = this.n; const st = new Uint8Array(n * n); const a = Float64Array.from(A);
      for (let i = 0; i < pat.list.length; i++) st[pat.list[i]] = 1;
      // row / column structural counts over the active submatrix
      const rc = new Int32Array(n), cc = new Int32Array(n), sc = new Float64Array(n);
      for (let i = 0; i < n; i++) { let m = 0; for (let j = 0; j < n; j++) { const k = i * n + j; if (st[k]) { rc[i]++; cc[j]++; const v = Math.abs(a[k]); if (v > m) m = v; } } if (m === 0) { this.bad = i; return false; } sc[i] = 1 / m; }
      const rowAct = new Uint8Array(n).fill(1), colAct = new Uint8Array(n).fill(1);
      const p = new Int32Array(n), q = new Int32Array(n);
      for (let k = 0; k < n; k++) {
        // candidate: smallest Markowitz count among entries passing the threshold test in their column
        let bi = -1, bj = -1, bm = Infinity, bv = 0;
        for (let j = 0; j < n; j++) {
          if (!colAct[j]) continue;
          let cm = 0; for (let i = 0; i < n; i++) if (rowAct[i] && st[i * n + j]) { const v = Math.abs(a[i * n + j]); if (v > cm) cm = v; }
          if (cm === 0) continue;
          for (let i = 0; i < n; i++) {
            if (!rowAct[i] || !st[i * n + j]) continue;
            const v = Math.abs(a[i * n + j]); if (v < PIV_SEL * cm || !(v * sc[i] > 1e-15)) continue;
            const m = (rc[i] - 1) * (cc[j] - 1);
            if (m < bm || (m === bm && v > bv)) { bm = m; bv = v; bi = i; bj = j; }
          }
        }
        if (bi < 0) { for (let i = 0; i < n; i++) if (rowAct[i]) { this.bad = i; break; } return false; }
        p[k] = bi; q[k] = bj; rowAct[bi] = 0; colAct[bj] = 0;
        const piv = a[bi * n + bj];
        for (let i = 0; i < n; i++) {
          if (!rowAct[i] || !st[i * n + bj]) continue;
          const f = a[i * n + bj] / piv; a[i * n + bj] = f; rc[i]--;
          for (let j = 0; j < n; j++) {
            if (!colAct[j] || !st[bi * n + j]) continue;
            const kk = i * n + j;
            if (!st[kk]) { st[kk] = 1; rc[i]++; cc[j]++; }
            a[kk] -= f * a[bi * n + j];
          }
        }
        cc[bj] = 0; for (let j = 0; j < n; j++) if (colAct[j] && st[bi * n + j]) cc[j]--;
      }
      // slots: one per structural entry of P·A·Q (L and U share the storage), in permuted coordinates
      const pr = new Int32Array(n), qc = new Int32Array(n); // original row -> permuted index, original col -> permuted index
      for (let k = 0; k < n; k++) { pr[p[k]] = k; qc[q[k]] = k; }
      const slot = new Int32Array(n * n).fill(-1); let ns = 0;
      for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) if (st[p[r] * n + q[c]]) slot[r * n + c] = ns++;
      // gather map: pattern list entry -> slot
      const gather = new Int32Array(pat.list.length);
      for (let e = 0; e < pat.list.length; e++) { const k = pat.list[e]; const i = (k / n) | 0, j = k - i * n; gather[e] = slot[pr[i] * n + qc[j]]; }
      // elimination operations per pivot k: for each row r > k with L(r,k): [slot(r,k), slot(k,k), m, (slot(r,j), slot(k,j)) × m]
      const ops = []; const colStart = new Int32Array(n + 1); const diag = new Int32Array(n);
      for (let k = 0; k < n; k++) {
        diag[k] = slot[k * n + k]; colStart[k] = ops.length;
        const uj = []; for (let j = k + 1; j < n; j++) if (slot[k * n + j] >= 0) uj.push(j);
        for (let r = k + 1; r < n; r++) {
          const sl = slot[r * n + k]; if (sl < 0) continue;
          ops.push(sl, uj.length);
          for (const j of uj) ops.push(slot[r * n + j], slot[k * n + j]);
        }
      }
      colStart[n] = ops.length;
      // solve lists: L row entries (j < i) and U row entries (j > i)
      const lp = new Int32Array(n + 1), up = new Int32Array(n + 1); const lj = [], ls = [], uj2 = [], us = [];
      for (let i = 0; i < n; i++) {
        lp[i] = lj.length; for (let j = 0; j < i; j++) { const s = slot[i * n + j]; if (s >= 0) { lj.push(j); ls.push(s); } }
        up[i] = uj2.length; for (let j = i + 1; j < n; j++) { const s = slot[i * n + j]; if (s >= 0) { uj2.push(j); us.push(s); } }
      }
      lp[n] = lj.length; up[n] = uj2.length;
      this.sym = { ver: pat.ver, ns, p, q, gather, ops: Int32Array.from(ops), colStart, diag, lp, lj: Int32Array.from(lj), ls: Int32Array.from(ls), up, uj: Int32Array.from(uj2), us: Int32Array.from(us), nnz: pat.list.length };
      this.v = new Float64Array(ns); this.y = new Float64Array(n); this.xq = new Float64Array(n); this.symCount = (this.symCount || 0) + 1;
      return true;
    }

    /* ---------- numeric phase ---------- */
    // pat: Pattern of A (committed). Returns false when singular (this.bad = offending row).
    factor(A, pat) {
      const n = this.n; this.bad = -1;
      if (!pat) { pat = this._own || (this._own = new Pattern(n)); pat.scan(A); pat.commit(); }
      if (!this.sym || this.sym.ver !== pat.ver || this.sym.nnz !== pat.list.length) { if (!this.symbolic(A, pat)) return false; }
      for (let attempt = 0; attempt < 2; attempt++) {
        const sy = this.sym, v = this.v, list = pat.list, g = sy.gather;
        v.fill(0);
        for (let e = 0; e < list.length; e++) v[g[e]] += A[list[e]];
        const ops = sy.ops, cs = sy.colStart, diag = sy.diag;
        let ok = true, minPiv = Infinity;
        for (let k = 0; k < n; k++) {
          const piv = v[diag[k]]; const ap = Math.abs(piv); if (ap < minPiv) minPiv = ap;
          // pivot check against the column below (the entries partial pivoting would consider)
          let cm = ap; let o = cs[k]; const oe = cs[k + 1];
          while (o < oe) { const av = Math.abs(v[ops[o]]); if (av > cm) cm = av; o += 2 + 2 * ops[o + 1]; }
          if (!(ap >= PIV_OK * cm) || cm === 0) { ok = false; this.bad = sy.p[k]; break; }
          const inv = 1 / piv; o = cs[k];
          while (o < oe) {
            const sl = ops[o], m = ops[o + 1]; const f = v[sl] * inv; v[sl] = f; o += 2;
            if (f !== 0) { const end = o + 2 * m; for (; o < end; o += 2) v[ops[o]] -= f * v[ops[o + 1]]; }
            else o += 2 * m;
          }
        }
        if (ok) { this.bad = -1; this.minPiv = minPiv; return true; }
        // a recorded pivot became too small: redo the pivot selection with the present values
        if (attempt === 0 && !this.symbolic(A, pat)) return false;
      }
      return false;
    }
    solve(b, x) {
      const n = this.n, sy = this.sym, v = this.v, y = this.y, xq = this.xq, p = sy.p, q = sy.q;
      const lp = sy.lp, lj = sy.lj, ls = sy.ls, up = sy.up, uj = sy.uj, us = sy.us, diag = sy.diag;
      for (let i = 0; i < n; i++) { let s = b[p[i]]; for (let o = lp[i], e = lp[i + 1]; o < e; o++) s -= v[ls[o]] * y[lj[o]]; y[i] = s; }
      for (let i = n - 1; i >= 0; i--) { let s = y[i]; for (let o = up[i], e = up[i + 1]; o < e; o++) s -= v[us[o]] * xq[uj[o]]; xq[i] = s / v[diag[i]]; }
      for (let i = 0; i < n; i++) x[q[i]] = xq[i];
    }
    // keep a compact copy of A's pattern values (for the residual after A has been overwritten)
    keep(A, pat) { const list = pat.list; const av = this.av && this.av.length === list.length ? this.av : (this.av = new Float64Array(list.length)); for (let e = 0; e < list.length; e++) av[e] = A[list[e]]; this.avPat = pat; }
    // r = b - A x using the kept values; returns true when any component is nonzero
    residual(b, x, r) {
      const n = this.n, list = this.avPat.list, av = this.av; r.set(b); let any = false;
      for (let e = 0; e < list.length; e++) { const k = list[e]; const i = (k / n) | 0; r[i] -= av[e] * x[k - i * n]; }
      for (let i = 0; i < n; i++) if (r[i] !== 0) { any = true; break; }
      return any;
    }
  }
  S.LU = LU;
})(SIM);

/* =====================================================================
 * 電路模擬器 — power-circuit models
 * Sign conventions
 *   - MNA rows for nodes: sum of currents leaving the node = injected current
 *   - branch variable current flows from the first terminal to the second
 *   - a current source drives current out of its "+" terminal into the
 *     external circuit (PSIM convention)
 * ===================================================================== */
'use strict';
(function (S) {
  const { aA, aG, aB, aI, aBr, aV } = S.st;
  const vx = S.vx;
  const M_ = S.MODELS;
  const RON = 1e-5, ROFF = 1e6; // PSIM: 10 µΩ on, 1 MΩ off
  const EPSV = 1e-9, EPSI = 1e-10;
  const TWO_PI = 2 * Math.PI;

  /* ------------------------------------------------------------------
   * Basic linear branches
   * ------------------------------------------------------------------ */
  class Res {
    constructor(sim, a, b, R) { this.isP = true; this.sim = sim; this.a = a; this.b = b; this.R = R; if (R === 0) this.wantI = true; }
    setup(sim) { if (this.wantI) sim.var(this, 'k'); }
    conn() { return [[this.a, this.b]]; }
    stampG(M) {
      if (this.wantI) { aBr(M, this.a, this.b, this.k); aV(M, this.k, this.a, this.b, 1); aA(M, this.k, this.k, -this.R); }
      else aG(M, this.a, this.b, 1 / this.R);
    }
    current(x) { return this.wantI ? x[this.k] : (vx(x, this.a) - vx(x, this.b)) / this.R; }
    stampAC(C) {
      if (this.wantI) { C.a(this.a, this.k, 1); C.a(this.b, this.k, -1); C.a(this.k, this.a, 1); C.a(this.k, this.b, -1); C.a(this.k, this.k, -this.R); }
      else C.g(this.a, this.b, 1 / this.R, 0);
    }
  }
  S.Res = Res;

  class Cap {
    constructor(sim, a, b, C, v0) { this.isP = true; this.sim = sim; this.a = a; this.b = b; this.C = C; this.v0 = v0 || 0; this.initVar = C > 0; sim.reqP(this, 's', 2); }
    setup(sim) { if (this.wantI) sim.var(this, 'k'); }
    conn() { return this.C > 0 ? [[this.a, this.b]] : []; }
    init() { this.s[0] = this.v0; this.s[1] = 0; }
    g(h, be) { return (be ? 1 : 2) * this.C / h; }
    ieq(h, be) { const g = this.g(h, be); return be ? g * this.s[0] : g * this.s[0] + this.s[1]; }
    stampG(M, h, be) {
      if (!(this.C > 0)) return;
      const g = this.g(h, be);
      if (this.wantI) { aBr(M, this.a, this.b, this.k); aA(M, this.k, this.k, 1); aA(M, this.k, this.a, -g); aA(M, this.k, this.b, g); }
      else aG(M, this.a, this.b, g);
    }
    stampB(b, t, h, be) {
      if (!(this.C > 0)) return;
      const I = this.ieq(h, be);
      if (this.wantI) b[this.k] -= I; else aI(b, this.a, this.b, -I);
    }
    stampInitG(M) {
      if (!(this.C > 0)) return;
      const k = this.wantI ? this.k : this._iv;
      aBr(M, this.a, this.b, k); aV(M, k, this.a, this.b, 1);
    }
    stampInitB(b) { if (!(this.C > 0)) return; b[this.wantI ? this.k : this._iv] += this.v0; }
    accept(x, t, h, be) {
      if (!(this.C > 0)) return;
      const v = vx(x, this.a) - vx(x, this.b); const g = this.g(h, be);
      const i = be ? g * (v - this.s[0]) : g * (v - this.s[0]) - this.s[1];
      this.s[0] = v; this.s[1] = i;
    }
    acceptInit(x, sim) { if (!(this.C > 0)) return; const k = this.wantI ? this.k : this._iv; this.s[0] = this.v0; this.s[1] = sim.initCurrents ? sim.initCurrents[k] : 0; }
    current(x) { return this.wantI ? x[this.k] : NaN; }
    stampAC(C) {
      if (!(this.C > 0)) { if (this.wantI) { C.a(this.k, this.k, 1); } return; }
      const w = C.w;
      if (this.wantI) { C.a(this.a, this.k, 1); C.a(this.b, this.k, -1); C.a(this.k, this.k, 1); C.a(this.k, this.a, 0, -w * this.C); C.a(this.k, this.b, 0, w * this.C); }
      else C.g(this.a, this.b, 0, w * this.C);
    }
  }
  S.Cap = Cap;

  // Series R-L-C branch with a branch-current variable (any of R, L, C may be 0 = absent)
  class SerBr {
    constructor(sim, a, b, R, L, C, i0, vc0) {
      this.isP = true; this.sim = sim; this.a = a; this.b = b; this.R = R || 0; this.L = L || 0; this.C = C || 0; this.i0 = i0 || 0; this.vc0 = vc0 || 0;
      sim.var(this, 'k'); sim.reqP(this, 's', 3);
    }
    conn() { return [[this.a, this.b]]; }
    init() { this.s[0] = this.L ? this.i0 : 0; this.s[1] = 0; this.s[2] = this.vc0; }
    req(h, be) { let r = this.R; if (this.L) r += (be ? 1 : 2) * this.L / h; if (this.C) r += (be ? 1 : 0.5) * h / this.C; return r; }
    stampG(M, h, be) { aBr(M, this.a, this.b, this.k); aV(M, this.k, this.a, this.b, 1); aA(M, this.k, this.k, -this.req(h, be)); }
    stampB(b, t, h, be) {
      const s = this.s; let v = 0;
      if (this.L) v += be ? -(this.L / h) * s[0] : -(2 * this.L / h) * s[0] - s[1];
      if (this.C) v += be ? s[2] : s[2] + (h / (2 * this.C)) * s[0];
      b[this.k] += v;
    }
    // t = 0: inductor = current source i0 (relaxed pass: an inductor with i0 = 0 is a DC short), capacitor = voltage source vc0
    initAsI(relax) { return this.L && !(relax && this.i0 === 0); }
    stampInitG(M, relax) {
      aBr(M, this.a, this.b, this.k);
      if (this.initAsI(relax)) aA(M, this.k, this.k, 1);
      else { aV(M, this.k, this.a, this.b, 1); aA(M, this.k, this.k, -this.R); }
    }
    stampInitB(b, relax) { if (this.initAsI(relax)) b[this.k] += this.i0; else if (this.C) b[this.k] += this.vc0; }
    accept(x, t, h, be) {
      const s = this.s, iN = x[this.k], iP = s[0];
      if (this.L) s[1] = be ? (this.L / h) * (iN - iP) : (2 * this.L / h) * (iN - iP) - s[1];
      if (this.C) s[2] = be ? s[2] + (h / this.C) * iN : s[2] + (h / (2 * this.C)) * (iN + iP);
      s[0] = iN;
    }
    acceptInit(x) {
      const s = this.s, i = x[this.k]; s[0] = i;
      if (this.L) s[1] = vx(x, this.a) - vx(x, this.b) - this.R * i - (this.C ? s[2] : 0);
    }
    current(x) { return x[this.k]; }
    stampAC(C) {
      const w = C.w; let xi = 0; if (this.L) xi += w * this.L; if (this.C) xi -= 1 / (w * this.C);
      C.a(this.a, this.k, 1); C.a(this.b, this.k, -1); C.a(this.k, this.a, 1); C.a(this.k, this.b, -1); C.a(this.k, this.k, -this.R, -xi);
    }
  }
  S.SerBr = SerBr;

  // Coupled inductors (any number of windings)
  class MutL {
    constructor(sim, wins, Lm, i0) {
      this.isP = true; this.sim = sim; this.w = wins; this.L = Lm; this.i0 = i0; this.m = wins.length;
      this.k = new Int32Array(this.m); this.ks = [];
      for (let j = 0; j < this.m; j++) { const o = {}; this.ks.push(o); sim.var(o, 'k'); }
      sim.reqP(this, 's', 2 * this.m);
    }
    conn() { return this.w.map(p => [p[0], p[1]]); }
    init() { this.ks.forEach((o, j) => { this.k[j] = o.k; }); for (let j = 0; j < this.m; j++) { this.s[j] = this.i0[j] || 0; this.s[this.m + j] = 0; } }
    stampG(M, h, be) {
      const c = (be ? 1 : 2) / h;
      for (let j = 0; j < this.m; j++) {
        const [a, b] = this.w[j], k = this.k[j];
        aBr(M, a, b, k); aV(M, k, a, b, 1);
        for (let l = 0; l < this.m; l++) aA(M, k, this.k[l], -c * this.L[j][l]);
      }
    }
    stampB(b, t, h, be) {
      const c = (be ? 1 : 2) / h, m = this.m, s = this.s;
      for (let j = 0; j < m; j++) { let v = 0; for (let l = 0; l < m; l++) v -= c * this.L[j][l] * s[l]; if (!be) v -= s[m + j]; b[this.k[j]] += v; }
    }
    stampInitG(M) { for (let j = 0; j < this.m; j++) { const [a, b] = this.w[j]; aBr(M, a, b, this.k[j]); aA(M, this.k[j], this.k[j], 1); } }
    stampInitB(b) { for (let j = 0; j < this.m; j++) b[this.k[j]] += this.i0[j] || 0; }
    acceptInit(x) { for (let j = 0; j < this.m; j++) { const [a, b] = this.w[j]; this.s[j] = x[this.k[j]]; this.s[this.m + j] = vx(x, a) - vx(x, b); } }
    accept(x, t, h, be) {
      const c = (be ? 1 : 2) / h, m = this.m, s = this.s;
      const v = new Float64Array(m);
      for (let j = 0; j < m; j++) { let a = 0; for (let l = 0; l < m; l++) a += c * this.L[j][l] * (x[this.k[l]] - s[l]); v[j] = be ? a : a - s[m + j]; }
      for (let j = 0; j < m; j++) { s[m + j] = v[j]; s[j] = x[this.k[j]]; }
    }
    stampAC(C) {
      for (let j = 0; j < this.m; j++) {
        const [a, b] = this.w[j], k = this.k[j];
        C.a(a, k, 1); C.a(b, k, -1); C.a(k, a, 1); C.a(k, b, -1);
        for (let l = 0; l < this.m; l++) C.a(k, this.k[l], 0, -C.w * this.L[j][l]);
      }
    }
  }

  // Ideal multi-winding transformer. wins: [[a,b,N],...]; winding 0 is the reference.
  class Xfmr {
    constructor(sim, wins) {
      this.isP = true; this.w = wins; this.ks = [];
      for (let j = 1; j < wins.length; j++) { const o = {}; this.ks.push(o); sim.var(o, 'k'); }
    }
    conn() { return this.w.map(p => [p[0], p[1]]); }
    st(add) {
      const [a1, b1, N1] = this.w[0];
      for (let j = 1; j < this.w.length; j++) {
        const [a, b, N] = this.w[j], k = this.ks[j - 1].k, r = N / N1;
        add(a, k, 1); add(b, k, -1); add(a1, k, -r); add(b1, k, r);
        add(k, a, 1); add(k, b, -1); add(k, a1, -r); add(k, b1, r);
      }
    }
    stampG(M) { this.st((i, j, v) => aA(M, i, j, v)); }
    stampAC(C) { this.st((i, j, v) => C.a(i, j, v)); }
  }

  /* ------------------------------------------------------------------
   * Generic PWL switch
   *   type: D (diode), ZEN, DIAC, THY, TRIAC, GTO (gated unidirectional),
   *         SSW (gated bidirectional), FIX (time-scheduled)
   *   state 0 = off, 1 = forward on, 2 = reverse on
   * ------------------------------------------------------------------ */
  class Sw {
    constructor(sim, a, k, o) {
      this.isP = true; this.sw = true; this.sim = sim; this.a = a; this.k = k;
      this.type = o.type; this.ron = Math.max(o.ron || RON, 1e-9); this.roff = o.roff || ROFF;
      this.vf = o.vf || 0; this.vb = o.vb || 0; this.vbb = o.vbb || 0; this.ih = o.ih || 0; this.il = o.il || 0;
      this.gn = (o.gn === undefined) ? -1 : o.gn; this.ginv = !!o.ginv; this.init0 = o.init || 0;
      this.sched = o.sched || null;
      sim.reqP(this, 's', 4); this.gIdx = 1; this.fIdx = 2;
      if (this.gn >= 0) this.cins = [this.gn];
    }
    conn() { return [[this.a, this.k]]; }
    init() { this.s[0] = this.init0; this.s[1] = 0; this.s[2] = -1; this.s[3] = 0; }
    state() { return this.s[0]; }
    setState(v) { this.s[0] = v; if (v === 0) this.s[3] = 0; }
    skey() { return this.s[0]; }
    v(x) { return vx(x, this.a) - vx(x, this.k); }
    v0() { const st = this.s[0]; return st === 1 ? this.fwdDrop() : (st === 2 ? -this.revDrop() : 0); }
    fwdDrop() { return this.type === 'DIAC' ? this.vbb : this.vf; }
    revDrop() { return this.type === 'ZEN' ? this.vb : (this.type === 'DIAC' ? this.vbb : this.vf); }
    g() { return this.s[0] ? 1 / this.ron : 1 / this.roff; }
    i(x) { return this.g() * (this.v(x) - this.v0()); }
    stampG(M) { aG(M, this.a, this.k, this.g()); }
    stampB(b) { const V0 = this.v0(); if (V0) { const g = this.g(); aI(b, this.a, this.k, -g * V0); } }
    eg() { const g = this.s[1]; return this.ginv ? 1 - g : g; }
    accept(x) { if (this.type === 'THY' && this.s[0] === 1 && this.i(x) >= this.il) this.s[3] = 1; }
    desired(x) {
      const st = this.s[0], v = this.v(x);
      switch (this.type) {
        case 'D': if (st === 0) return v > this.vf + EPSV ? 1 : 0; return this.i(x) < -EPSI ? 0 : 1;
        case 'ZEN':
          if (st === 0) return v > this.vf + EPSV ? 1 : (-v > this.vb + EPSV ? 2 : 0);
          if (st === 1) return this.i(x) < -EPSI ? 0 : 1;
          return this.i(x) > EPSI ? 0 : 2;
        case 'DIAC':
          if (st === 0) return v > this.vb + EPSV ? 1 : (-v > this.vb + EPSV ? 2 : 0);
          if (st === 1) return this.i(x) < -EPSI ? 0 : 1;
          return this.i(x) > EPSI ? 0 : 2;
        case 'THY': {
          if (st === 0) return (this.eg() && v > this.vf + EPSV) ? 1 : 0;
          const i = this.i(x);
          if (i < this.ih - EPSI) return 0;
          if (!this.eg() && !this.s[3] && this.il > 0 && i < this.il) return 0;
          return 1;
        }
        case 'TRIAC':
          if (st === 0) { if (!this.eg()) return 0; return v > this.vf + EPSV ? 1 : (-v > this.vf + EPSV ? 2 : 0); }
          if (st === 1) return this.i(x) < -EPSI ? 0 : 1;
          return this.i(x) > EPSI ? 0 : 2;
        case 'GTO':
          if (!this.eg()) return 0;
          if (st === 0) return v > this.vf + EPSV ? 1 : 0;
          return this.i(x) < -EPSI ? 0 : 1;
        case 'SSW': return this.eg() ? 1 : 0;
        default: return st;
      }
    }
    // switching quantity (crosses zero at the transition to ns) at the two interval ends, or null
    qq(xA, xB, ns) {
      const st = this.s[0];
      if (this.type === 'SSW' || this.type === 'FIX') return null;
      if (st !== 0 && ns === 0) { const th = this.type === 'THY' ? this.ih : 0; return [this.i(xA) - th, this.i(xB) - th]; }
      if (st === 0 && ns === 1) { const th = this.type === 'DIAC' ? this.vb : this.vf; return [this.v(xA) - th, this.v(xB) - th]; }
      if (st === 0 && ns === 2) { const th = (this.type === 'ZEN' || this.type === 'DIAC') ? this.vb : this.vf; return [-this.v(xA) - th, -this.v(xB) - th]; }
      return null;
    }
    // fraction of the interval at which the transition to ns happened
    tau(xA, xB, ns) {
      const q = this.qq(xA, xB, ns); if (!q) return NaN;
      const [qa, qb] = q;
      if (qa === qb || (qa > 0) === (qb > 0)) return NaN;
      return qa / (qa - qb);
    }
    // gate change applied at the start of an interval. returns true when state changed.
    onGate(g, xP) {
      const eg = this.ginv ? 1 - g : g; const st = this.s[0];
      const v = this.v(xP);
      switch (this.type) {
        case 'SSW': if (st !== eg) { this.s[0] = eg; return true; } return false;
        case 'GTO':
          if (!eg && st) { this.setState(0); return true; }
          if (eg && !st && v > this.vf + EPSV) { this.s[0] = 1; return true; }
          return false;
        case 'THY':
          if (eg && !st && v > this.vf + EPSV) { this.s[0] = 1; return true; }
          return false;
        case 'TRIAC':
          if (eg && !st) { if (v > this.vf + EPSV) { this.s[0] = 1; return true; } if (-v > this.vf + EPSV) { this.s[0] = 2; return true; } }
          return false;
      }
      return false;
    }
    stampAC() { throw new S.SimError('開關元件無法做線性 AC 分析'); }
  }
  delete Sw.prototype.stampAC; // switches are not linear
  S.Sw = Sw;

  // Push-button / scheduled switch
  class TimedSw extends Sw {
    constructor(sim, a, b, init, times) { super(sim, a, b, { type: 'FIX', init }); this.times = times.slice().sort((p, q) => p - q); this.timed = true; }
    stateAt(t) { let s = this.init0; for (const tt of this.times) if (tt <= t) s = 1 - s; return s; }
    init() { super.init(); this.s[0] = this.stateAt(0); }
    nextBreak(t) { for (const tt of this.times) if (tt > t) return tt; return Infinity; }
    jumpAt(t) { return this.times.some(tt => Math.abs(tt - t) <= this.sim.epsT); }
    desired() { return this.s[0]; }
  }

  /* ------------------------------------------------------------------
   * Gate phase-shifter used by thyristor bridges (control block)
   * ------------------------------------------------------------------ */
  class GateShift {
    constructor(sim, ctx, ct, shifts) {
      this.isC = true; this.sim = sim; this.ctx = ctx; this.ct = ct; this.shifts = shifts;
      this.ins = [ct]; this.outs = shifts.map(() => sim.newCNode()); this.ft = true;
      this.edges = []; this.pend = null; this.v0 = 0;
    }
    init(sim) {
      const d = sim.drv[this.ct];
      if (!d || !(d.gateFreq > 0)) throw this.ctx.err('閘極（Ct）必須連接到 Gating Block 或 Alpha Controller，才能推算其他開關的觸發相位');
      this.T = 1 / d.gateFreq; this.del = this.shifts.map(s => s / 360 * this.T);
      this.edges = [[-Infinity, 0]];
    }
    valAt(t) { const e = this.edges; let v = e[0][1]; for (let i = e.length - 1; i >= 0; i--) if (e[i][0] <= t + this.sim.epsT) { v = e[i][1]; break; } return v; }
    eval(t, h, sim) {
      const g = sim.cv[this.ct] > 0.5 ? 1 : 0; const last = this.edges[this.edges.length - 1][1];
      this.pend = null;
      if (g !== last) { const e = sim.cev[this.ct]; this.pend = [isFinite(e) ? e : t, g]; }
      this.outs.forEach((o, k) => {
        const d = this.del[k];
        let v;
        if (d === 0) { v = g; sim.cev[o] = sim.cev[this.ct]; }
        else { v = this.valAt(t - d); const prev = sim.cvP[o]; sim.cev[o] = v !== prev ? t : NaN; }
        sim.cv[o] = v;
      });
    }
    commit(t) {
      if (this.pend) this.edges.push(this.pend);
      const keep = t - Math.max.apply(null, this.del) - 2 * this.T;
      while (this.edges.length > 2 && this.edges[1][0] < keep) this.edges.shift();
    }
    nextBreak(t) {
      let tb = Infinity;
      for (const d of this.del) { if (d === 0) continue; for (const e of this.edges) { const te = e[0] + d; if (te > t && te < tb) tb = te; } }
      return tb;
    }
    saveX() { return this.edges.map(e => e.slice()); }
    restoreX(x) { this.edges = x.map(e => e.slice()); }
  }

  /* ------------------------------------------------------------------
   * Sources (waveform objects shared by power and control sources)
   * ------------------------------------------------------------------ */
  function W_dc(V) { return { val: () => V, next: () => Infinity, jump: () => false, off: V }; }
  function W_sin(Vm, f, ph, off, ts) {
    const w = TWO_PI * f, p = ph * Math.PI / 180;
    const W = {
      inj: null, off,
      val(t) {
        if (this.inj) {
          const j = this.inj, u = t - j.t0;
          const env = (j.Tr > 0 && u < j.Tr) ? (u <= 0 ? 0 : 0.5 * (1 - Math.cos(Math.PI * u / j.Tr))) : 1;
          return off + j.A * env * Math.sin(TWO_PI * j.f * u);
        }
        if (t < ts) return 0; return Vm * Math.sin(w * t + p) + off;
      },
      next(t) { return (!this.inj && ts > t) ? ts : Infinity; },
      jump(t) { return !this.inj && ts > 0 && Math.abs(t - ts) <= W.eps && (Vm * Math.sin(w * ts + p) + off) !== 0; }
    };
    return W;
  }
  function frac(x) { return x - Math.floor(x); }
  function W_squ(Vpp, f, D, off, th) {
    const T = 1 / f, td = th / 360 * T;
    const e1 = frac(td / T) * T, e2 = frac((td + D * T) / T) * T;
    return {
      val(t) { const ph = frac((t - td) / T); return ph < D ? off + Vpp : off; },
      next(t) { return S.nextPeriodic(t, T, this._offs || (this._offs = [e1, e2])); },
      jump(t) { const r = frac(t / T) * T; const eps = this.eps; return [e1, e2].some(e => Math.abs(r - e) <= eps || Math.abs(r - e - T) <= eps || Math.abs(r - e + T) <= eps); }
    };
  }
  function W_tri(Vpp, f, D, off, th) {
    const T = 1 / f, td = th / 360 * T;
    const e1 = frac(td / T) * T, e2 = frac((td + D * T) / T) * T;
    const isJ = D <= 0 || D >= 1;
    return {
      val(t) {
        let ph = frac((t - td) / T);
        if (ph > 1 - 1e-12) ph = 0;
        if (D >= 1) return off + Vpp * ph;
        if (D <= 0) return off + Vpp * (1 - ph);
        return ph < D ? off + Vpp * ph / D : off + Vpp * (1 - ph) / (1 - D);
      },
      next(t) { return S.nextPeriodic(t, T, this._offs || (this._offs = [e1, e2])); },
      jump(t) { if (!isJ) return false; const r = frac(t / T) * T; return Math.abs(r - e1) <= this.eps || Math.abs(r - e1 - T) <= this.eps || r <= this.eps; }
    };
  }
  function W_saw(Vpk, f, off, th) { return W_tri(Vpk, f, 1, off, th); }
  function W_step(V, ts) { return { val: t => t < ts ? 0 : V, next: t => ts > t ? ts : Infinity, jump(t) { return Math.abs(t - ts) <= this.eps && V !== 0; } }; }
  function W_step2(V1, V2, ts, tr) {
    return {
      val(t) { if (t < ts) return V1; if (tr > 0 && t < ts + tr) return V1 + (V2 - V1) * (t - ts) / tr; return V2; },
      next(t) { if (ts > t) return ts; if (tr > 0 && ts + tr > t) return ts + tr; return Infinity; },
      jump(t) { return !(tr > 0) && Math.abs(t - ts) <= this.eps && V1 !== V2; }
    };
  }
  function W_pwl(f, ts, vs) {
    const n = ts.length; const T = f > 0 ? 1 / f : 0;
    const loc = t => (T > 0 ? frac(t / T) * T : t);
    return {
      val(t) {
        const u = loc(t);
        if (u <= ts[0]) return vs[0];
        for (let i = 1; i < n; i++) if (u < ts[i]) { const dt = ts[i] - ts[i - 1]; return dt > 0 ? vs[i - 1] + (vs[i] - vs[i - 1]) * (u - ts[i - 1]) / dt : vs[i]; }
        return vs[n - 1];
      },
      next(t) {
        if (T > 0) return S.nextPeriodic(t, T, ts.map(x => frac(x / T) * T));
        for (const x of ts) if (x > t) return x; return Infinity;
      },
      jump(t) {
        const u = loc(t);
        for (let i = 1; i < n; i++) if (ts[i] === ts[i - 1] && Math.abs(u - ts[i]) <= this.eps && vs[i] !== vs[i - 1]) return true;
        return false;
      }
    };
  }
  function W_rand(Vpp, off, seed, dt) {
    const hash = k => { let x = (k * 2654435761 + seed * 97) >>> 0; x ^= x >>> 16; x = Math.imul(x, 0x45d9f3b) >>> 0; x ^= x >>> 16; x = Math.imul(x, 0x45d9f3b) >>> 0; x ^= x >>> 16; return (x >>> 0) / 4294967296; };
    return { val: t => off + Vpp * hash(Math.ceil(t / dt - 1e-6)), next: () => Infinity, jump: () => false };
  }
  function W_math(fn, ts) { return { val: t => t < ts ? 0 : fn(t), next: t => ts > t ? ts : Infinity, jump(t) { return ts > 0 && Math.abs(t - ts) <= this.eps; } }; }
  S.W = { W_dc, W_sin, W_squ, W_tri, W_saw, W_step, W_step2, W_pwl, W_rand, W_math };

  // waveform from element params by type
  function makeWave(ctx, t) {
    const P = ctx.P, sim = ctx.sim;
    switch (t) {
      case 'DC': case 'BAT': case 'GDC': case 'GDCT': return W_dc(P.num('V'));
      case 'SIN': return W_sin(P.num('Vm'), P.num('f'), P.num('ph', 0), P.num('off', 0), P.num('ts', 0));
      case 'SQU': { const f = P.pos('f'); return W_squ(P.num('Vpp'), f, P.num('D', 0.5), P.num('off', 0), P.num('ph', 0)); }
      case 'TRI': { const f = P.pos('f'); const D = P.num('D', 0.5); if (D < 0 || D > 1) throw ctx.err('Duty Cycle 必須在 0 到 1 之間'); return W_tri(P.num('Vpp'), f, D, P.num('off', 0), P.num('ph', 0)); }
      case 'SAW': return W_saw(P.num('Vpk'), P.pos('f'), P.num('off', 0), P.num('ph', 0));
      case 'STEP': return W_step(P.num('V'), P.num('ts'));
      case 'STEP2': return W_step2(P.num('V1'), P.num('V2'), P.num('ts'), P.num('tr', 0));
      case 'PWL': {
        const vs = P.list('vals'), ts = P.list('times');
        if (vs.length !== ts.length || !vs.length) throw ctx.err('數值與時間的點數必須相同且至少一點');
        for (let i = 1; i < ts.length; i++) if (ts[i] < ts[i - 1]) throw ctx.err('時間必須遞增');
        return W_pwl(P.num('f', 0), ts, vs);
      }
      case 'PWL2': {
        const pr = P.pairs('pairs');
        for (let i = 1; i < pr.length; i++) if (pr[i][0] < pr[i - 1][0]) throw ctx.err('時間必須遞增');
        return W_pwl(P.num('f', 0), pr.map(q => q[0]), pr.map(q => q[1]));
      }
      case 'RAND': return W_rand(P.num('Vpp'), P.num('off', 0), P.int('seed', 1), sim.dt);
      case 'MATH': return W_math(P.fn('expr', ['t']), P.num('ts', 0));
    }
    throw ctx.err('未知的波形');
  }
  S.makeWave = makeWave;

  // Power voltage source
  class PVS {
    constructor(sim, p, n, W) { this.isP = true; this.sim = sim; this.p = p; this.n = n; this.W = W; sim.var(this, 'k'); W.eps = 0; }
    init(sim) { this.W.eps = sim.epsT; }
    conn() { return [[this.p, this.n]]; }
    stampG(M) { aBr(M, this.p, this.n, this.k); aV(M, this.k, this.p, this.n, 1); }
    stampB(b, t) { b[this.k] += this.W.val(t - 0.5 * this.sim.epsT); }
    stampInitB(b) { b[this.k] += this.W.val(0); }
    nextBreak(t) { return this.W.next(t); }
    jumpAt(t) { return this.W.jump(t); }
    setInj(A, f, t0, Tr) { this.W.inj = { A, f, t0, Tr: Tr || 0 }; }
    current(x) { return x[this.k]; }
    stampAC(C) { C.a(this.p, this.k, 1); C.a(this.n, this.k, -1); C.a(this.k, this.p, 1); C.a(this.k, this.n, -1); if (C.src === this) C.b(this.k, 0, -1); }
  }
  // Power current source (current leaves the + terminal into the circuit)
  class PIS {
    constructor(sim, p, n, W) { this.isP = true; this.sim = sim; this.p = p; this.n = n; this.W = W; W.eps = 0; }
    init(sim) { this.W.eps = sim.epsT; }
    conn() { return []; }
    stampB(b, t) { const I = this.W.val(t - 0.5 * this.sim.epsT); aB(b, this.p, I); aB(b, this.n, -I); }
    stampInitB(b) { const I = this.W.val(0); aB(b, this.p, I); aB(b, this.n, -I); }
    nextBreak(t) { return this.W.next(t); }
    jumpAt(t) { return this.W.jump(t); }
    setInj(A, f, t0, Tr) { this.W.inj = { A, f, t0, Tr: Tr || 0 }; }
    stampAC(C) { if (C.src === this) { C.b(this.p, 0, -1); C.b(this.n, 0, 1); } }
  }
  // Control-circuit voltage source (signal generator)
  class CSrc {
    constructor(sim, c, W) { this.isC = true; this.sim = sim; this.outs = [c]; this.ins = []; this.c = c; this.W = W; W.eps = 0; }
    init(sim) { this.W.eps = sim.epsT; }
    eval(t, h, sim) { const v = this.W.val(t + 0.5 * sim.epsT); sim.cv[this.c] = v; sim.cev[this.c] = this.W.jump(t) ? t : NaN; }
    nextBreak(t) { return this.W.next(t); }
    setInj(A, f, t0, Tr) { this.W.inj = { A, f, t0, Tr: Tr || 0 }; }
    stampAC(C) { const r = C.ci(this.c); C.a(r, r, 1); if (C.src === this) C.b(r, 0, -1); }
  }
  S.PVS = PVS; S.PIS = PIS; S.CSrc = CSrc;

  // Register voltage sources: pins [+ , -] (or single pin for grounded types)
  function regVSrc(type, wtype, single) {
    M_[type] = ctx => {
      const sim = ctx.sim;
      const W = makeWave(ctx, wtype);
      const nm = ctx.el.name;
      let m;
      if (single) {
        if (ctx.isCtl(0)) m = new CSrc(sim, ctx.c(0), W);
        else m = new PVS(sim, ctx.p(0), -1, W);
      } else {
        const k0 = ctx.kind(0), k1 = ctx.kind(1);
        if (k0 === 'C' || k1 === 'C') {
          if (k0 === 'C' && k1 === 'G') m = new CSrc(sim, ctx.c(0), W);
          else if (k1 === 'C' && k0 === 'G') { m = new CSrc(sim, ctx.c(1), W_neg(W)); }
          else throw ctx.err('在控制電路中使用電壓源時，負端必須接地');
        } else m = new PVS(sim, ctx.p(0), ctx.p(1), W);
      }
      if (wtype === 'SIN') m.srcName = nm;
      return m;
    };
  }
  function W_neg(W) { return { val: t => -W.val(t), next: t => W.next(t), jump: t => W.jump(t), set eps(v) { W.eps = v; }, get eps() { return W.eps; }, get inj() { return W.inj; }, set inj(v) { W.inj = v; } }; }
  [['VDC', 'DC'], ['VBAT', 'DC'], ['VSIN', 'SIN'], ['VSQU', 'SQU'], ['VTRI', 'TRI'], ['VSAW', 'SAW'], ['VSTEP', 'STEP'], ['VSTEP2', 'STEP2'], ['VPWL', 'PWL'], ['VPWL2', 'PWL2'], ['VRAND', 'RAND'], ['VMATH', 'MATH']].forEach(([t, w]) => regVSrc(t, w, false));
  regVSrc('VGDC', 'DC', true); regVSrc('VGDCT', 'DC', true);

  function regISrc(type, wtype) {
    M_[type] = ctx => {
      if (ctx.isCtl(0) || ctx.isCtl(1)) throw ctx.err('電流源只能用於功率電路');
      const m = new PIS(ctx.sim, ctx.p(0), ctx.p(1), makeWave(ctx, wtype));
      if (wtype === 'SIN') m.srcName = ctx.el.name;
      return m;
    };
  }
  [['IDC', 'DC'], ['ISIN', 'SIN'], ['ISQU', 'SQU'], ['ITRI', 'TRI'], ['ISTEP', 'STEP'], ['ISTEP2', 'STEP2'], ['IPWL', 'PWL'], ['IPWL2', 'PWL2'], ['IRAND', 'RAND'], ['IMATH', 'MATH']].forEach(([t, w]) => regISrc(t, w));

  // 3-phase sine (Y connected): pins [n, a, b, c]
  M_.VSIN3 = ctx => {
    const P = ctx.P, sim = ctx.sim;
    const Vll = P.num('V'), f = P.num('f'), th = P.num('ph', 0);
    const Vm = Vll * Math.SQRT2 / Math.sqrt(3);
    const n = ctx.p(0);
    return [0, -120, 120].map((d, i) => new PVS(sim, ctx.p(i + 1), n, W_sin(Vm, f, th + d, 0, 0)));
  };

  /* ------------------------------------------------------------------
   * RLC elements
   * ------------------------------------------------------------------ */
  M_.R = ctx => { const R = ctx.P.num('R'); if (R < 0) throw ctx.err('電阻不可為負值'); return new Res(ctx.sim, ctx.p(0), ctx.p(1), R); };
  M_.C = ctx => { const C = ctx.P.num('C'); if (!(C > 0)) throw ctx.err('電容必須大於 0'); return new Cap(ctx.sim, ctx.p(0), ctx.p(1), C, ctx.P.num('v0', 0)); };
  M_.L = ctx => { const L = ctx.P.num('L'); if (!(L > 0)) throw ctx.err('電感必須大於 0'); return new SerBr(ctx.sim, ctx.p(0), ctx.p(1), 0, L, 0, ctx.P.num('i0', 0), 0); };
  function brMk(hasR, hasL, hasC) {
    return ctx => {
      const P = ctx.P;
      const R = hasR ? P.num('R') : 0, L = hasL ? P.num('L') : 0, C = hasC ? P.num('C') : 0;
      if (R < 0 || L < 0 || C < 0) throw ctx.err('R、L、C 不可為負值');
      if (!R && !L && !C) throw ctx.err('R、L、C 不可全部為 0');
      return new SerBr(ctx.sim, ctx.p(0), ctx.p(1), R, L, C, hasL ? P.num('i0', 0) : 0, hasC ? P.num('v0', 0) : 0);
    };
  }
  M_.RL = brMk(1, 1, 0); M_.RC = brMk(1, 0, 1); M_.LC = brMk(0, 1, 1); M_.RLC = brMk(1, 1, 1);
  function br3(hasR, hasL, hasC) {
    return ctx => {
      const P = ctx.P; const R = hasR ? P.num('R') : 0, L = hasL ? P.num('L') : 0, C = hasC ? P.num('C') : 0;
      if (!R && !L && !C) throw ctx.err('R、L、C 不可全部為 0');
      return [0, 1, 2].map(i => (hasR && !hasL && !hasC) ? new Res(ctx.sim, ctx.p(i), ctx.p(i + 3), R) : new SerBr(ctx.sim, ctx.p(i), ctx.p(i + 3), R, L, C, 0, 0));
    };
  }
  M_.R3 = br3(1, 0, 0); M_.RL3 = br3(1, 1, 0); M_.RC3 = br3(1, 0, 1); M_.RLC3 = br3(1, 1, 1);
  M_.RHEO = ctx => {
    const R = ctx.P.pos('R'), tap = ctx.P.num('tap', 0.5);
    if (tap < 0 || tap > 1) throw ctx.err('Tap 位置必須在 0 到 1 之間');
    return [new Res(ctx.sim, ctx.p(0), ctx.p(2), R * tap), new Res(ctx.sim, ctx.p(2), ctx.p(1), R * (1 - tap))];
  };

  // Saturable inductor: table "i1 L1 i2 L2 ..." (L = lambda/i)
  class SatL {
    constructor(sim, a, b, ii, ll) {
      this.isP = true; this.nl = true; this.sim = sim; this.a = a; this.b = b;
      this.ip = [0]; this.lp = [0];
      for (let j = 0; j < ii.length; j++) { this.ip.push(Math.abs(ii[j])); this.lp.push(Math.abs(ii[j]) * ll[j]); }
      this.L0 = ll[0];
      sim.var(this, 'k'); sim.reqP(this, 's', 3);
    }
    conn() { return [[this.a, this.b]]; }
    init() { this.s.fill(0); }
    lam(i) { // flux linkage lambda(i) and slope
      const ai = Math.abs(i), sg = i < 0 ? -1 : 1, ip = this.ip, lp = this.lp, n = ip.length;
      let j = 1; while (j < n - 1 && ai > ip[j]) j++;
      const sl = (lp[j] - lp[j - 1]) / (ip[j] - ip[j - 1] || 1e-30);
      return [sg * (lp[j - 1] + sl * (ai - ip[j - 1])), Math.max(sl, 1e-15)];
    }
    stampG(M) { aBr(M, this.a, this.b, this.k); aV(M, this.k, this.a, this.b, 1); }
    stampNL(M, x, h, be) { const [, sl] = this.lam(x[this.k]); aA(M, this.k, this.k, -(be ? 1 : 2) * sl / h); }
    stampNLB(b, x, t, h, be) {
      const ig = x[this.k]; const [lg, sl] = this.lam(ig); const c = (be ? 1 : 2) / h;
      b[this.k] += c * (lg - sl * ig - this.s[0]) - (be ? 0 : this.s[1]);
    }
    stampInitG(M) { aBr(M, this.a, this.b, this.k); aA(M, this.k, this.k, 1); }
    acceptInit(x) { this.s[0] = this.lam(x[this.k])[0]; this.s[1] = vx(x, this.a) - vx(x, this.b); this.s[2] = x[this.k]; }
    accept(x, t, h, be) { const i = x[this.k]; const [l] = this.lam(i); const c = (be ? 1 : 2) / h; this.s[1] = c * (l - this.s[0]) - (be ? 0 : this.s[1]); this.s[0] = l; this.s[2] = i; }
  }
  M_.LSAT = ctx => {
    const lst = ctx.P.list('tab');
    if (lst.length < 2 || lst.length % 2) throw ctx.err('請以 i1 L1 i2 L2 ... 的成對格式輸入電流與電感');
    const ii = [], ll = []; for (let i = 0; i < lst.length; i += 2) { ii.push(lst[i]); ll.push(lst[i + 1]); }
    for (let i = 1; i < ii.length; i++) if (!(ii[i] > ii[i - 1])) throw ctx.err('電流必須遞增');
    if (!(ii[0] > 0)) throw ctx.err('第一個電流點必須大於 0');
    return new SatL(ctx.sim, ctx.p(0), ctx.p(1), ii, ll);
  };

  // Nonlinear conductance i = f(v)
  class NonI {
    constructor(sim, a, b, f, df, v0, lo, hi) { this.isP = true; this.nl = true; Object.assign(this, { sim, a, b, f, df, v0, lo, hi }); }
    conn() { return [[this.a, this.b]]; }
    vg(x) { return Math.min(this.hi, Math.max(this.lo, vx(x, this.a) - vx(x, this.b))); }
    stampNL(M, x) { aG(M, this.a, this.b, Math.max(this.df(this.vg(x)), 1e-12)); }
    stampNLB(b, x) { const v = this.vg(x); const g = Math.max(this.df(v), 1e-12); const I = this.f(v) - g * v; aI(b, this.a, this.b, I); }
    stampInitG(M) { aG(M, this.a, this.b, Math.max(this.df(this.v0), 1e-12)); }
    stampInitB(b) { const g = Math.max(this.df(this.v0), 1e-12); aI(b, this.a, this.b, this.f(this.v0) - g * this.v0); }
  }
  // Nonlinear resistance v = f(i)
  class NonV {
    constructor(sim, a, b, f, df, i0, lo, hi) { this.isP = true; this.nl = true; Object.assign(this, { sim, a, b, f, df, i0, lo, hi }); sim.var(this, 'k'); }
    conn() { return [[this.a, this.b]]; }
    ig(x) { return Math.min(this.hi, Math.max(this.lo, x[this.k])); }
    stampG(M) { aBr(M, this.a, this.b, this.k); aV(M, this.k, this.a, this.b, 1); }
    stampNL(M, x) { aA(M, this.k, this.k, -this.df(this.ig(x))); }
    stampNLB(b, x) { const i = this.ig(x); b[this.k] += this.f(i) - this.df(i) * i; }
    stampInitG(M) { aBr(M, this.a, this.b, this.k); aV(M, this.k, this.a, this.b, 1); aA(M, this.k, this.k, -this.df(this.i0)); }
    stampInitB(b) { b[this.k] += this.f(this.i0) - this.df(this.i0) * this.i0; }
  }
  M_.NONI = ctx => { const P = ctx.P; return new NonI(ctx.sim, ctx.p(0), ctx.p(1), P.fn('f', ['v']), P.fn('df', ['v']), P.num('x0', 0), P.num('lo', -1e30), P.num('hi', 1e30)); };
  M_.NONV = ctx => { const P = ctx.P; return new NonV(ctx.sim, ctx.p(0), ctx.p(1), P.fn('f', ['i']), P.fn('df', ['i']), P.num('x0', 0), P.num('lo', -1e30), P.num('hi', 1e30)); };

  /* ------------------------------------------------------------------
   * Switches
   * ------------------------------------------------------------------ */
  function gate(ctx, i) { return ctx.c(i); }
  const phys = (ctx, d) => String(ctx.P.raw('model', d)) !== 'ideal' && String(ctx.P.raw('model', d)) !== '3state';
  M_.DIODE = ctx => phys(ctx, 'ideal') ? S.devMake.diode(ctx, ctx.p(0), ctx.p(1)) : new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'D', vf: ctx.P.num('vf', 0), ron: ctx.P.num('ron', RON), roff: ctx.P.num('roff', ROFF), init: ctx.P.int('init', 0) ? 1 : 0 });
  M_.LED = ctx => phys(ctx, 'ideal') ? S.devMake.diode(ctx, ctx.p(0), ctx.p(1)) : new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'D', vf: ctx.P.num('vf', 2), ron: ctx.P.num('ron', RON), roff: ctx.P.num('roff', ROFF), init: 0 });
  M_.ZENER = ctx => phys(ctx, 'ideal') ? S.devMake.diode(ctx, ctx.p(0), ctx.p(1), { bv: ctx.P.num('vb') }) : new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'ZEN', vf: ctx.P.num('vf', 0), vb: ctx.P.num('vb'), ron: ctx.P.num('ron', RON), roff: ctx.P.num('roff', ROFF) });
  M_.DIAC = ctx => new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'DIAC', vb: ctx.P.num('vbo'), vbb: ctx.P.num('vbb', 0) });
  M_.THY = ctx => new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'THY', vf: ctx.P.num('vf', 0), ih: ctx.P.num('ih', 0), il: ctx.P.num('il', 0), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2) });
  M_.TRIAC = ctx => new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'TRIAC', vf: ctx.P.num('vf', 0), gn: gate(ctx, 2) });
  M_.GTO = ctx => new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'GTO', vf: ctx.P.num('vf', 0), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2) });
  M_.NPN = ctx => new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'GTO', vf: ctx.P.num('vf', 0), roff: ctx.P.num('roff', ROFF), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2) });
  M_.PNP = ctx => new Sw(ctx.sim, ctx.p(1), ctx.p(0), { type: 'GTO', vf: ctx.P.num('vf', 0), roff: ctx.P.num('roff', ROFF), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2), ginv: true });
  M_.IGBT = ctx => [
    new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'GTO', vf: ctx.P.num('vf', 0), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2) }),
    new Sw(ctx.sim, ctx.p(1), ctx.p(0), { type: 'D', vf: ctx.P.num('vd', 0), roff: ctx.P.num('roff', ROFF) })];
  M_.MOSFET = ctx => [
    new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'GTO', ron: ctx.P.num('ron', RON), roff: ctx.P.num('roff', ROFF), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2) }),
    new Sw(ctx.sim, ctx.p(1), ctx.p(0), { type: 'D', vf: ctx.P.num('vd', 0), roff: ctx.P.num('roff', ROFF) })];
  M_.PMOS = ctx => [
    new Sw(ctx.sim, ctx.p(1), ctx.p(0), { type: 'GTO', ron: ctx.P.num('ron', RON), roff: ctx.P.num('roff', ROFF), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2), ginv: true }),
    new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'D', vf: ctx.P.num('vd', 0), roff: ctx.P.num('roff', ROFF) })];
  M_.SSWI = ctx => new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'SSW', ron: ctx.P.num('ron', RON), init: ctx.P.int('init', 0) ? 1 : 0, gn: gate(ctx, 2) });
  M_.SSWI3 = ctx => [0, 1, 2].map(i => new Sw(ctx.sim, ctx.p(i), ctx.p(i + 3), { type: 'SSW', ron: ctx.P.num('ron', RON), gn: gate(ctx, 6) }));
  M_.SPDT = ctx => { const ron = ctx.P.num('ron', RON), init = ctx.P.int('init', 0) ? 1 : 0, gn = gate(ctx, 3); return [new Sw(ctx.sim, ctx.p(0), ctx.p(1), { type: 'SSW', ron, init, gn }), new Sw(ctx.sim, ctx.p(0), ctx.p(2), { type: 'SSW', ron, init: 1 - init, gn, ginv: true })]; };
  M_.DPST = ctx => { const ron = ctx.P.num('ron', RON), init = ctx.P.int('init', 0) ? 1 : 0, gn = gate(ctx, 4); return [0, 1].map(i => new Sw(ctx.sim, ctx.p(2 * i), ctx.p(2 * i + 1), { type: 'SSW', ron, init, gn })); };
  M_.DPDT = ctx => {
    const ron = ctx.P.num('ron', RON), init = ctx.P.int('init', 0) ? 1 : 0, gn = gate(ctx, 6); const out = [];
    for (const o of [0, 3]) { out.push(new Sw(ctx.sim, ctx.p(o), ctx.p(o + 1), { type: 'SSW', ron, init, gn }), new Sw(ctx.sim, ctx.p(o), ctx.p(o + 2), { type: 'SSW', ron, init: 1 - init, gn, ginv: true })); }
    return out;
  };
  M_.PBSW = ctx => new TimedSw(ctx.sim, ctx.p(0), ctx.p(1), ctx.P.int('init', 0) ? 1 : 0, ctx.P.list('times', []));

  // Bridges
  function bridge(ctx, list, vfKey, kind, ctPin, shifts) {
    const sim = ctx.sim; const vf = ctx.P.num(vfKey, 0);
    if (kind === 'D') return list.map(([a, k]) => new Sw(sim, ctx.p(a), ctx.p(k), { type: 'D', vf }));
    const ct = ctx.c(ctPin);
    const gs = new GateShift(sim, ctx, ct, shifts);
    return [gs].concat(list.map(([a, k], i) => new Sw(sim, ctx.p(a), ctx.p(k), { type: 'THY', vf, gn: gs.outs[i] })));
  }
  // BDIODE1 pins: A+ A- DC+ DC-
  M_.BDIODE1 = ctx => bridge(ctx, [[0, 2], [3, 1], [1, 2], [3, 0]], 'vf', 'D');
  // BDIODE3 pins: A B C DC+ DC-
  M_.BDIODE3 = ctx => bridge(ctx, [[0, 3], [1, 3], [2, 3], [4, 0], [4, 1], [4, 2]], 'vf', 'D');
  // BTHY1 pins: A+ A- DC+ DC- Ct ; switches 1:A+->DC+ 2:DC- ->A- 3:A- ->DC+ 4:DC- ->A+
  M_.BTHY1 = ctx => bridge(ctx, [[0, 2], [3, 1], [1, 2], [3, 0]], 'vf', 'T', 4, [0, 0, 180, 180]);
  // BTHY3 pins: A B C DC+ DC- Ct ; switch order 1..6
  M_.BTHY3 = ctx => bridge(ctx, [[0, 3], [4, 2], [1, 3], [4, 0], [2, 3], [4, 1]], 'vf', 'T', 5, [0, 60, 120, 180, 240, 300]);
  // BTHY3H pins: A B C N Ct
  M_.BTHY3H = ctx => bridge(ctx, [[0, 3], [1, 3], [2, 3]], 'vf', 'T', 4, [0, 120, 240]);
  // BTHY6H pins: A1..A6 N Ct
  M_.BTHY6H = ctx => bridge(ctx, [[0, 6], [1, 6], [2, 6], [3, 6], [4, 6], [5, 6]], 'vf', 'T', 7, [0, 60, 120, 180, 240, 300]);

  /* ------------------------------------------------------------------
   * Three-state (linear-region) devices
   * ------------------------------------------------------------------ */
  // MOSFET channel: current from X to Y = gm*(pol*(vg - vS) - vth) in active region
  class MosCh {
    constructor(sim, X, Y, g, S0, pol, vth, gm, ron) { this.isP = true; this.sw = true; Object.assign(this, { sim, X, Y, gN: g, S0, pol, vth, gm, ron }); sim.reqP(this, 's', 1); }
    conn() { return [[this.X, this.Y]]; }
    init() { this.s[0] = 0; }
    state() { return this.s[0]; } setState(v) { this.s[0] = v; } skey() { return this.s[0]; }
    stampG(M) {
      const st = this.s[0];
      if (st === 2) { aG(M, this.X, this.Y, 1 / this.ron); return; }
      aG(M, this.X, this.Y, 1 / ROFF);
      if (st === 1) { const k = this.gm * this.pol; aA(M, this.X, this.gN, k); aA(M, this.X, this.S0, -k); aA(M, this.Y, this.gN, -k); aA(M, this.Y, this.S0, k); }
    }
    stampB(b) { if (this.s[0] === 1) { const I = this.gm * this.vth; aB(b, this.X, I); aB(b, this.Y, -I); } }
    desired(x) {
      const vov = this.pol * (vx(x, this.gN) - vx(x, this.S0)) - this.vth;
      if (vov <= 0) return 0;
      const Isat = this.gm * vov; const vxy = vx(x, this.X) - vx(x, this.Y);
      return vxy >= Isat * this.ron ? 1 : 2;
    }
  }
  M_.MOS3 = ctx => { if (phys(ctx, 'GENERIC_N')) return S.devMake.mos(ctx, 1); const P = ctx.P, d = ctx.p(0), s = ctx.p(1), g = ctx.p(2); return [new MosCh(ctx.sim, d, s, g, s, 1, P.num('vth'), P.pos('gm'), P.pos('ron')), new Sw(ctx.sim, s, d, { type: 'D', vf: P.num('vd', 0) })]; };
  M_.PMOS3 = ctx => { if (phys(ctx, 'GENERIC_P')) return S.devMake.mos(ctx, -1); const P = ctx.P, d = ctx.p(0), s = ctx.p(1), g = ctx.p(2); return [new MosCh(ctx.sim, s, d, g, s, -1, P.num('vth'), P.pos('gm'), P.pos('ron')), new Sw(ctx.sim, d, s, { type: 'D', vf: P.num('vd', 0) })]; };

  // Linear BJT: states BE (0/1) x CE (0 off,1 active,2 sat) encoded st = be*3+ce
  class BJT {
    constructor(sim, Pn, Qn, X, Y, beta, vr, vsat, roff) {
      this.isP = true; this.sw = true; Object.assign(this, { sim, Pn, Qn, X, Y, beta, vr, vsat }); this.roff = roff || ROFF;
      sim.var(this, 'kb'); sim.reqP(this, 's', 1);
    }
    conn() { return [[this.Pn, this.Qn, this.X, this.Y]]; }
    init() { this.s[0] = 0; }
    state() { return this.s[0]; } setState(v) { this.s[0] = v; } skey() { return String.fromCharCode(65 + this.s[0]); }
    stampG(M) {
      const st = this.s[0], be = (st / 3) | 0, ce = st % 3, kb = this.kb;
      if (be) { aBr(M, this.Pn, this.Qn, kb); aV(M, kb, this.Pn, this.Qn, 1); aA(M, kb, kb, -RON); }
      else aA(M, kb, kb, 1);
      aG(M, this.X, this.Y, ce === 2 ? 1 / RON : 1 / this.roff);
      if (ce === 1) { aA(M, this.X, kb, this.beta); aA(M, this.Y, kb, -this.beta); }
    }
    stampB(b) {
      const st = this.s[0], be = (st / 3) | 0, ce = st % 3;
      if (be) b[this.kb] += this.vr;
      if (ce === 2) aI(b, this.X, this.Y, -this.vsat / RON);
    }
    desired(x) {
      const st = this.s[0]; let be = (st / 3) | 0, ce = st % 3;
      const vbe = vx(x, this.Pn) - vx(x, this.Qn), ib = be ? x[this.kb] : 0;
      if (!be) { if (vbe > this.vr + EPSV) be = 1; }
      else if (ib < -EPSI) be = 0;
      if (!be) return 0;
      const vce = vx(x, this.X) - vx(x, this.Y);
      const ic = ce === 2 ? (vce - this.vsat) / RON : (ce === 1 ? this.beta * ib : vce / this.roff);
      if (ce === 0) ce = 1;
      else if (ce === 1 && vce < this.vsat) ce = 2;
      else if (ce === 2 && ic > this.beta * Math.max(ib, 0)) ce = 1;
      return be * 3 + ce;
    }
  }
  M_.NPN3 = ctx => { const P = ctx.P; if (phys(ctx, '2N3904')) return S.devMake.bjt(ctx, 1); return new BJT(ctx.sim, ctx.p(2), ctx.p(1), ctx.p(0), ctx.p(1), P.pos('beta'), P.num('vr', 0.7), P.num('vsat', 0.2), P.num('roff', ROFF)); };
  M_.PNP3 = ctx => { const P = ctx.P; if (phys(ctx, '2N3906')) return S.devMake.bjt(ctx, -1); return new BJT(ctx.sim, ctx.p(1), ctx.p(2), ctx.p(1), ctx.p(0), P.pos('beta'), P.num('vr', 0.7), P.num('vsat', 0.2), P.num('roff', ROFF)); };

  /* ------------------------------------------------------------------
   * Coupled inductors and transformers
   * ------------------------------------------------------------------ */
  M_.MUT2 = ctx => {
    const P = ctx.P; const L11 = P.num('l11'), L12 = P.num('l12'), L22 = P.num('l22');
    if (L11 * L22 - L12 * L12 < -1e-12 * L11 * L22) throw ctx.err('互感過大：需 L12² ≤ L11·L22');
    return new MutL(ctx.sim, [[ctx.p(0), ctx.p(1)], [ctx.p(2), ctx.p(3)]], [[L11, L12], [L12, L22]], [P.num('i1', 0), P.num('i2', 0)]);
  };
  M_.MUT3 = ctx => {
    const P = ctx.P; const L = [[P.num('l11'), P.num('l12'), P.num('l13')], [0, P.num('l22'), P.num('l23')], [0, 0, P.num('l33')]];
    L[1][0] = L[0][1]; L[2][0] = L[0][2]; L[2][1] = L[1][2];
    return new MutL(ctx.sim, [[ctx.p(0), ctx.p(1)], [ctx.p(2), ctx.p(3)], [ctx.p(4), ctx.p(5)]], L, [P.num('i1', 0), P.num('i2', 0), P.num('i3', 0)]);
  };
  M_.TFI = ctx => {
    const Np = ctx.P.pos('np'), Ns = ctx.P.pos('ns');
    return new Xfmr(ctx.sim, [[ctx.p(0), ctx.p(1), Np], [ctx.p(2), ctx.p(3), Ns]]);
  };
  // Real transformer: windings [{a,b,R,L,N}], first is primary; Lm across primary (referred to primary)
  function realXfmr(sim, wins, Lm) {
    const out = []; const N1 = wins[0].N; const xw = [];
    wins.forEach((w, j) => {
      const r2 = (w.N / N1) * (w.N / N1);
      const R = j === 0 ? w.R : w.R * r2, L = j === 0 ? w.L : w.L * r2;
      let inner;
      if (R || L) { inner = sim.newNode(); out.push(new SerBr(sim, w.a, inner, R, L, 0, 0, 0)); }
      else inner = w.a;
      xw.push([inner, w.b, w.N]);
    });
    if (Lm > 0) out.push(new SerBr(sim, xw[0][0], xw[0][1], 0, Lm, 0, 0, 0));
    out.push(new Xfmr(sim, xw));
    return out;
  }
  M_.TF1 = ctx => {
    const P = ctx.P;
    return realXfmr(ctx.sim, [
      { a: ctx.p(0), b: ctx.p(1), R: P.num('rp', 0), L: P.num('lp', 0), N: P.pos('np') },
      { a: ctx.p(2), b: ctx.p(3), R: P.num('rs', 0), L: P.num('ls', 0), N: P.pos('ns') }], P.num('lm', 0));
  };
  M_.TF3W = ctx => {
    const P = ctx.P;
    return realXfmr(ctx.sim, [
      { a: ctx.p(0), b: ctx.p(1), R: P.num('rp', 0), L: P.num('lp', 0), N: P.pos('np') },
      { a: ctx.p(2), b: ctx.p(3), R: P.num('rs', 0), L: P.num('ls', 0), N: P.pos('ns') },
      { a: ctx.p(4), b: ctx.p(5), R: P.num('rt', 0), L: P.num('lt', 0), N: P.pos('nt') }], P.num('lm', 0));
  };
  // 3-phase Y/Y: pins A B C N a b c n
  M_.TF3YY = ctx => {
    const P = ctx.P; let out = [];
    for (let i = 0; i < 3; i++) out = out.concat(realXfmr(ctx.sim, [
      { a: ctx.p(i), b: ctx.p(3), R: P.num('rp', 0), L: P.num('lp', 0), N: P.pos('np') },
      { a: ctx.p(4 + i), b: ctx.p(7), R: P.num('rs', 0), L: P.num('ls', 0), N: P.pos('ns') }], P.num('lm', 0)));
    return out;
  };
  // 3-phase Y/D: pins A B C N a b c ; delta windings a-b, b-c, c-a
  M_.TF3YD = ctx => {
    const P = ctx.P; let out = [];
    for (let i = 0; i < 3; i++) out = out.concat(realXfmr(ctx.sim, [
      { a: ctx.p(i), b: ctx.p(3), R: P.num('rp', 0), L: P.num('lp', 0), N: P.pos('np') },
      { a: ctx.p(4 + i), b: ctx.p(4 + (i + 1) % 3), R: P.num('rs', 0), L: P.num('ls', 0), N: P.pos('ns') }], P.num('lm', 0)));
    return out;
  };

  /* ------------------------------------------------------------------
   * Op-amp (PSIM model: A=1e5, Ro=80 ohm, output clamped to Vs-..Vs+)
   * ------------------------------------------------------------------ */
  class OpAmp {
    constructor(sim, vn, vp, out, g, A, Ro, vsp, vsn) {
      this.isP = true; this.sw = true; Object.assign(this, { sim, vn, vp, out, gnd: g, A, Ro, vsp, vsn });
      this.vi = sim.newNode(); sim.var(this, 'k'); sim.reqP(this, 's', 1);
    }
    conn() { return [[this.vi, this.gnd, this.out]]; }
    init() { this.s[0] = 0; }
    state() { return this.s[0]; } setState(v) { this.s[0] = v; } skey() { return this.s[0]; }
    stampG(M) {
      const k = this.k;
      aBr(M, this.vi, this.gnd, k); aV(M, k, this.vi, this.gnd, 1);
      if (this.s[0] === 0) { aA(M, k, this.vp, -this.A); aA(M, k, this.vn, this.A); }
      aG(M, this.vi, this.out, 1 / this.Ro);
    }
    stampB(b) { const st = this.s[0]; if (st === 1) b[this.k] += this.vsp; else if (st === 2) b[this.k] += this.vsn; }
    desired(x) {
      const st = this.s[0]; const u = this.A * (vx(x, this.vp) - vx(x, this.vn));
      const vo = vx(x, this.vi) - vx(x, this.gnd);
      if (st === 0) return vo > this.vsp + 1e-9 ? 1 : (vo < this.vsn - 1e-9 ? 2 : 0);
      if (st === 1) return u < this.vsp ? 0 : 1;
      return u > this.vsn ? 0 : 2;
    }
    stampAC(C) {
      const k = this.k;
      C.a(this.vi, k, 1); C.a(this.gnd, k, -1); C.a(k, this.vi, 1); C.a(k, this.gnd, -1);
      C.a(k, this.vp, -this.A); C.a(k, this.vn, this.A); C.g(this.vi, this.out, 1 / this.Ro, 0);
    }
  }
  M_.OPAMP = ctx => { const P = ctx.P; return new OpAmp(ctx.sim, ctx.p(0), ctx.p(1), ctx.p(2), -1, P.num('A', 1e5), P.num('ro', 80), P.num('vsp', 15), P.num('vsn', -15)); };
  M_.OPAMP1 = ctx => { const P = ctx.P; return new OpAmp(ctx.sim, ctx.p(0), ctx.p(1), ctx.p(2), ctx.p(3), P.num('A', 1e5), P.num('ro', 80), P.num('vsp', 15), P.num('vsn', -15)); };

  // dv/dt block in the power circuit: pins [in, out]
  class DVDT {
    constructor(sim, i, o) { this.isP = true; this.sim = sim; this.i = i; this.o = o; sim.var(this, 'k'); sim.reqP(this, 's', 1); this.dyn = true; }
    conn() { return [[this.o, -1]]; }
    stampG(M, h) { aBr(M, this.o, -1, this.k); aA(M, this.k, this.o, 1); aA(M, this.k, this.i, -1 / h); }
    stampB(b, t, h) { b[this.k] += -this.s[0] / h; }
    stampInitG(M) { aBr(M, this.o, -1, this.k); aA(M, this.k, this.o, 1); }
    acceptInit(x) { this.s[0] = vx(x, this.i); }
    accept(x) { this.s[0] = vx(x, this.i); }
    stampAC(C) { C.a(this.o, this.k, 1); C.a(this.k, this.o, 1); C.a(this.k, this.i, 0, -C.w); }
  }
  M_.DVDT = ctx => new DVDT(ctx.sim, ctx.p(0), ctx.p(1));

  /* ------------------------------------------------------------------
   * Controlled sources
   * ------------------------------------------------------------------ */
  // input descriptor: {p} power node or {c} control node
  function inDesc(ctx, i) { const k = ctx.kind(i); if (k === 'N') throw ctx.err('控制輸入端未連接'); if (k === 'C') return { c: ctx.c(i) }; if (k === 'G') return { g: 1 }; return { p: ctx.p(i) }; }
  function inVal(d, sim, x) { if (d.c !== undefined) return sim.cv[d.c]; if (d.p !== undefined) return vx(x || sim.xP, d.p); return 0; }

  // Linear VCVS / VCCS with differential input (power or control)
  class VCtl {
    constructor(sim, ctx, op, on, ip, inn, g, isI) {
      this.isP = true; Object.assign(this, { sim, op, on, ip, inn, gain: g, isI });
      const ctl = ip.c !== undefined || inn.c !== undefined, pw = ip.p !== undefined || inn.p !== undefined;
      if (ctl && pw) throw ctx.err('控制輸入的兩端必須同在功率電路或同在控制電路');
      this.ctl = ctl;
      if (!isI) sim.var(this, 'k');
      this.cins = []; if (ip.c > 0) this.cins.push(ip.c); if (inn.c > 0) this.cins.push(inn.c);
    }
    conn() { return this.isI ? [] : [[this.op, this.on]]; }
    stampG(M) {
      const g = this.gain;
      if (!this.isI) { aBr(M, this.op, this.on, this.k); aV(M, this.k, this.op, this.on, 1); if (!this.ctl) { aA(M, this.k, this.ip.p ?? -1, -g); aA(M, this.k, this.inn.p ?? -1, g); } }
      else if (!this.ctl) {
        const a = this.ip.p ?? -1, b = this.inn.p ?? -1;
        aA(M, this.op, a, -g); aA(M, this.op, b, g); aA(M, this.on, a, g); aA(M, this.on, b, -g);
      }
    }
    stampB(b) {
      if (!this.ctl) return;
      const v = this.gain * (inVal(this.ip, this.sim) - inVal(this.inn, this.sim));
      if (this.isI) { aB(b, this.op, v); aB(b, this.on, -v); } else b[this.k] += v;
    }
    stampAC(C) {
      const g = this.gain;
      const col = d => d.c !== undefined ? C.ci(d.c) : (d.p !== undefined ? d.p : -1);
      const a = col(this.ip), b = col(this.inn);
      if (!this.isI) { C.a(this.op, this.k, 1); C.a(this.on, this.k, -1); C.a(this.k, this.op, 1); C.a(this.k, this.on, -1); C.a(this.k, a, -g); C.a(this.k, b, g); }
      else { C.a(this.op, a, -g); C.a(this.op, b, g); C.a(this.on, a, g); C.a(this.on, b, -g); }
    }
  }
  M_.VVCVS = ctx => new VCtl(ctx.sim, ctx, ctx.p(0), ctx.p(1), inDesc(ctx, 2), inDesc(ctx, 3), ctx.P.num('k', 1), false);
  M_.IVCCS = ctx => new VCtl(ctx.sim, ctx, ctx.p(0), ctx.p(1), inDesc(ctx, 2), inDesc(ctx, 3), ctx.P.num('k', 1), true);

  // Current-controlled (sense current flows through pins 2 -> 3)
  class CCtl {
    constructor(sim, op, on, cp, cn, g, isI) { this.isP = true; Object.assign(this, { sim, op, on, cp, cn, gain: g, isI }); sim.var(this, 'ks'); if (!isI) sim.var(this, 'ko'); }
    conn() { return this.isI ? [[this.cp, this.cn]] : [[this.op, this.on], [this.cp, this.cn]]; }
    stampG(M) {
      const ks = this.ks; aBr(M, this.cp, this.cn, ks); aV(M, ks, this.cp, this.cn, 1);
      if (!this.isI) { const ko = this.ko; aBr(M, this.op, this.on, ko); aV(M, ko, this.op, this.on, 1); aA(M, ko, ks, -this.gain); }
      else { aA(M, this.op, ks, -this.gain); aA(M, this.on, ks, this.gain); }
    }
    stampAC(C) { this.stampG({ A: null, n: 0 }, C); }
  }
  CCtl.prototype.stampAC = function (C) {
    const ks = this.ks; C.a(this.cp, ks, 1); C.a(this.cn, ks, -1); C.a(ks, this.cp, 1); C.a(ks, this.cn, -1);
    if (!this.isI) { const ko = this.ko; C.a(this.op, ko, 1); C.a(this.on, ko, -1); C.a(ko, this.op, 1); C.a(ko, this.on, -1); C.a(ko, ks, -this.gain); }
    else { C.a(this.op, ks, -this.gain); C.a(this.on, ks, this.gain); }
  };
  M_.VCCVS1 = ctx => new CCtl(ctx.sim, ctx.p(0), ctx.p(1), ctx.p(2), ctx.p(3), ctx.P.num('k', 1), false);
  M_.ICCCS1 = ctx => new CCtl(ctx.sim, ctx.p(0), ctx.p(1), ctx.p(2), ctx.p(3), ctx.P.num('k', 1), true);

  // Current-controlled by the current of an RLC branch connected across pins 2,3
  class CCtlX {
    constructor(sim, ctx, op, on, n2, n3, g, isI) { this.isP = true; Object.assign(this, { sim, ctx, op, on, n2, n3, gain: g, isI }); if (!isI) sim.var(this, 'ko'); }
    link(sim) {
      const el = this.ctx.el; const nets = [el.pins[2], el.pins[3]];
      for (const m of sim.insts) {
        if (m === this || !m.el || m.el === el) continue;
        if (!(m instanceof Res || m instanceof Cap || m instanceof SerBr)) continue;
        if (!['R', 'L', 'C', 'RL', 'RC', 'LC', 'RLC'].includes(m.el.type)) continue;
        const pn = m.el.pins;
        if (pn[0] === nets[0] && pn[1] === nets[1]) { this.ref = m; this.sg = 1; break; }
        if (pn[0] === nets[1] && pn[1] === nets[0]) { this.ref = m; this.sg = -1; break; }
      }
      if (!this.ref) throw this.ctx.err('控制端必須跨接在一個 R/L/C 支路兩端（以量測該支路電流）');
      this.ref.wantI = true;
    }
    conn() { return this.isI ? [] : [[this.op, this.on]]; }
    stampG(M) {
      const ks = this.ref.k, g = this.gain * this.sg;
      if (!this.isI) { const ko = this.ko; aBr(M, this.op, this.on, ko); aV(M, ko, this.op, this.on, 1); aA(M, ko, ks, -g); }
      else { aA(M, this.op, ks, -g); aA(M, this.on, ks, g); }
    }
    stampAC(C) {
      const ks = this.ref.k, g = this.gain * this.sg;
      if (!this.isI) { const ko = this.ko; C.a(this.op, ko, 1); C.a(this.on, ko, -1); C.a(ko, this.op, 1); C.a(ko, this.on, -1); C.a(ko, ks, -g); }
      else { C.a(this.op, ks, -g); C.a(this.on, ks, g); }
    }
  }
  M_.VCCVS = ctx => new CCtlX(ctx.sim, ctx, ctx.p(0), ctx.p(1), ctx.p(2), ctx.p(3), ctx.P.num('k', 1), false);
  M_.ICCCS = ctx => new CCtlX(ctx.sim, ctx, ctx.p(0), ctx.p(1), ctx.p(2), ctx.p(3), ctx.P.num('k', 1), true);

  // Nonlinear / variable-gain voltage-controlled sources with single-ended inputs
  class VNon {
    constructor(sim, ctx, op, on, ins, f, df, isI, varGain) {
      this.isP = true; Object.assign(this, { sim, op, on, ins, f, df, isI, varGain });
      this.pw = ins.some(d => d.p !== undefined);
      if (!isI) sim.var(this, 'k');
      this.cins = ins.filter(d => d.c > 0).map(d => d.c);
      if (varGain) this.dyn = this.pw; else if (this.pw) this.nl = true;
    }
    conn() { return this.isI ? [] : [[this.op, this.on]]; }
    vals(x) { return this.ins.map(d => inVal(d, this.sim, x)); }
    stampG(M) { if (!this.isI) { aBr(M, this.op, this.on, this.k); aV(M, this.k, this.op, this.on, 1); } if (this.varGain && this.pw) this.stampVG(M); }
    // variable gain: out = (k*vin2_prev) * vin1
    stampVG(M) {
      const g = this.f(1, inVal(this.ins[1], this.sim, this.sim.xP));
      const d = this.ins[0];
      if (d.p === undefined) return;
      if (!this.isI) aA(M, this.k, d.p, -g); else { aA(M, this.op, d.p, -g); aA(M, this.on, d.p, g); }
    }
    stampB(b) {
      if (this.nl) return;
      let v;
      if (this.varGain) { if (this.pw && this.ins[0].p !== undefined) return; v = this.f(inVal(this.ins[0], this.sim), inVal(this.ins[1], this.sim, this.sim.xP)); }
      else v = this.f.apply(null, this.vals(this.sim.xP));
      if (this.isI) { aB(b, this.op, v); aB(b, this.on, -v); } else b[this.k] += v;
    }
    stampNL(M, x) {
      const v = this.vals(x); const d = this.df(v);
      this.ins.forEach((dd, j) => { if (dd.p === undefined) return; if (!this.isI) aA(M, this.k, dd.p, -d[j]); else { aA(M, this.op, dd.p, -d[j]); aA(M, this.on, dd.p, d[j]); } });
    }
    stampNLB(b, x) {
      const v = this.vals(x); const d = this.df(v); let r = this.f.apply(null, v);
      this.ins.forEach((dd, j) => { if (dd.p !== undefined) r -= d[j] * v[j]; });
      if (this.isI) { aB(b, this.op, r); aB(b, this.on, -r); } else b[this.k] += r;
    }
  }
  const SAFE = (v, e) => (Math.abs(v) < e ? (v < 0 ? -e : e) : v);
  function nonMk(kind, isI) {
    return ctx => {
      const P = ctx.P, k = P.num('k', 1);
      let ins, f, df, vg = false;
      switch (kind) {
        case 'M': ins = [inDesc(ctx, 2), inDesc(ctx, 3)]; f = (a, b) => k * a * b; df = v => [k * v[1], k * v[0]]; break;
        case 'D': ins = [inDesc(ctx, 2), inDesc(ctx, 3)]; f = (a, b) => k * a / SAFE(b, 1e-30); df = v => [k / SAFE(v[1], 1e-30), -k * v[0] / (SAFE(v[1], 1e-30) * SAFE(v[1], 1e-30))]; break;
        case 'Q': ins = [inDesc(ctx, 2)]; f = a => k * Math.sqrt(Math.max(a, 0)); df = v => [k * 0.5 / Math.sqrt(Math.max(v[0], 1e-24))]; break;
        case 'P': { const k1 = P.num('k1', 1), k2 = P.num('k2', 1); ins = [inDesc(ctx, 2)]; f = a => Math.sign(a) * k * Math.pow(k1 * Math.abs(a), k2); df = v => [k * k2 * k1 * Math.pow(Math.max(k1 * Math.abs(v[0]), 1e-12), k2 - 1)]; break; }
        case 'V': ins = [inDesc(ctx, 2), inDesc(ctx, 3)]; f = (a, b) => k * b * a; df = null; vg = true; break;
      }
      return new VNon(ctx.sim, ctx, ctx.p(0), ctx.p(1), ins, f, df, isI, vg);
    };
  }
  M_.VNONM = nonMk('M', false); M_.VNOND = nonMk('D', false); M_.VNONSQ = nonMk('Q', false); M_.VPOWERS = nonMk('P', false); M_.VVCVSV = nonMk('V', false);
  M_.INONM = nonMk('M', true); M_.INOND = nonMk('D', true); M_.INONSQ = nonMk('Q', true); M_.IVCCSV = nonMk('V', true);

  // Control-to-power interface: pins [in(control), out(power)]
  class CTOP {
    constructor(sim, c, o) { this.isP = true; this.sim = sim; this.cn = c; this.o = o; this.cins = [c]; sim.var(this, 'k'); }
    conn() { return [[this.o, -1]]; }
    stampG(M) { aBr(M, this.o, -1, this.k); aA(M, this.k, this.o, 1); }
    stampB(b) { b[this.k] += this.sim.cv[this.cn]; }
    stampAC(C) { C.a(this.o, this.k, 1); C.a(this.k, this.o, 1); C.a(this.k, C.ci(this.cn), -1); }
  }
  M_.CTOP = ctx => new CTOP(ctx.sim, ctx.c(0), ctx.p(1));

  /* ------------------------------------------------------------------
   * Sensors and probes
   * ------------------------------------------------------------------ */
  // zero-volt ammeter branch (current from a to b)
  class Amm {
    constructor(sim, a, b) { this.isP = true; this.a = a; this.b = b; sim.var(this, 'k'); }
    conn() { return [[this.a, this.b]]; }
    stampG(M) { aBr(M, this.a, this.b, this.k); aV(M, this.k, this.a, this.b, 1); }
    stampAC(C) { C.a(this.a, this.k, 1); C.a(this.b, this.k, -1); C.a(this.k, this.a, 1); C.a(this.k, this.b, -1); }
  }
  S.Amm = Amm;

  // voltage sensor: pins [+, -, out]
  class VSen {
    constructor(sim, p, n, o, k) { this.isC = true; Object.assign(this, { sim, p, n, o, k }); this.ins = []; this.outs = [o]; }
    eval(t, h, sim) { sim.cv[this.o] = this.k * (vx(sim.x, this.p) - vx(sim.x, this.n)); }
    stampAC(C) { const r = C.ci(this.o); C.a(r, r, 1); C.a(r, this.p, -this.k); C.a(r, this.n, this.k); }
  }
  M_.VSEN = ctx => new VSen(ctx.sim, ctx.p(0), ctx.p(1), ctx.c(2), ctx.P.num('k', 1));
  // current sensor: pins [in, out, o]
  class ISen extends Amm {
    constructor(sim, a, b, o, k) { super(sim, a, b); this.isC = true; this.o = o; this.kg = k; this.ins = []; this.outs = [o]; }
    eval(t, h, sim) { sim.cv[this.o] = this.kg * sim.x[this.k]; }
    stampAC(C) { super.stampAC(C); }
  }
  ISen.prototype.stampAC = function (C) {
    Amm.prototype.stampAC.call(this, C);
    const r = C.ci(this.o); C.a(r, r, 1); C.a(r, this.k, -this.kg);
  };
  M_.ISEN = ctx => new ISen(ctx.sim, ctx.p(0), ctx.p(1), ctx.c(2), ctx.P.num('k', 1));

  // Probes ------------------------------------------------------------
  class VProbe {
    constructor(sim, name, d1, d2) {
      this.probe = true; this.pname = name; this.d1 = d1; this.d2 = d2 || { g: 1 };
      this.cins = []; [this.d1, this.d2].forEach(d => { if (d.c > 0) this.cins.push(d.c); });
    }
    value(sim) { return inVal(this.d1, sim, sim.x) - inVal(this.d2, sim, sim.x); }
  }
  M_.VP = ctx => new VProbe(ctx.sim, ctx.el.name, inDesc(ctx, 0));
  M_.VP2 = ctx => { const a = inDesc(ctx, 0), b = inDesc(ctx, 1); if ((a.c !== undefined && b.p !== undefined) || (a.p !== undefined && b.c !== undefined)) throw ctx.err('兩個量測端必須同在功率電路或同在控制電路'); return new VProbe(ctx.sim, ctx.el.name, a, b); };
  class IProbe extends Amm {
    constructor(sim, a, b, name) { super(sim, a, b); this.probe = true; this.pname = name; }
    value(sim) { return sim.x[this.k]; }
  }
  M_.IP = ctx => new IProbe(ctx.sim, ctx.p(0), ctx.p(1), ctx.el.name);

  // Meters: measured quantity -> filter -> output, implemented as control blocks
  class Meter {
    constructor(sim, name, getV, kind, f, fc) {
      this.isC = true; this.probe = true; this.pname = name; this.getV = getV; this.kind = kind; this.f = f; this.fc = fc;
      this.o = sim.newCNode(); this.outs = [this.o]; this.ins = []; this.ft = true;
      sim.reqC(this, 's', 4); // [lpf y, hp y, prev in, prev hp in]
      this.buf = null;
    }
    init(sim) { this.s.fill(0); this.tau = 1 / (2 * Math.PI * this.fc); if (this.kind === 'AC') { this.T = 1 / this.f; this.buf = { t: [0], I: [0] }; } }
    filt(u, h) {
      // returns [lpf, hp] using trapezoidal discretisation of 1st-order sections
      const s = this.s, a = h / (2 * this.tau);
      if (h === 0) return [s[0], s[1]];
      const y = (s[0] * (1 - a) + a * (u + s[2])) / (1 + a);
      const hp = ((1 - a) * s[1] + (u - s[2])) / (1 + a);
      return [y, hp];
    }
    eval(t, h, sim) {
      const u = this.getV(sim);
      if (h === 0 && t === 0) { this.p = { u, y: this.kind === 'DC' ? u : 0, hp: 0 }; sim.cv[this.o] = this.p.y; return; }
      const [y, hp] = this.filt(u, h);
      this.p = { u, y, hp };
      if (this.kind === 'DC') sim.cv[this.o] = y;
      else {
        // moving RMS of the high-passed signal over one period
        const b = this.buf; const n = b.t.length;
        const It = b.I[n - 1] + 0.5 * h * (hp * hp + this.s[1] * this.s[1]);
        const t0 = t - this.T;
        let I0 = 0;
        if (t0 <= 0) I0 = 0;
        else {
          let j = n - 1; while (j > 0 && b.t[j] > t0) j--;
          const ta = b.t[j], tb = j + 1 < n ? b.t[j + 1] : t, Ia = b.I[j], Ib = j + 1 < n ? b.I[j + 1] : It;
          I0 = Ia + (Ib - Ia) * (tb > ta ? (t0 - ta) / (tb - ta) : 0);
        }
        this.p.It = It;
        sim.cv[this.o] = Math.sqrt(Math.max(0, (It - I0) / Math.min(this.T, Math.max(t, 1e-300))));
      }
    }
    commit(t, h) {
      if (!this.p) return;
      this.s[0] = this.p.y; this.s[1] = this.p.hp; this.s[2] = this.p.u;
      if (this.kind === 'AC' && h > 0) {
        const b = this.buf; b.t.push(t); b.I.push(this.p.It);
        const keep = t - this.T * 1.5; let cut = 0; while (cut < b.t.length - 2 && b.t[cut + 1] < keep) cut++;
        if (cut > 1024) { b.t.splice(0, cut); b.I.splice(0, cut); }
      }
    }
    value(sim) { return sim.cv[this.o]; }
    stampAC(C) { const r = C.ci(this.o); C.a(r, r, 1); }
    saveX() { return this.buf ? { t: this.buf.t.slice(), I: this.buf.I.slice() } : null; }
    restoreX(x) { if (x) this.buf = { t: x.t.slice(), I: x.I.slice() }; }
  }
  M_.VDCM = ctx => new Meter(ctx.sim, ctx.el.name, (p => s => vx(s.x, p[0]) - vx(s.x, p[1]))([ctx.p(0), ctx.p(1)]), 'DC', 0, ctx.P.pos('fc'));
  M_.VACM = ctx => new Meter(ctx.sim, ctx.el.name, (p => s => vx(s.x, p[0]) - vx(s.x, p[1]))([ctx.p(0), ctx.p(1)]), 'AC', ctx.P.pos('f'), ctx.P.pos('fc'));
  M_.ADCM = ctx => { const a = new Amm(ctx.sim, ctx.p(0), ctx.p(1)); return [a, new Meter(ctx.sim, ctx.el.name, s => s.x[a.k], 'DC', 0, ctx.P.pos('fc'))]; };
  M_.AACM = ctx => { const a = new Amm(ctx.sim, ctx.p(0), ctx.p(1)); return [a, new Meter(ctx.sim, ctx.el.name, s => s.x[a.k], 'AC', ctx.P.pos('f'), ctx.P.pos('fc'))]; };
  // Wattmeter pins: [I+, I-, V-]; v = v(I+) - v(V-)
  M_.WATT = ctx => { const a = new Amm(ctx.sim, ctx.p(0), ctx.p(1)); const p0 = ctx.p(0), pn = ctx.p(2); return [a, new Meter(ctx.sim, ctx.el.name, s => s.x[a.k] * (vx(s.x, p0) - vx(s.x, pn)), 'DC', 0, ctx.P.pos('fc'))]; };

  // AC sweep probes
  class ACProbe {
    constructor(sim, name, d1, d2, neg) {
      this.acProbe = true; this.pname = name; this.d1 = d1; this.d2 = d2; this.loop = !!d2; this.neg = !!neg;
      this.cins = []; [d1, d2].forEach(d => { if (d && d.c > 0) this.cins.push(d.c); });
    }
    acVals(sim) { return [inVal(this.d1, sim, sim.x), this.d2 ? inVal(this.d2, sim, sim.x) : 0]; }
    acPhasor(xr, xi, ci) {
      const ph = d => d.c !== undefined ? (d.c > 0 ? [xr[ci(d.c)], xi[ci(d.c)]] : [0, 0]) : (d.p !== undefined ? (d.p >= 0 ? [xr[d.p], xi[d.p]] : [0, 0]) : [0, 0]);
      const a = ph(this.d1);
      if (!this.loop) return a;
      const b = ph(this.d2); return [a[0], a[1], b[0], b[1]];
    }
  }
  M_.ACP = ctx => new ACProbe(ctx.sim, ctx.el.name, inDesc(ctx, 0), null);
  M_.ACP2 = ctx => new ACProbe(ctx.sim, ctx.el.name, inDesc(ctx, 0), inDesc(ctx, 1), ctx.P.int('neg', 1) === 1);

})(SIM);

/* =====================================================================
 * 電路模擬器 — physical (SPICE-grade) semiconductor models
 *   - Diode / LED / Zener : SPICE level-1 diode (IS, N, RS, BV, IBV, CJO, VJ, M, FC, TT)
 *   - BJT                 : Gummel-Poon (IS, BF, BR, NF, NR, VAF, VAR, IKF, IKR,
 *                           ISE, NE, ISC, NC, RB, RC, RE, CJE, VJE, MJE, CJC, VJC, MJC, FC, TF, TR)
 *   - MOSFET              : Shichman-Hodges level 1 (VTO, KP, W, L, LAMBDA, RD, RS, CGS, CGD, CDS)
 *                           + body diode
 *   - Comparator          : open-collector comparator (LM393 / AS393 class) with supply pins,
 *                           finite gain, response time, saturating sink stage, input bias,
 *                           input clamp diodes and common-mode-range monitoring
 * All are solved by Newton iteration with SPICE junction-voltage limiting (pnjlim).
 * Charges use the simulator's integration (trapezoidal, backward Euler after events).
 * The t = 0 solution is a true DC operating point for these devices (charges open).
 * Temperature is fixed at 27 °C.
 * ===================================================================== */
'use strict';
(function (S) {
  const { aA, aG, aI } = S.st;
  const vx = S.vx;
  const M_ = S.MODELS;
  const SimError = S.SimError;
  const VT = 1.380649e-23 * 300.15 / 1.602176634e-19; // 25.865 mV at 27 °C
  const GMIN = 1e-12;
  const EXPLIM = 80;

  /* ------------------------------------------------------------------
   * SPICE .model text parser (SPICE suffixes: M = milli, MEG = mega)
   * ------------------------------------------------------------------ */
  const SFX = { t: 1e12, g: 1e9, meg: 1e6, k: 1e3, mil: 25.4e-6, m: 1e-3, u: 1e-6, 'µ': 1e-6, n: 1e-9, p: 1e-12, f: 1e-15 };
  function spiceNum(s) {
    const m = /^([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)(meg|mil|[tgkmunpfµ])?[a-zA-Z]*$/i.exec(String(s).trim());
    if (!m) return NaN;
    let v = parseFloat(m[1]);
    if (m[2]) v *= SFX[m[2].toLowerCase()];
    return v;
  }
  function parseSpiceModel(txt) {
    let s = String(txt || '').split(/\r?\n/).map(l => l.replace(/;.*$/, '')).filter(l => !/^\s*\*/.test(l)).map(l => l.replace(/^\s*\+/, ' ')).join(' ');
    s = s.replace(/^\s*\.model\s+\S+\s+[a-z]+\s*\(?/i, '').replace(/\)\s*$/, '').replace(/[()]/g, ' ');
    const out = {}; const bad = [];
    const re = /([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([^\s=,]+)/g; let m; let used = '';
    while ((m = re.exec(s))) {
      const k = m[1].toUpperCase();
      if (k === 'LEVEL' || k === 'MFG' || k === 'TYPE' || k === 'VCEO' || k === 'ICRATING' || k === 'IAVE' || k === 'VPK' || k === 'IPK' || k === 'IRATING') { used += m[0]; continue; }
      const v = spiceNum(m[2]);
      if (!isFinite(v)) { bad.push(m[0]); continue; }
      out[k] = v; used += m[0];
    }
    const rest = s.replace(re, ' ').trim();
    if (rest && /[A-Za-z0-9]/.test(rest)) bad.push(rest.slice(0, 40));
    return { p: out, bad };
  }
  S.parseSpiceModel = parseSpiceModel;

  /* ------------------------------------------------------------------
   * Junction helpers
   * ------------------------------------------------------------------ */
  function pnjlim(vnew, vold, vt, vcrit) {
    if (vnew > vcrit && Math.abs(vnew - vold) > 2 * vt) {
      if (vold > 0) { const arg = 1 + (vnew - vold) / vt; vnew = arg > 0 ? vold + vt * Math.log(arg) : vcrit; }
      else vnew = vt * Math.log(vnew / vt);
    }
    return vnew;
  }
  // safe exp with linear continuation (keeps Newton finite)
  function sexp(x) { if (x > EXPLIM) { const e = Math.exp(EXPLIM); return [e * (1 + x - EXPLIM), e]; } const e = Math.exp(x); return [e, e]; }
  // depletion charge and capacitance (SPICE)
  function depQ(v, cj0, vj, m, fc) {
    if (!(cj0 > 0)) return [0, 0];
    m = Math.min(m, 0.99); vj = Math.max(vj, 0.05);
    if (v < fc * vj) {
      const arg = 1 - v / vj, sarg = Math.pow(arg, -m);
      return [vj * cj0 * (1 - arg * sarg) / (1 - m), cj0 * sarg];
    }
    const f1 = vj * (1 - Math.pow(1 - fc, 1 - m)) / (1 - m), f2 = Math.pow(1 - fc, 1 + m), f3 = 1 - fc * (1 + m), v0 = fc * vj;
    return [cj0 * f1 + cj0 / f2 * (f3 * (v - v0) + m / (2 * vj) * (v * v - v0 * v0)), cj0 / f2 * (f3 + m * v / vj)];
  }
  const get = (p, k, d) => (p[k] !== undefined ? p[k] : d);
  function ignoredWarn(sim, el, p, known) {
    const ig = Object.keys(p).filter(k => !known.includes(k));
    if (ig.length) sim.warn(el.name + '：已忽略 SPICE 參數 ' + ig.join(', ') + '（溫度相關與高階效應未建模，模擬溫度固定 27 °C）。');
  }

  /* ------------------------------------------------------------------
   * Charge-storage bookkeeping shared by the devices.
   * For charge j: s[off+2j] = q at last accepted point, s[off+2j+1] = its current.
   * ------------------------------------------------------------------ */
  function qk(h, be) { return (be ? 1 : 2) / h; }
  function qCur(s, o, q, h, be) { return qk(h, be) * (q - s[o]) - (be ? 0 : s[o + 1]); }
  // accept a charge state. The trapezoidal rule rings (the charge current flips sign every step) when a
  // charge stops changing abruptly within one step, e.g. at the end of a diode's reverse recovery.
  // Signature: the current flips sign while the interval-average (backward-Euler) current is small;
  // the history is then reset to that average, which kills the oscillation after one step.
  // State per charge: [q, i, flipped-last-step]. A physical oscillation that is resolved by the step (several steps
  // per half period) flips sign at most every few steps; the numerical ringing flips on every step, so the reset
  // is only applied on the second consecutive flip (one flip alone is never touched).
  function qAcc(s, o, q, h, be) {
    const it = qCur(s, o, q, h, be);
    const flip = !be && h > 0 && it * s[o + 1] < 0;
    if (flip && s[o + 2] > 0) { const ib = (q - s[o]) / h; if (Math.abs(ib) < 0.25 * Math.abs(it)) { s[o] = q; s[o + 1] = ib; s[o + 2] = 0; return; } }
    s[o] = q; s[o + 1] = it; s[o + 2] = flip ? 1 : 0;
  }

  /* ------------------------------------------------------------------
   * Diode (also LED / Zener): anode a, cathode k
   * ------------------------------------------------------------------ */
  const D_KNOWN = ['IS', 'N', 'RS', 'BV', 'IBV', 'NBV', 'CJO', 'CJ0', 'VJ', 'M', 'FC', 'TT', 'EG', 'XTI'];
  class SpDiode {
    constructor(sim, el, a, k, p, opt) {
      opt = opt || {};
      this.isP = true; this.nl = true; this.nlInit = true; this.sim = sim; this.a = a; this.k = k;
      this.IS = get(p, 'IS', 1e-14); this.N = get(p, 'N', 1); this.RS = get(p, 'RS', 0);
      this.BV = get(p, 'BV', opt.bv !== undefined ? opt.bv : Infinity); this.IBV = get(p, 'IBV', 1e-3); this.NBV = get(p, 'NBV', this.N);
      this.CJO = get(p, 'CJO', get(p, 'CJ0', 0)); this.VJ = get(p, 'VJ', 1); this.M = get(p, 'M', 0.5); this.FC = get(p, 'FC', 0.5); this.TT = get(p, 'TT', 0);
      if (!(this.IS > 0) || !(this.N > 0)) throw new SimError(el.name + '：SPICE 參數 IS、N 必須大於 0', el.id);
      if (!(this.BV > 0)) this.BV = Infinity;
      this.vte = this.N * VT; this.vcrit = this.vte * Math.log(this.vte / (Math.SQRT2 * this.IS));
      this.ai = this.RS > 0 ? sim.newNode() : a;
      this.hasQ = this.CJO > 0 || this.TT > 0;
      sim.reqP(this, 's', 3);
      this.vOld = 0; this.limited = false;
    }
    conn() { return [[this.a, this.ai, this.k]]; }
    init() { this.s.fill(0); }
    vd(x) { return vx(x, this.ai) - vx(x, this.k); }
    stat(v) { // static current and conductance
      const [e, de] = sexp(v / this.vte);
      let i = this.IS * (e - 1), g = this.IS * de / this.vte;
      if (this.BV < Infinity) { const vtb = this.NBV * VT; const [e2, de2] = sexp(-(v + this.BV) / vtb); i -= this.IBV * e2; g += this.IBV * de2 / vtb; }
      return [i + GMIN * v, g + GMIN];
    }
    charge(v, id, gd) { const [q, c] = depQ(v, this.CJO, this.VJ, this.M, this.FC); return [q + this.TT * id, c + this.TT * gd]; }
    nlGuess(x, xr) { this.vOld = this.vd(xr || x); }
    limit(v) {
      let vl = pnjlim(v, this.vOld, this.vte, this.vcrit);
      if (this.BV < Infinity && v < Math.min(0, -this.BV + 10 * this.NBV * VT)) {
        const vtb = this.NBV * VT; let r = -(v + this.BV); r = pnjlim(r, -(this.vOld + this.BV), vtb, vtb * Math.log(vtb / (Math.SQRT2 * this.IBV))); vl = -(r + this.BV);
      }
      return vl;
    }
    stampG(M) { if (this.RS > 0) aG(M, this.a, this.ai, 1 / this.RS); }
    stampNL(M, x, h, be, init) {
      const v0 = this.vd(x); const v = this.limit(v0);
      this.limited = Math.abs(v - v0) > 1e-9 * Math.max(1, Math.abs(v0)); this.vOld = v;
      let [i, g] = this.stat(v);
      if (this.hasQ && !init) { const [q, c] = this.charge(v, i, g); const k = qk(h, be); i += qCur(this.s, 0, q, h, be); g += k * c; }
      this._g = g; this._ieq = i - g * v;
      aG(M, this.ai, this.k, g);
    }
    stampNLB(b) { aI(b, this.ai, this.k, this._ieq); }
    nlOk() { return !this.limited; }
    accept(x, t, h, be) {
      if (!this.hasQ) return;
      const v = this.vd(x); const [i, g] = this.stat(v); const [q] = this.charge(v, i, g);
      qAcc(this.s, 0, q, h, be);
    }
    acceptInit(x) { if (!this.hasQ) return; const v = this.vd(x); const [i, g] = this.stat(v); this.s[0] = this.charge(v, i, g)[0]; this.s[1] = 0; this.s[2] = 0; }
    current(x) { // anode current (for probes that ask)
      const v = this.vd(x); return this.stat(v)[0];
    }
  }
  S.SpDiode = SpDiode;

  /* ------------------------------------------------------------------
   * Gummel-Poon BJT. pol = +1 NPN, -1 PNP. External nodes C, B, E.
   * ------------------------------------------------------------------ */
  const Q_KNOWN = ['IS', 'BF', 'BR', 'NF', 'NR', 'VAF', 'VA', 'VAR', 'VB', 'IKF', 'IK', 'IKR', 'ISE', 'NE', 'ISC', 'NC', 'RB', 'RC', 'RE', 'CJE', 'VJE', 'PE', 'MJE', 'ME', 'CJC', 'VJC', 'PC', 'MJC', 'MC', 'FC', 'TF', 'TR', 'XCJC'];
  class SpBJT {
    constructor(sim, el, C, B, E, pol, p) {
      this.isP = true; this.nl = true; this.nlInit = true; this.sim = sim; this.pol = pol;
      Object.assign(this, { C, B, E });
      this.IS = get(p, 'IS', 1e-16); this.BF = get(p, 'BF', 100); this.BR = get(p, 'BR', 1); this.NF = get(p, 'NF', 1); this.NR = get(p, 'NR', 1);
      const vaf = get(p, 'VAF', get(p, 'VA', 0)), var_ = get(p, 'VAR', get(p, 'VB', 0));
      this.iVAF = vaf > 0 ? 1 / vaf : 0; this.iVAR = var_ > 0 ? 1 / var_ : 0;
      const ikf = get(p, 'IKF', get(p, 'IK', 0)), ikr = get(p, 'IKR', 0);
      this.iIKF = ikf > 0 ? 1 / ikf : 0; this.iIKR = ikr > 0 ? 1 / ikr : 0;
      this.ISE = get(p, 'ISE', 0); this.NE = get(p, 'NE', 1.5); this.ISC = get(p, 'ISC', 0); this.NC = get(p, 'NC', 2);
      this.RB = get(p, 'RB', 0); this.RC = get(p, 'RC', 0); this.RE = get(p, 'RE', 0);
      this.CJE = get(p, 'CJE', 0); this.VJE = get(p, 'VJE', get(p, 'PE', 0.75)); this.MJE = get(p, 'MJE', get(p, 'ME', 0.33));
      this.CJC = get(p, 'CJC', 0); this.VJC = get(p, 'VJC', get(p, 'PC', 0.75)); this.MJC = get(p, 'MJC', get(p, 'MC', 0.33));
      this.FC = get(p, 'FC', 0.5); this.TF = get(p, 'TF', 0); this.TR = get(p, 'TR', 0);
      if (!(this.IS > 0) || !(this.BF > 0) || !(this.BR > 0)) throw new SimError(el.name + '：SPICE 參數 IS、BF、BR 必須大於 0', el.id);
      this.Ci = this.RC > 0 ? sim.newNode() : C; this.Bi = this.RB > 0 ? sim.newNode() : B; this.Ei = this.RE > 0 ? sim.newNode() : E;
      this.vcrit = VT * Math.log(VT / (Math.SQRT2 * this.IS));
      sim.reqP(this, 's', 6);
      this.hasQ = this.CJE > 0 || this.CJC > 0 || this.TF > 0 || this.TR > 0;
      this.vbeO = 0; this.vbcO = 0; this.limited = false;
      this.F = new Float64Array(4); this.J = new Float64Array(8); this._b = new Float64Array(3);
    }
    conn() { return [[this.C, this.Ci, this.B, this.Bi, this.E, this.Ei]]; }
    init() { this.s.fill(0); }
    jv(x) { const p = this.pol, vb = vx(x, this.Bi); return [p * (vb - vx(x, this.Ei)), p * (vb - vx(x, this.Ci))]; }
    // static model with analytic derivatives (NPN sense, currents into C and B).
    // Fills this.F = [Ic, Ib, Qbe, Qbc] and this.J = d[Ic,Ib,Qbe,Qbc]/d[vbe,vbc] (row-major 4×2).
    f(vbe, vbc) {
      const F = this.F, J = this.J;
      const vtf = this.NF * VT, vtr = this.NR * VT;
      const [ebe] = sexp(vbe / vtf), [ebc] = sexp(vbc / vtr);
      const cbe = this.IS * (ebe - 1), cbc = this.IS * (ebc - 1), gbe = this.IS * ebe / vtf, gbc = this.IS * ebc / vtr;
      let cben = 0, gben = 0, cbcn = 0, gbcn = 0;
      if (this.ISE > 0) { const vt = this.NE * VT; const [e] = sexp(vbe / vt); cben = this.ISE * (e - 1); gben = this.ISE * e / vt; }
      if (this.ISC > 0) { const vt = this.NC * VT; const [e] = sexp(vbc / vt); cbcn = this.ISC * (e - 1); gbcn = this.ISC * e / vt; }
      const den = 1 - vbc * this.iVAF - vbe * this.iVAR;
      let q1, dq1e, dq1c;
      if (den > 1e-6) { q1 = 1 / den; dq1e = q1 * q1 * this.iVAR; dq1c = q1 * q1 * this.iVAF; } else { q1 = 1e6; dq1e = 0; dq1c = 0; }
      const q2 = cbe * this.iIKF + cbc * this.iIKR, dq2e = gbe * this.iIKF, dq2c = gbc * this.iIKR;
      const sq = Math.sqrt(1 + 4 * q2), qb = q1 * (1 + sq) / 2;
      const dqbe = dq1e * (1 + sq) / 2 + q1 * dq2e / sq, dqbc = dq1c * (1 + sq) / 2 + q1 * dq2c / sq;
      const iqb = 1 / qb, dif = cbe - cbc;
      F[0] = dif * iqb - cbc / this.BR - cbcn - GMIN * vbc;
      F[1] = cbe / this.BF + cben + cbc / this.BR + cbcn + GMIN * vbe + GMIN * vbc;
      J[0] = gbe * iqb - dif * iqb * iqb * dqbe;
      J[1] = -gbc * iqb - dif * iqb * iqb * dqbc - gbc / this.BR - gbcn - GMIN;
      J[2] = gbe / this.BF + gben + GMIN;
      J[3] = gbc / this.BR + gbcn + GMIN;
      if (this.hasQ) {
        const [qe, ce] = depQ(vbe, this.CJE, this.VJE, this.MJE, this.FC), [qc, cc] = depQ(vbc, this.CJC, this.VJC, this.MJC, this.FC);
        F[2] = qe + this.TF * cbe * iqb; F[3] = qc + this.TR * cbc;
        J[4] = ce + this.TF * (gbe * iqb - cbe * iqb * iqb * dqbe); J[5] = this.TF * (-cbe * iqb * iqb * dqbc);
        J[6] = 0; J[7] = cc + this.TR * gbc;
      } else { F[2] = F[3] = 0; J[4] = J[5] = J[6] = J[7] = 0; }
      return F;
    }
    nlGuess(x, xr) { [this.vbeO, this.vbcO] = this.jv(xr || x); }
    stampG(M) {
      if (this.RC > 0) aG(M, this.C, this.Ci, 1 / this.RC);
      if (this.RB > 0) aG(M, this.B, this.Bi, 1 / this.RB);
      if (this.RE > 0) aG(M, this.E, this.Ei, 1 / this.RE);
    }
    stampNL(M, x, h, be, init) {
      const [b0, c0] = this.jv(x);
      const vbe = pnjlim(b0, this.vbeO, VT * this.NF, this.vcrit), vbc = pnjlim(c0, this.vbcO, VT * this.NR, this.vcrit);
      this.limited = Math.abs(vbe - b0) > 1e-9 || Math.abs(vbc - c0) > 1e-9; this.vbeO = vbe; this.vbcO = vbc;
      const F = this.f(vbe, vbc), J = this.J;
      // terminal currents into C and B (NPN sense) incl. charge currents
      let iC = F[0], iB = F[1], gCbe = J[0], gCbc = J[1], gBbe = J[2], gBbc = J[3];
      if (this.hasQ && !init) {
        const k = qk(h, be);
        const iqbe = qCur(this.s, 0, F[2], h, be), iqbc = qCur(this.s, 3, F[3], h, be);
        iC -= iqbc; iB += iqbe + iqbc;
        gCbe -= k * J[6]; gCbc -= k * J[7];
        gBbe += k * (J[4] + J[6]); gBbc += k * (J[5] + J[7]);
      }
      const p = this.pol, bb = this._b;
      const stampRow = (X, I, gbe, gbc, o) => { aA(M, X, this.Bi, gbe + gbc); aA(M, X, this.Ei, -gbe); aA(M, X, this.Ci, -gbc); bb[o] = p * (I - gbe * vbe - gbc * vbc); };
      stampRow(this.Ci, iC, gCbe, gCbc, 0); stampRow(this.Bi, iB, gBbe, gBbc, 1); stampRow(this.Ei, -(iC + iB), -(gCbe + gBbe), -(gCbc + gBbc), 2);
    }
    stampNLB(b) { const bb = this._b; if (this.Ci >= 0) b[this.Ci] -= bb[0]; if (this.Bi >= 0) b[this.Bi] -= bb[1]; if (this.Ei >= 0) b[this.Ei] -= bb[2]; }
    nlOk() { return !this.limited; }
    accept(x, t, h, be) {
      if (!this.hasQ) return;
      const [vbe, vbc] = this.jv(x); const F = this.f(vbe, vbc);
      qAcc(this.s, 0, F[2], h, be); qAcc(this.s, 3, F[3], h, be);
    }
    acceptInit(x) { if (!this.hasQ) return; const [vbe, vbc] = this.jv(x); const F = this.f(vbe, vbc); this.s.fill(0); this.s[0] = F[2]; this.s[3] = F[3]; }
  }
  S.SpBJT = SpBJT;

  /* ------------------------------------------------------------------
   * Level-1 MOSFET channel + linear gate charges. pol = +1 N, -1 P.
   * Threshold is smoothed over a few mV (softplus) so Newton has a
   * continuous derivative; the effect on currents above threshold is nil.
   * ------------------------------------------------------------------ */
  const M_KNOWN = ['VTO', 'VT0', 'KP', 'W', 'L', 'LAMBDA', 'RD', 'RS', 'CGS', 'CGD', 'CDS', 'CGSO', 'CGDO', 'CBD', 'IS', 'N', 'RB', 'CJO', 'TT', 'BV'];
  class SpMOS {
    constructor(sim, el, D, G, Sn, pol, p) {
      this.isP = true; this.nl = true; this.nlInit = true; this.sim = sim; this.pol = pol;
      Object.assign(this, { D, G, Sn });
      this.VTO = get(p, 'VTO', get(p, 'VT0', pol > 0 ? 2 : -2)) * pol; // magnitude in device sense
      this.beta = get(p, 'KP', 2e-5) * get(p, 'W', 1) / get(p, 'L', 1); this.LAM = get(p, 'LAMBDA', 0);
      this.RD = get(p, 'RD', 0); this.RS = get(p, 'RS', 0);
      this.CGS = get(p, 'CGS', get(p, 'CGSO', 0)); this.CGD = get(p, 'CGD', get(p, 'CGDO', 0)); this.CDS = get(p, 'CDS', get(p, 'CBD', 0));
      if (!(this.beta > 0)) throw new SimError(el.name + '：SPICE 參數 KP 必須大於 0', el.id);
      this.Di = this.RD > 0 ? sim.newNode() : D; this.Si = this.RS > 0 ? sim.newNode() : Sn;
      this.vs = 1.5 * VT; // smoothing width
      sim.reqP(this, 's', 9);
      this.hasQ = this.CGS > 0 || this.CGD > 0 || this.CDS > 0;
      // charge functions in device sense (v = pol * physical voltage); each returns [Q(v), C(v)]
      const lin = C => v => [C * v, C];
      let qgd = lin(this.CGD), qds = lin(this.CDS);
      // gate-drain: two-level capacitance Cmin (drain far above gate) → Cmax (gate above drain), VDMOS style.
      // Cmax is set from the datasheet Qgd at Vds = VQ: Qgd = Cmin·VQ + (Cmax − Cmin)·Vk·ln2
      const QGD = get(p, 'QGD', 0), VQ = get(p, 'VQ', 400), VK = 2;
      if (QGD > 0 && this.CGD > 0) {
        const Cmin = this.CGD, Cmax = Math.max(Cmin, Cmin + (QGD - Cmin * VQ) / (VK * Math.LN2));
        const lncosh = x => { const a = Math.abs(x); return a + Math.log1p(Math.exp(-2 * a)) - Math.LN2; };
        qgd = v => [Cmin * v + (Cmax - Cmin) * 0.5 * (v + VK * lncosh(v / VK)), Cmin + (Cmax - Cmin) * 0.5 * (1 + Math.tanh(v / VK))]; // v = vgd
        if (Cmax === Cmin) sim.warn(el.name + '：Qgd 小於 Crss·Vq，米勒電容視為定值 Crss。');
      }
      // drain-source: junction-like 1/sqrt(1 + vds/V1) scaled so that C(VCAP) = CDS; constant below 0 V
      const VCAP = get(p, 'VCAP', 0), V1 = 1;
      if (VCAP > 0 && this.CDS > 0) {
        const C0 = this.CDS * Math.sqrt(1 + VCAP / V1);
        qds = v => v > 0 ? [2 * C0 * V1 * (Math.sqrt(1 + v / V1) - 1), C0 / Math.sqrt(1 + v / V1)] : [C0 * v, C0];
      }
      this.qf = [lin(this.CGS), qgd, qds];
    }
    // physical charge of capacitor index j at physical voltage v
    capQ(j, v) { const [q, c] = this.qf[j](this.pol * v); return [this.pol * q, c]; }
    conn() { return [[this.D, this.Di, this.Sn, this.Si]]; }
    init() { this.s.fill(0); }
    ids(vgs, vds) { // device sense, vds >= 0
      const u = (vgs - this.VTO) / this.vs; const sp = u > 30 ? u : Math.log1p(Math.exp(u)); const dsp = u > 30 ? 1 : 1 / (1 + Math.exp(-u));
      const vov = this.vs * sp, dvov = dsp; const cl = 1 + this.LAM * vds; const b = this.beta;
      if (vds < vov) { const i0 = b * (vov * vds - vds * vds / 2); return [i0 * cl, b * vds * cl * dvov, b * (vov - vds) * cl + i0 * this.LAM]; }
      const i0 = b / 2 * vov * vov; return [i0 * cl, b * vov * cl * dvov, i0 * this.LAM];
    }
    stampG(M) { if (this.RD > 0) aG(M, this.D, this.Di, 1 / this.RD); if (this.RS > 0) aG(M, this.Sn, this.Si, 1 / this.RS); }
    stampNL(M, x, h, be, init) {
      const p = this.pol; const vd = vx(x, this.Di), vg = vx(x, this.G), vsn = vx(x, this.Si);
      let vds = p * (vd - vsn), vgs = p * (vg - vsn); let X = this.Di, Y = this.Si, sw = false;
      if (vds < 0) { sw = true; vgs = p * (vg - vd); vds = -vds; X = this.Si; Y = this.Di; }
      const [id, gm, gds] = this.ids(vgs, vds);
      // current flows X -> Y (device sense, multiplied by pol for physical)
      // I = p*id(vgs, vds), vgs = p*(vG - vY), vds = p*(vX - vY)
      aA(M, X, this.G, gm); aA(M, X, Y, -gm - gds); aA(M, X, X, gds);
      aA(M, Y, this.G, -gm); aA(M, Y, Y, gm + gds); aA(M, Y, X, -gds);
      aG(M, this.Di, this.Si, GMIN);
      const ieq = p * (id - gm * vgs - gds * vds);
      this._b = [[X, ieq], [Y, -ieq]];
      if (this.hasQ && !init) {
        const k = qk(h, be); const caps = [[this.G, this.Si, this.CGS, 0], [this.G, this.Di, this.CGD, 3], [this.Di, this.Si, this.CDS, 6]];
        for (let j = 0; j < 3; j++) {
          const [a, c, C, o] = caps[j]; if (!(C > 0)) continue;
          const v = vx(x, a) - vx(x, c); const [q, Cv] = this.capQ(j, v); const i = qCur(this.s, o, q, h, be); const g = k * Cv;
          aG(M, a, c, g); this._b.push([a, i - g * v], [c, -(i - g * v)]);
        }
      }
    }
    stampNLB(b) { for (const [X, c] of this._b) if (X >= 0) b[X] -= c; }
    accept(x, t, h, be) {
      if (!this.hasQ) return;
      const caps = [[this.G, this.Si, this.CGS, 0], [this.G, this.Di, this.CGD, 3], [this.Di, this.Si, this.CDS, 6]];
      for (let j = 0; j < 3; j++) { const [a, c, C, o] = caps[j]; if (!(C > 0)) continue; qAcc(this.s, o, this.capQ(j, vx(x, a) - vx(x, c))[0], h, be); }
    }
    acceptInit(x) {
      if (!this.hasQ) return;
      const caps = [[this.G, this.Si, this.CGS, 0], [this.G, this.Di, this.CGD, 3], [this.Di, this.Si, this.CDS, 6]];
      for (let j = 0; j < 3; j++) { const [a, c, C, o] = caps[j]; this.s[o] = C > 0 ? this.capQ(j, vx(x, a) - vx(x, c))[0] : 0; this.s[o + 1] = 0; this.s[o + 2] = 0; }
    }
  }
  S.SpMOS = SpMOS;

  /* ------------------------------------------------------------------
   * Open-collector comparator (one channel of LM393 / AS393 / LM339 class)
   * pins: in+, in-, out, V+, GND
   * Internal state y in [-1, 1] (+1: in+ > in-, output off) follows
   *   tau dy/dt = tanh((v+ - v- + Vos)/Vlin) - y        (backward Euler)
   * Output sink:  I = Isc * (1 - y)/2 * fs(Vs) * tanh(Vce/Vk) + Ileak-conductance
   * ------------------------------------------------------------------ */
  class CompOC {
    constructor(sim, el, o) {
      this.isP = true; this.nl = true; this.nlInit = true; this.sw = true; this.sim = sim; this.el = el; Object.assign(this, o);
      sim.var(this, 'k'); sim.reqP(this, 's', 2); this.lteSkip = ['k']; // internal decision state, not a physical waveform
      this.a = 1 / this.tauY; this.Vk = this.vol / Math.atanh(Math.min(0.95, this.iol / this.isc));
      this.cm = [{ on: false, t0: 0, tot: 0, w: -1 }, { on: false, t0: 0, tot: 0, w: -1 }];
    }
    conn() { return [[this.ip, this.vcc, this.gnd], [this.inn, this.vcc, this.gnd], [this.out, this.gnd]]; }
    init() { this.s[0] = 0; this.s[1] = 0; }
    vd(x) { return vx(x, this.ip) - vx(x, this.inn) + this.vos; }
    state() { return this.s[1]; } setState(v) { this.s[1] = v; } skey() { return this.s[1]; }
    desired(x) { const v = this.vd(x); return v > 0 ? 1 : (v < 0 ? 0 : this.s[1]); }
    qq(xA, xB) { return [this.vd(xA), this.vd(xB)]; }
    tau(xA, xB) { const a = this.vd(xA), b = this.vd(xB); if (a === b || (a > 0) === (b > 0)) return NaN; return a / (a - b); }
    fs(vs) { const z = Math.tanh((vs - this.vsmin) / 0.1); return [0.5 * (1 + z), 0.5 * (1 - z * z) / 0.1]; }
    stampNL(M, x, h, be, init) {
      const k = this.k, g = this.gnd;
      // y equation (input decision is an event-located state, so Newton never sees the 200 V/mV gain)
      // target ±3 with the state clamped to ±1: crossing at tpd, rail reached in finite time
      const tgt = this.s[1] ? 3 : -3;
      if (init) { aA(M, k, k, 1); this._by = tgt / 3; }
      else { const ha = h * this.a; aA(M, k, k, 1 + ha); this._by = this.s[0] + ha * tgt; }
      // output sink current out -> gnd
      // output drive switches sharply when the internal state crosses 0 (50 % point = tpd)
      const y = Math.max(-1, Math.min(1, x[k])); const Ty = Math.tanh(y / 0.05); const sg = 0.5 * (1 - Ty), dsg = -0.5 * (1 - Ty * Ty) / 0.05;
      const vs = vx(x, this.vcc) - vx(x, g), vceR = vx(x, this.out) - vx(x, g);
      // limit the output-stage linearisation point (current-source region would let Newton overshoot)
      const vce = Math.max(vceR, Math.min(-0.3, this.vceO - 0.5)); this.limited = vce !== vceR; this.vceO = vce;
      const [f, df] = this.fs(vs); const Tc = Math.tanh(vce / this.Vk), dTc = (1 - Tc * Tc) / this.Vk;
      const I = this.isc * sg * f * Tc;
      const dIy = this.isc * dsg * f * Tc, dIce = this.isc * sg * f * dTc + 1 / this.roff, dIs = this.isc * sg * df * Tc;
      // stamp I(out->gnd) = I0 + dIy*(y-y0) + dIce*(vce-vce0) + dIs*(vs-vs0)
      const rowsO = [[this.out, 1], [g, -1]];
      for (const [r, sgn] of rowsO) {
        aA(M, r, k, sgn * dIy); aA(M, r, this.out, sgn * dIce); aA(M, r, g, -sgn * dIce); aA(M, r, this.vcc, sgn * dIs); aA(M, r, g, -sgn * dIs);
      }
      const Ieq = I + vce / this.roff - dIy * y - dIce * vce - dIs * vs;
      // quiescent supply current vcc -> gnd and input bias currents (vcc -> inputs)
      const Iq = this.iq * f, dIq = this.iq * df;
      aA(M, this.vcc, this.vcc, dIq); aA(M, this.vcc, g, -dIq); aA(M, g, this.vcc, -dIq); aA(M, g, g, dIq);
      this._bo = [Ieq, Iq - dIq * vs, this.ib * f];
    }
    stampNLB(b) {
      b[this.k] += this._by;
      const [Ieq, Iqeq, Ib] = this._bo;
      aI(b, this.out, this.gnd, Ieq);
      aI(b, this.vcc, this.gnd, Iqeq);
      aI(b, this.vcc, this.ip, Ib); aI(b, this.vcc, this.inn, Ib);
    }
    accept(x, t) {
      this.s[0] = Math.max(-1, Math.min(1, x[this.k]));
      // common-mode range monitor
      const g = vx(x, this.gnd), vs = vx(x, this.vcc) - g;
      if (!(vs > this.vsmin)) { this.cm.forEach((st, i) => { if (st.on) { st.on = false; st.tot += t - st.t0; this.report(i, t); } }); return; }
      const lim = vs - this.vcmr;
      [this.ip, this.inn].forEach((n, i) => {
        const st = this.cm[i]; const over = vx(x, n) - g > lim;
        if (over && !st.on) { st.on = true; st.t0 = t; }
        else if (!over && st.on) { st.on = false; st.tot += t - st.t0; this.report(i, t); }
      });
    }
    report(i, t) {
      const st = this.cm[i]; const sim = this.sim; const tot = st.tot + (st.on ? t - st.t0 : 0);
      const msg = this.el.name + '：' + (i ? 'IN−' : 'IN+') + ' 超出輸入共模範圍上限（V+ − ' + this.vcmr + ' V），累計約 ' + (tot * 1e3).toPrecision(3) + ' ms。超出範圍時實際元件的輸出狀態不保證（另一輸入在範圍內時通常仍正確）。';
      if (st.w >= 0 && sim.warnings[st.w] && sim.warnings[st.w].startsWith(this.el.name + '：' + (i ? 'IN−' : 'IN+'))) sim.warnings[st.w] = msg;
      else { sim.warn(msg); st.w = sim.warnings.indexOf(msg); }
    }
    finish(t) { this.cm.forEach((st, i) => { if (st.on || st.tot > 0) this.report(i, t); }); }
    nlGuess(x) { this.vceO = vx(x, this.out) - vx(x, this.gnd); }
    nlOk() { return !this.limited; }
    acceptInit(x) { this.s[0] = Math.max(-1, Math.min(1, x[this.k])); }
  }
  S.CompOC = CompOC;

  /* ------------------------------------------------------------------
   * TL431 shunt regulator (behavioural, datasheet-typical values)
   * pins K, A, REF. Internal state X = error-amplifier output (V), single pole:
   *   tau dX/dt + X = u,  u = (Vref - VA) - Vref0 - kvk*(Vka - Vref0) + x0
   * Cathode current (K -> A):
   *   core(Y) = 0.03·wl·softplus((Y - c1)/wl) + (gm - 0.03)·w2·softplus(Y/w2),  Y = (1-r)X + r·u
   *   I = core·Is(Vka)/(core + Is(Vka)) + Vka/Roff
   * low-gm bias region (0.03 A/V) up to Imin, gm = 1/Zka above (convex, Newton-safe);
   * Is = output saturation below about 2 V; r = Zka/Zhf limits |Zka| at high frequency;
   * x0 puts Ika = 10 mA at Vref = Vref0 with REF tied to K.
   * ------------------------------------------------------------------ */
  class Tl431 {
    constructor(sim, el, K, A, R, o) {
      this.isP = true; this.nl = true; this.nlInit = true; this.sim = sim; this.el = el;
      Object.assign(this, { K, A, R }, o);
      sim.var(this, 'k'); sim.reqP(this, 's', 2); this.lteSkip = ['k'];
      this.gm = 1 / o.zka; this.w2 = 2e-5; this.glow = 0.03; this.wl = 1.5e-3; this.c1 = -o.imin / this.glow; this.Goff = 1e-9;
      this.tau = 1 / (2 * Math.PI * o.fp); this.r = Math.min(0.5, o.zka / o.zhf);
      // x0: core current = 10 mA
      let lo = -0.2, hi = 0.2;
      for (let i = 0; i < 200; i++) { const m = 0.5 * (lo + hi); if (this.core(m)[0] > 10e-3) hi = m; else lo = m; }
      this.x0 = 0.5 * (lo + hi);
      this.uc = -o.vref + o.kvk * o.vref + this.x0;
    }
    conn() { return [[this.K, this.A, this.R]]; }
    init() { this.s[0] = 0; this.s[1] = 0; }
    // convex static transfer: low-gm bias region (0.03 A/V up to Imin) + high-gm region (1/Zka)
    core(Y) {
      const sp = z => (z > 40 ? z : (z < -40 ? Math.exp(z) : Math.log1p(Math.exp(z)))), lg = z => (z > 40 ? 1 : (z < -40 ? Math.exp(z) : 1 / (1 + Math.exp(-z))));
      const z1 = (Y - this.c1) / this.wl, z2 = Y / this.w2;
      return [this.glow * this.wl * sp(z1) + (this.gm - this.glow) * this.w2 * sp(z2), this.glow * lg(z1) + (this.gm - this.glow) * lg(z2)];
    }
    // output saturation (cathode below about 2 V): I = core·Is/(core + Is)
    isat(v) { const z = (v - 1.6) / 0.05; const sp = z > 40 ? z : (z < -40 ? Math.exp(z) : Math.log1p(Math.exp(z))); const lg = z > 40 ? 1 : (z < -40 ? Math.exp(z) : 1 / (1 + Math.exp(-z))); return [2 * 0.05 * sp + 1e-12, 2 * lg]; }
    Lv(x) { const vA = vx(x, this.A); return vx(x, this.R) - vA - this.kvk * (vx(x, this.K) - vA); }
    stampNL(M, x, h, be, init) {
      const k = this.k, K = this.K, A = this.A, R = this.R, kv = this.kvk;
      // error-amplifier state equation:  cX*X - a*L(v) = rhs
      let a, rhs;
      if (init) { a = 1; rhs = this.uc; aA(M, k, k, 1); }
      else if (be) { a = h / this.tau; rhs = this.s[0] + a * this.uc; aA(M, k, k, 1 + a); }
      else { a = 0.5 * h / this.tau; rhs = this.s[0] + a * (this.uc + this.s[1] - this.s[0]); aA(M, k, k, 1 + a); }
      aA(M, k, R, -a); aA(M, k, A, a * (1 - kv)); aA(M, k, K, a * kv);
      this._bk = rhs;
      // cathode current
      // output command Y = (1-r)·X + r·u : the feedthrough r = Zka/Zhf limits the cathode impedance at high frequency
      const X = x[k], vkaR = vx(x, K) - vx(x, A), L0 = this.Lv(x), r = this.r;
      const YR = (1 - r) * X + r * (L0 + this.uc);
      // Newton step limiting in the exponential tails only (SPICE pnjlim style); the transfer is convex otherwise
      let Y = YR, vka = vkaR;
      if (this.yO !== undefined) {
        const lim = (vn, vo, th, w) => (vn > vo && vo < th && vn - vo > 2 * w) ? Math.max(vo + w * Math.log(1 + (vn - vo) / w), Math.min(vn, th)) : vn;
        Y = lim(Y, this.yO, this.c1, this.wl); Y = lim(Y, this.yO, 0, this.w2 * 50);
        vka = lim(vka, this.vO, 1.6, 0.05);
      }
      this.limited = Math.abs(Y - YR) > 1e-12 || Math.abs(vka - vkaR) > 1e-9; this.yO = Y; this.vO = vka;
      const [c, dc] = this.core(Y); const [is, dis] = this.isat(vka); const q = c + is;
      const I = c * is / q + vka * this.Goff, gY = dc * is * is / (q * q), gV = dis * c * c / (q * q) + this.Goff;
      const gX = gY * (1 - r), gL = gY * r;
      for (const [row, sg] of [[K, 1], [A, -1]]) { aA(M, row, k, sg * gX); aA(M, row, R, sg * gL); aA(M, row, A, sg * gL * (-1 + kv)); aA(M, row, K, -sg * gL * kv); }
      aG(M, K, A, gV);
      this._ieq = I + gY * (r * this.uc - Y) - gV * vka;
      // REF input bias current (REF -> A)
      const vr = vx(x, R) - vx(x, A); const t2 = Math.tanh((vr - 1) / 0.2);
      const Ir = this.iref * 0.5 * (1 + t2) + GMIN * vr, gr = this.iref * 0.5 * (1 - t2 * t2) / 0.2 + GMIN;
      aG(M, R, A, gr); this._ireq = Ir - gr * vr;
    }
    stampNLB(b) { b[this.k] += this._bk; aI(b, this.K, this.A, this._ieq); aI(b, this.R, this.A, this._ireq); }
    nlGuess(x) { const X = x[this.k], r = this.r; this.yO = (1 - r) * X + r * (this.Lv(x) + this.uc); this.vO = vx(x, this.K) - vx(x, this.A); }
    nlOk() { return !this.limited; }
    accept(x) { this.s[0] = x[this.k]; this.s[1] = this.Lv(x) + this.uc; }
    acceptInit(x) { this.accept(x); }
  }
  S.Tl431 = Tl431;

  /* ------------------------------------------------------------------
   * Optocoupler output coupling: photocurrent eta_j * I_LED(junction) injected
   * collector -> internal base of each Gummel-Poon phototransistor.
   * Must come after its LED in the element list (uses the LED's Newton point).
   * ------------------------------------------------------------------ */
  class OptoLink {
    constructor(sim, led, outs) { this.isP = true; this.nl = true; this.nlInit = true; this.sim = sim; this.led = led; this.outs = outs; }
    conn() { return this.outs.map(o => [o.C, o.B]); }
    fwd(v) { const L = this.led; const [e, de] = sexp(v / L.vte); return [L.IS * (e - 1), L.IS * de / L.vte]; }
    stampNL(M) {
      const L = this.led; const v = L.vOld; const [i, g] = this.fwd(v); const ai = L.ai, kk = L.k;
      this._b = [];
      for (const o of this.outs) {
        const G = o.eta * g;
        aA(M, o.C, ai, G); aA(M, o.C, kk, -G); aA(M, o.B, ai, -G); aA(M, o.B, kk, G);
        this._b.push([o.C, o.B, o.eta * i - G * v]);
      }
    }
    stampNLB(b) { for (const [c, bb, I] of this._b) aI(b, c, bb, I); }
  }
  function optoMake(ctx, nOut) {
    const P = ctx.P, sim = ctx.sim, el = ctx.el;
    const a = ctx.p(0), k = ctx.p(1);
    const vf = P.pos('vf'), ifv = P.pos('ifv'), ifn = P.pos('ifn'), bf = P.pos('bf', 400), ccb = P.num('ccb', 5e-12), tf = P.num('tf', 5.5e-9);
    const N = 1.5, RS = 1.0; const vj = vf - ifv * RS;
    if (!(vj > 0.3)) throw ctx.err('LED Vf 與測試電流不合理（扣除串聯電阻後的接面電壓過低）');
    const led = new SpDiode(sim, el, a, k, { IS: ifv / Math.exp(vj / (N * VT)), N, RS, CJO: 30e-12, VJ: 1.2, M: 0.4 });
    const list = [led]; const outs = [];
    for (let j = 0; j < nOut; j++) {
      const ctr = P.num(nOut === 1 ? 'ctr' : 'ctr' + (j + 1)) / 100;
      if (!(ctr > 0)) throw ctx.err('CTR 必須大於 0');
      const C = ctx.p(2 + 2 * j), E = ctx.p(3 + 2 * j), B = sim.newNode();
      const q = new SpBJT(sim, el, C, B, E, 1, { IS: 1e-14, BF: bf, BR: 0.1, NF: 1, VAF: 100, IKF: 0.1, RC: 5, CJE: 10e-12, CJC: ccb, VJC: 0.75, MJC: 0.33, TF: tf, TR: 100e-9 });
      // calibrate eta so that Ic(terminal) = CTR*IF at IF = ifn, VCE = 5 V (base open)
      const icOf = (Ip) => {
        let lo = 0, hi = 1.3;
        for (let i = 0; i < 80; i++) { const m = 0.5 * (lo + hi); const F = q.f(m, m - 5); if (F[1] > Ip) hi = m; else lo = m; }
        const vb = 0.5 * (lo + hi); return q.f(vb, vb - 5)[0] + Ip;
      };
      const target = ctr * ifn; let lo = Math.log(target * 1e-6), hi = Math.log(target * 2);
      for (let i = 0; i < 60; i++) { const m = 0.5 * (lo + hi); if (icOf(Math.exp(m)) > target) hi = m; else lo = m; }
      outs.push({ C, B, eta: Math.exp(0.5 * (lo + hi)) / ifn });
      list.push(q);
    }
    list.push(new OptoLink(sim, led, outs));
    return list;
  }
  M_.OPTO1 = ctx => optoMake(ctx, 1);
  M_.OPTO2 = ctx => optoMake(ctx, 2);

  M_.TL431 = ctx => {
    const P = ctx.P;
    const o = { vref: P.pos('vref'), iref: Math.max(0, P.num('iref', 2e-6)), imin: P.pos('imin'), zka: P.pos('zka'), kvk: P.num('kvk', -1.4e-3), fp: P.pos('fp'), zhf: P.pos('zhf', 10) };
    const K = ctx.p(0), A = ctx.p(1), R = ctx.p(2);
    return [new Tl431(ctx.sim, ctx.el, K, A, R, o), new SpDiode(ctx.sim, ctx.el, A, K, { IS: 1e-14, N: 1 })];
  };

  /* ------------------------------------------------------------------
   * Gate driver for the non-ideal switches: control gate (0/1) selects
   * Von / Voff, applied to the internal gate node through Rg (Norton form).
   * Uses the simulator's exact gate-edge timing (gated element).
   * ------------------------------------------------------------------ */
  class GateDrv {
    constructor(sim, gi, sN, gn, vOn, vOff, rg, inv) {
      this.isP = true; this.sim = sim; Object.assign(this, { gi, sN, gn, vOn, vOff }); this.ginv = !!inv; this.gc = 1 / rg;
      sim.reqP(this, 's', 3); this.gIdx = 1; this.fIdx = 2; this.cins = [gn];
    }
    conn() { return [[this.gi, this.sN]]; }
    init() { this.s[0] = 0; this.s[1] = 0; this.s[2] = -1; }
    onGate(g) { const on = this.ginv ? 1 - g : g; if (on !== this.s[0]) { this.s[0] = on; return true; } return false; }
    stampG(M) { aG(M, this.gi, this.sN, this.gc); }
    stampB(b) { aI(b, this.sN, this.gi, (this.s[0] ? this.vOn : this.vOff) * this.gc); }
  }
  S.GateDrv = GateDrv;
  function dtWarn(ctx, tg, what) {
    const dt = ctx.sim.dt;
    if (dt > tg) ctx.sim.warn(ctx.el.name + '：' + what + ' ≈ ' + (tg * 1e9).toPrecision(3) + ' ns，時間步長 ' + (dt * 1e9).toPrecision(3) + ' ns 太大，切換暫態（米勒平台、切換損失、反向恢復）無法解析；結果仍穩定，但切換細節與損失不準。建議時間步長 ≤ ' + (tg / 5 * 1e9).toPrecision(2) + ' ns。');
  }
  function bodyDiode(ctx, a, k) {
    const P = ctx.P; const vf = P.pos('vfd'), iff = P.pos('ifd'), tt = Math.max(0, P.num('ttd', 0));
    return new SpDiode(ctx.sim, ctx.el, a, k, { IS: iff / Math.exp(vf / VT), N: 1, TT: tt });
  }
  function niMos(ctx, pol) {
    const P = ctx.P, sim = ctx.sim, el = ctx.el;
    const d = ctx.p(0), s = ctx.p(1), gn = ctx.c(2);
    const ron = P.pos('ron'), vth = P.pos('vth'), von = P.num('von'), voff = P.num('voff', 0), rg = P.pos('rg');
    const ciss = P.pos('ciss'), crss = P.num('crss'), coss = P.num('coss');
    if (!(crss >= 0 && crss < ciss)) throw ctx.err('Crss 必須介於 0 與 Ciss 之間');
    if (!(coss >= crss)) throw ctx.err('Coss 不可小於 Crss');
    if (!(von > vth)) throw ctx.err('閘極驅動電壓必須大於臨界電壓');
    const kp = 1 / (ron * (von - vth));
    const gi = sim.newNode();
    // a device probe (DEVP) on this element needs the terminal current: insert a zero-volt ammeter in the drain lead
    let dd = d, amm = null;
    if (sim.devWant && sim.devWant.has(el.name)) { dd = sim.newNode(); amm = new S.Amm(sim, d, dd); }
    const ch = new SpMOS(sim, el, dd, gi, s, pol, { VTO: pol * vth, KP: kp, LAMBDA: P.num('lam', 0), CGS: ciss - crss, CGD: crss, CDS: coss - crss, QGD: P.num('qgd', 0), VQ: P.num('vq', 400), VCAP: P.num('vcap', 0) });
    const drv = new GateDrv(sim, gi, s, gn, pol * von, pol * voff, rg, pol < 0);
    dtWarn(ctx, rg * ciss, '閘極時間常數 Rg·Ciss');
    const list = [drv, ch, pol > 0 ? bodyDiode(ctx, s, dd) : bodyDiode(ctx, dd, s)];
    if (amm) { list.unshift(amm); sim.devs[el.name] = { kind: 'MOS', g: gi, d, s, amm, drv, names: ['Vgs', 'Vds', 'Id', 'Ig'] }; }
    return list;
  }
  M_.MOSNI = ctx => niMos(ctx, 1);
  M_.PMOSNI = ctx => niMos(ctx, -1);
  // IGBT = MOSFET driving the base of a wide-base PNP (emitter = IGBT collector)
  M_.IGBTNI = ctx => {
    const P = ctx.P, sim = ctx.sim, el = ctx.el;
    const c = ctx.p(0), e = ctx.p(1), gn = ctx.c(2);
    const vth = P.pos('vth'), vce = P.pos('vce'), icn = P.pos('icn'), beta = P.pos('beta', 0.5), ttail = P.pos('ttail');
    const cies = P.pos('cies'), cres = P.num('cres'), coes = P.num('coes'), von = P.num('von'), voff = P.num('voff', 0), rg = P.pos('rg');
    if (!(cres >= 0 && cres < cies)) throw ctx.err('Cres 必須介於 0 與 Cies 之間');
    if (!(coes >= cres)) throw ctx.err('Coes 不可小於 Cres');
    if (!(von > vth)) throw ctx.err('閘極驅動電壓必須大於臨界電壓');
    const VEB = 0.8; const vds = vce - VEB, vov = von - vth;
    if (!(vds > 0.05)) throw ctx.err('Vce(sat) 必須大於約 0.85 V（內部 PNP 射-基極壓降約 0.8 V）');
    if (!(vds < vov)) throw ctx.err('Vce(sat) 過大或驅動電壓過低（MOS 通道未在線性區）');
    const imos = icn / (1 + beta);
    const kp = imos / (vov * vds - vds * vds / 2);
    const bn = sim.newNode(), gi = sim.newNode();
    let cc = c, amm = null;
    if (sim.devWant && sim.devWant.has(el.name)) { cc = sim.newNode(); amm = new S.Amm(sim, c, cc); }
    const mos = new SpMOS(sim, el, bn, gi, e, 1, { VTO: vth, KP: kp, CGS: cies - cres, CGD: cres, CDS: coes - cres, QGD: P.num('qgd', 0), VQ: P.num('vq', 600), VCAP: P.num('vcap', 0) });
    const pnp = new SpBJT(sim, el, e, bn, cc, -1, { IS: beta * imos / Math.exp(VEB / VT), BF: beta, BR: 0.1, NF: 1, TF: ttail / beta });
    const drv = new GateDrv(sim, gi, e, gn, von, voff, rg, false);
    dtWarn(ctx, rg * cies, '閘極時間常數 Rg·Cies');
    const list = [drv, mos, pnp, bodyDiode(ctx, e, cc)];
    if (amm) { list.unshift(amm); sim.devs[el.name] = { kind: 'IGBT', g: gi, d: c, s: e, amm, drv, names: ['Vge', 'Vce', 'Ic', 'Ig'] }; }
    return list;
  };

  /* ------------------------------------------------------------------
   * Device probe: internal quantities of a non-ideal switch (gate voltage,
   * terminal voltage/current, instantaneous power, accumulated energy).
   * Implemented as a control block so the energy integral is advanced on
   * every accepted (sub)step with the trapezoidal rule.
   * ------------------------------------------------------------------ */
  class DevProbe {
    constructor(sim, el, target, q) {
      this.isC = true; this.probe = true; this.pname = el.name; this.target = target; this.q = q;
      this.o = sim.newCNode(); this.outs = [this.o]; this.ins = []; this.ft = true;
      sim.reqC(this, 's', 2); // [energy, previous power]
    }
    link(sim) {
      const dv = sim.devs[this.target];
      if (!dv) throw new S.SimError(this.el.name + '：找不到名為「' + this.target + '」的非理想開關（MOSNI / PMOSNI / IGBTNI）', this.el.id);
      this.dv = dv;
    }
    init() { this.s.fill(0); }
    eval(t, h, sim) {
      const x = sim.x, dv = this.dv, drv = dv.drv;
      const vg = vx(x, dv.g) - vx(x, dv.s), vd = vx(x, dv.d) - vx(x, dv.s), id = x[dv.amm.k];
      const ig = drv.gc * ((drv.s[0] ? drv.vOn : drv.vOff) - vg);
      const p = vd * id;
      const E = h > 0 ? this.s[0] + 0.5 * h * (p + this.s[1]) : this.s[0];
      this.pv = p; this.pE = E;
      let v;
      switch (this.q) { case 'vg': v = vg; break; case 'vd': v = vd; break; case 'id': v = id; break; case 'ig': v = ig; break; case 'e': v = E; break; default: v = p; }
      sim.cv[this.o] = v;
    }
    commit() { this.s[0] = this.pE; this.s[1] = this.pv; }
    value(sim) { return sim.cv[this.o]; }
    stampAC(C) { const r = C.ci(this.o); C.a(r, r, 1); }
  }
  M_.DEVP = ctx => new DevProbe(ctx.sim, ctx.el, String(ctx.el.params.el || '').trim(), String(ctx.el.params.q || 'p'));

  /* ------------------------------------------------------------------
   * Presets (published SPICE models; temperature/high-order terms ignored)
   * ------------------------------------------------------------------ */
  const PRESETS = {
    D: {
      '1N4148': 'IS=2.52n RS=0.568 N=1.752 CJO=4p M=0.4 TT=20n BV=100 IBV=100u',
      '1N4007': 'IS=7.02767n RS=0.0341512 N=1.80803 BV=1000 IBV=0.1 CJO=10p VJ=0.7 M=0.5 FC=0.5 TT=100n',
      '1N5819': 'IS=31.7u RS=0.051 N=1.373 CJO=110p M=0.35 VJ=0.38 BV=40 IBV=1m',
      'LEDRED': 'IS=93.2p RS=42m N=3.73 BV=4 IBV=10u CJO=2.97p VJ=0.75 M=0.333 TT=4.32u',
      'ZENER': 'IS=5n N=1.2 RS=2 CJO=100p VJ=0.75 M=0.33 IBV=5m'
    },
    NPN: {
      '2N3904': 'IS=6.734f VAF=74.03 BF=416.4 NE=1.259 ISE=6.734f IKF=66.78m BR=.7371 NC=2 ISC=0 IKR=0 RC=1 CJC=3.638p MJC=.3085 VJC=.75 FC=.5 CJE=4.493p MJE=.2593 VJE=.75 TR=239.5n TF=301.2p RB=10',
      '2N4401': 'IS=26.03f VAF=90.7 BF=4.292K NE=1.244 ISE=26.03f IKF=.2061 BR=1.01 NC=2 ISC=0 IKR=0 RC=.5 CJC=11.01p MJC=.3763 VJC=.75 FC=.5 CJE=24.07p MJE=.3641 VJE=.75 TR=233.7n TF=466.5p RB=10',
      '2N2222A': 'IS=14.34f VAF=74.03 BF=255.9 NE=1.307 ISE=14.34f IKF=.2847 BR=6.092 NC=2 ISC=0 IKR=0 RC=1 CJC=7.306p MJC=.3416 VJC=.75 FC=.5 CJE=22.01p MJE=.377 VJE=.75 TR=46.91n TF=411.1p RB=10'
    },
    PNP: {
      '2N3906': 'IS=1.41f VAF=18.7 BF=180.7 NE=1.5 ISE=0 IKF=80m BR=4.977 NC=2 ISC=0 IKR=0 RC=2.5 CJC=9.728p MJC=.5776 VJC=.75 FC=.5 CJE=8.063p MJE=.3677 VJE=.75 TR=33.42n TF=179.3p RB=10'
    },
    NMOS: { 'GENERIC_N': 'VTO=2.1 KP=0.105 LAMBDA=0.01 CGS=25p CGD=5p CDS=10p' },
    PMOS: { 'GENERIC_P': 'VTO=-2 KP=0.05 LAMBDA=0.01 CGS=30p CGD=6p CDS=12p' },
    COMP: {
      // vos [V], ib [A], tpd [s], isc [A], vol [V] @ iol [A], vcmr [V], vsmin [V], iq [A]
      'LM393': { vos: 0, ib: 25e-9, tpd: 1.3e-6, isc: 16e-3, vol: 0.15, iol: 4e-3, vcmr: 1.5, vsmin: 2, iq: 0.2e-3 },
      'LM339': { vos: 0, ib: 25e-9, tpd: 1.3e-6, isc: 16e-3, vol: 0.15, iol: 4e-3, vcmr: 1.5, vsmin: 2, iq: 0.2e-3 },
      'LM2903': { vos: 0, ib: 25e-9, tpd: 1.5e-6, isc: 16e-3, vol: 0.15, iol: 4e-3, vcmr: 1.5, vsmin: 2, iq: 0.2e-3 }
    }
  };
  S.DEV_PRESETS = PRESETS;

  /* ------------------------------------------------------------------
   * Factories used by engine_power model functions
   * ------------------------------------------------------------------ */
  function spiceParams(ctx, kind) {
    const el = ctx.el; const model = String(ctx.P.raw('model', 'ideal'));
    if (kind === 'D' && model === 'ds') {
      // datasheet parameters -> SPICE level-1 diode
      const P = ctx.P; const vf = P.pos('vfd'), iff = P.pos('ifd'), rd = Math.max(0, P.num('rd', 0)), trr = Math.max(0, P.num('trr', 0)), qrr = Math.max(0, P.num('qrr', 0));
      const irr = P.num('irr', 0) > 0 ? P.num('irr') : iff; const cj = Math.max(0, P.num('cjd', 0)), vcj = Math.max(0, P.num('vcj', 0)), bv = P.num('bvd', 0);
      const vj = vf - iff * rd; if (!(vj > 0.05)) throw new SimError(el.name + '：Vf − If·Rd 必須大於約 0.05 V（Rd 太大）', el.id);
      const p = { IS: iff / Math.expm1(vj / VT), N: 1, RS: rd, TT: qrr > 0 ? qrr / irr : trr, CJO: cj * Math.sqrt(1 + vcj / 0.7), VJ: 0.7, M: 0.5 };
      if (bv > 0) { p.BV = bv; p.IBV = 1e-3; }
      return p;
    }
    let txt = ctx.P.raw('spice', '');
    if (!String(txt).trim()) {
      const pre = PRESETS[kind] || {};
      if (pre[model]) txt = pre[model];
      else if (model === 'custom') ctx.sim.warn(el.name + '：選擇自訂模型但未填 SPICE 參數，使用 SPICE 預設值。');
      else txt = Object.values(pre)[0] || ''; // parameter absent (older file / script): first preset of this kind
    }
    const r = parseSpiceModel(txt);
    if (r.bad.length) throw new SimError(el.name + '：SPICE 參數無法解析：' + r.bad.join('、'), el.id);
    return r.p;
  }
  S.devMake = {
    diode(ctx, a, k, opt) {
      const p = spiceParams(ctx, 'D'); ignoredWarn(ctx.sim, ctx.el, p, D_KNOWN);
      return new SpDiode(ctx.sim, ctx.el, a, k, p, opt);
    },
    bjt(ctx, pol) {
      const p = spiceParams(ctx, pol > 0 ? 'NPN' : 'PNP'); ignoredWarn(ctx.sim, ctx.el, p, Q_KNOWN);
      return new SpBJT(ctx.sim, ctx.el, ctx.p(0), ctx.p(2), ctx.p(1), pol, p);
    },
    mos(ctx, pol) {
      const p = spiceParams(ctx, pol > 0 ? 'NMOS' : 'PMOS'); ignoredWarn(ctx.sim, ctx.el, p, M_KNOWN);
      const d = ctx.p(0), s = ctx.p(1), g = ctx.p(2);
      const ch = new SpMOS(ctx.sim, ctx.el, d, g, s, pol, p);
      const bd = { IS: get(p, 'IS', 1e-14), N: get(p, 'N', 1), RS: get(p, 'RB', 0), CJO: get(p, 'CJO', 0), TT: get(p, 'TT', 0) };
      if (p.BV) bd.BV = p.BV;
      const body = pol > 0 ? new SpDiode(ctx.sim, ctx.el, s, d, bd) : new SpDiode(ctx.sim, ctx.el, d, s, bd);
      return [ch, body];
    }
  };

  M_.COMPOC = ctx => {
    const P = ctx.P; const model = String(P.raw('model', 'LM393'));
    const pre = PRESETS.COMP[model] || PRESETS.COMP.LM393;
    const num = (k, d) => P.num(k, d);
    const o = {
      ip: ctx.p(0), inn: ctx.p(1), out: ctx.p(2), vcc: ctx.p(3), gnd: ctx.p(4),
      vos: num('vos', pre.vos), ib: num('ib', pre.ib), tauY: Math.max(1e-12, num('tpd', pre.tpd) / Math.log(4 / 3)),
      isc: Math.max(1e-6, num('isc', pre.isc)), vol: Math.max(1e-3, num('vol', pre.vol)), iol: Math.max(1e-9, num('iol', pre.iol)),
      vcmr: num('vcmr', pre.vcmr), vsmin: num('vsmin', pre.vsmin), iq: num('iq', pre.iq), roff: 1e10
    };
    if (o.iol >= o.isc) throw ctx.err('Vol 量測電流必須小於短路吸入電流');
    const c = new CompOC(ctx.sim, ctx.el, o);
    // input clamp (substrate) diodes: GND -> each input
    const cl = { IS: 1e-14, N: 1 };
    return [c, new SpDiode(ctx.sim, ctx.el, o.gnd, o.ip, cl), new SpDiode(ctx.sim, ctx.el, o.gnd, o.inn, cl)];
  };
})(SIM);

/* =====================================================================
 * 電路模擬器 — control-circuit models
 * Each block reads sim.cv (values at the end of the present interval)
 * and sim.cvP (values at its start). Outputs are written in eval();
 * internal state changes only in commit(), so eval() may be repeated
 * (algebraic loops, switching-instant re-solves).
 * sim.cev[node] carries the exact time of a discrete jump, when known.
 * ===================================================================== */
'use strict';
(function (S) {
  const M_ = S.MODELS;
  const D2R = Math.PI / 180, R2D = 180 / Math.PI;
  const hi = v => v > 0.5;

  // event time of a discrete output change, from inputs that jumped
  function inEvt(sim, ins) { let e = NaN; for (const i of ins) { const v = sim.cev[i]; if (v === v && !(v <= e)) e = v; } return e; }
  // crossing time of (a - b) through 0 within the interval
  function crossT(sim, t, h, a, b, th) {
    const ea = sim.cev[a], eb = b >= 0 ? sim.cev[b] : NaN;
    if (ea === ea || eb === eb) return Math.max(ea === ea ? ea : -Infinity, eb === eb ? eb : -Infinity);
    const dA = sim.cvP[a] - (b >= 0 ? sim.cvP[b] : th), dB = sim.cv[a] - (b >= 0 ? sim.cv[b] : th);
    if (h > 0 && dA !== dB && (dA > 0) !== (dB > 0)) return t - h + h * dA / (dA - dB);
    return NaN;
  }
  // rising-edge (through 0.5) detection with event time
  function rise(sim, t, h, i) { if (sim.cvP[i] <= 0.5 && sim.cv[i] > 0.5) return crossT(sim, t, h, i, -1, 0.5) || t; return null; }
  function fall(sim, t, h, i) { if (sim.cvP[i] > 0.5 && sim.cv[i] <= 0.5) return crossT(sim, t, h, i, -1, 0.5) || t; return null; }
  function setOut(sim, o, v, e) { sim.cv[o] = v; sim.cev[o] = (v !== sim.cvP[o]) ? e : NaN; }

  class Blk {
    constructor(ctx, ins, outs, ft) { this.isC = true; this.sim = ctx.sim; this.ctx = ctx; this.ins = ins; this.outs = outs; this.ft = ft !== false; }
  }

  // Simple stateless function block y = f(u...)
  class Fn extends Blk {
    constructor(ctx, ins, out, f, ac) { super(ctx, ins, [out]); this.f = f; if (ac) this.acGain = ac; }
    eval(t, h, sim) { const u = this.ins.map(i => sim.cv[i]); sim.cv[this.outs[0]] = this.f.apply(null, u); sim.cev[this.outs[0]] = NaN; }
  }
  Fn.prototype.stampAC = function (C) {
    if (!this.acGain) throw new S.SimError(this.ctx.el.name + '：非線性元件，無法線性 AC 分析', this.ctx.el.id);
    const r = C.ci(this.outs[0]); C.a(r, r, 1);
    this.acGain.forEach((g, j) => C.a(r, C.ci(this.ins[j]), -g, 0));
  };
  // remove stampAC when no AC gain is defined (linear analysis then reports it as nonlinear)
  function fn(ctx, ins, out, f, ac) { const b = new Fn(ctx, ins, out, f, ac); if (!ac) b.stampAC = undefined; return b; }

  /* ------------------------------------------------------------------
   * LTI transfer function (observable canonical form, trapezoidal)
   * ------------------------------------------------------------------ */
  function matInv(A, n) {
    const M = A.map(r => r.slice()), I = A.map((r, i) => r.map((_, j) => i === j ? 1 : 0));
    for (let k = 0; k < n; k++) {
      let p = k; for (let i = k + 1; i < n; i++) if (Math.abs(M[i][k]) > Math.abs(M[p][k])) p = i;
      [M[k], M[p]] = [M[p], M[k]]; [I[k], I[p]] = [I[p], I[k]];
      const d = M[k][k]; if (d === 0) throw new Error('singular');
      for (let j = 0; j < n; j++) { M[k][j] /= d; I[k][j] /= d; }
      for (let i = 0; i < n; i++) if (i !== k) { const f = M[i][k]; if (f) for (let j = 0; j < n; j++) { M[i][j] -= f * M[k][j]; I[i][j] -= f * I[k][j]; } }
    }
    return I;
  }
  class LTI {
    // num, den: descending powers, same length n+1 (den[0] != 0)
    constructor(num, den, k, x0) {
      while (den.length > 1 && den[0] === 0) { den = den.slice(1); }
      if (num.length > den.length) { const extra = num.slice(0, num.length - den.length); if (extra.some(v => v !== 0)) throw new Error('分子階數不可大於分母（非真分式）'); num = num.slice(num.length - den.length); }
      while (num.length < den.length) num = [0].concat(num);
      const n = den.length - 1, an = den[0];
      this.n = n; this.num = num; this.den = den; this.k = k;
      const a = [], c = []; const bn = k * num[0] / an; this.D = bn;
      for (let i = 0; i < n; i++) { a.push(den[n - i] / an); c.push(k * num[n - i] / an - (den[n - i] / an) * bn); }
      const A = []; for (let i = 0; i < n; i++) { const r = new Array(n).fill(0); if (i > 0) r[i - 1] = 1; r[n - 1] = -a[i]; A.push(r); }
      this.A = A; this.B = c; this.cache = new Map();
      this.x0 = (x0 || []).slice();
    }
    disc(h) {
      let d = this.cache.get(h); if (d) return d;
      const n = this.n, A = this.A;
      const L = A.map((r, i) => r.map((v, j) => (i === j ? 1 : 0) - 0.5 * h * v));
      const R = A.map((r, i) => r.map((v, j) => (i === j ? 1 : 0) + 0.5 * h * v));
      const Li = matInv(L, n);
      const Ad = Li.map(r => R[0].map((_, j) => r.reduce((s, v, k) => s + v * R[k][j], 0)));
      const Bd = Li.map(r => r.reduce((s, v, k) => s + v * this.B[k], 0) * 0.5 * h);
      d = { Ad, Bd, ff: Bd[n - 1] + this.D };
      if (this.cache.size > 32) this.cache.clear();
      this.cache.set(h, d); return d;
    }
    // next state & output; x: Float64Array state (committed), uP previous input
    step(x, uP, u, h, xo) {
      const n = this.n;
      if (n === 0) return this.D * u;
      if (h === 0) { for (let i = 0; i < n; i++) xo[i] = x[i]; return x[n - 1] + this.D * u; }
      const d = this.disc(h);
      for (let i = 0; i < n; i++) { let s = d.Bd[i] * (u + uP); const r = d.Ad[i]; for (let j = 0; j < n; j++) s += r[j] * x[j]; xo[i] = s; }
      return xo[n - 1] + this.D * u;
    }
    H(w) { // complex gain at s = jw
      const ev = p => { let re = 0, im = 0; for (let i = 0; i < p.length; i++) { const nre = re * 0 - im * w, nim = re * w + im * 0; re = nre + p[i]; im = nim; } return [re, im]; };
      const N = ev(this.num), D = ev(this.den); const d = D[0] * D[0] + D[1] * D[1];
      return [this.k * (N[0] * D[0] + N[1] * D[1]) / d, this.k * (N[1] * D[0] - N[0] * D[1]) / d];
    }
  }
  S.LTI = LTI;

  class TFBlk extends Blk {
    constructor(ctx, i, o, lti) {
      super(ctx, [i], [o]); this.L = lti; this.n = lti.n;
      ctx.sim.reqC(this, 's', this.n + 1); this.xo = new Float64Array(this.n);
      this.ft = !(lti.D === 0 && this.n > 0) || true;
    }
    init() { for (let i = 0; i < this.n; i++) this.s[i] = this.L.x0[i] || 0; this.s[this.n] = 0; }
    eval(t, h, sim) { const u = sim.cv[this.ins[0]]; sim.cv[this.outs[0]] = this.L.step(this.s, this.s[this.n], u, h, this.xo); sim.cev[this.outs[0]] = NaN; }
    commit(t, h, sim) { for (let i = 0; i < this.n; i++) this.s[i] = this.xo[i]; this.s[this.n] = sim.cv[this.ins[0]]; }
    stampAC(C) { const r = C.ci(this.outs[0]); const [gr, gi] = this.L.H(C.w); C.a(r, r, 1); C.a(r, C.ci(this.ins[0]), -gr, -gi); }
  }
  function tf(ctx, num, den, k, x0) {
    let L; try { L = new LTI(num, den, k, x0); } catch (e) { throw ctx.err(e.message); }
    return new TFBlk(ctx, ctx.c(0), ctx.c(1), L);
  }

  /* ------------------------------------------------------------------
   * Continuous control blocks
   * ------------------------------------------------------------------ */
  M_.CONST = ctx => {
    const v = ctx.P.num('v'); const b = new Blk(ctx, [], [ctx.c(0)]);
    b.eval = (t, h, sim) => { sim.cv[b.outs[0]] = v; sim.cev[b.outs[0]] = NaN; };
    b.stampAC = C => { const r = C.ci(b.outs[0]); C.a(r, r, 1); };
    return b;
  };
  M_.P = ctx => { const k = ctx.P.num('k'); return fn(ctx, [ctx.c(0)], ctx.c(1), u => k * u, [k]); };

  class Integ extends Blk {
    constructor(ctx, i, o, T, y0, rst, flag) { super(ctx, rst >= 0 ? [i, rst] : [i], [o]); Object.assign(this, { T, y0, rst, flag }); ctx.sim.reqC(this, 's', 2); }
    init() { this.s[0] = this.y0; this.s[1] = 0; }
    eval(t, h, sim) {
      const u = sim.cv[this.ins[0]];
      let y = this.s[0] + (h / (2 * this.T)) * (u + this.s[1]);
      if (this.rst >= 0) {
        const r = this.rst;
        if (this.flag === 1) { if (hi(sim.cv[r])) y = 0; }
        else if (rise(sim, t, h, r) !== null) y = 0;
      }
      this.y = y; sim.cv[this.outs[0]] = y; sim.cev[this.outs[0]] = NaN;
    }
    commit(t, h, sim) { this.s[0] = this.y; this.s[1] = sim.cv[this.ins[0]]; }
    stampAC(C) { if (this.rst >= 0) throw new Error(); const r = C.ci(this.outs[0]); C.a(r, r, 1); C.a(r, C.ci(this.ins[0]), 0, 1 / (C.w * this.T)); }
  }
  M_.I = ctx => new Integ(ctx, ctx.c(0), ctx.c(1), ctx.P.pos('T'), ctx.P.num('y0', 0), -1, 0);
  M_.RESETI = ctx => { const b = new Integ(ctx, ctx.c(0), ctx.c(1), ctx.P.pos('T'), ctx.P.num('y0', 0), ctx.c(2), ctx.P.int('flag', 0)); b.stampAC = undefined; return b; };

  class Diff extends Blk {
    constructor(ctx, i, o, T) { super(ctx, [i], [o]); this.T = T; ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = NaN; }
    eval(t, h, sim) { const u = sim.cv[this.ins[0]]; const up = this.s[0]; sim.cv[this.outs[0]] = (h > 0 && up === up) ? this.T * (u - up) / h : 0; sim.cev[this.outs[0]] = NaN; }
    commit(t, h, sim) { this.s[0] = sim.cv[this.ins[0]]; }
    stampAC(C) { const r = C.ci(this.outs[0]); C.a(r, r, 1); C.a(r, C.ci(this.ins[0]), 0, -C.w * this.T); }
  }
  M_.DIFF = ctx => new Diff(ctx, ctx.c(0), ctx.c(1), ctx.P.pos('T'));

  M_.PI = ctx => { const k = ctx.P.num('k'), T = ctx.P.pos('T'); return tf(ctx, [k * T, k], [T, 0], 1); };
  M_.TFS = ctx => { const P = ctx.P; return tf(ctx, P.list('num'), P.list('den'), P.num('k', 1)); };
  M_.TFS1 = ctx => { const P = ctx.P; const x0 = P.list('x0', []).slice().reverse(); return tf(ctx, P.list('num'), P.list('den'), P.num('k', 1), x0); };
  M_.LP1 = ctx => { const k = ctx.P.num('k', 1), w = 2 * Math.PI * ctx.P.pos('fc'); return tf(ctx, [0, w], [1, w], k); };
  M_.HP1 = ctx => { const k = ctx.P.num('k', 1), w = 2 * Math.PI * ctx.P.pos('fc'); return tf(ctx, [1, 0], [1, w], k); };
  M_.LP2 = ctx => { const k = ctx.P.num('k', 1), z = ctx.P.num('z', 0.7071), w = 2 * Math.PI * ctx.P.pos('fc'); return tf(ctx, [0, 0, w * w], [1, 2 * z * w, w * w], k); };
  M_.HP2 = ctx => { const k = ctx.P.num('k', 1), z = ctx.P.num('z', 0.7071), w = 2 * Math.PI * ctx.P.pos('fc'); return tf(ctx, [1, 0, 0], [1, 2 * z * w, w * w], k); };
  M_.BP2 = ctx => { const k = ctx.P.num('k', 1), w = 2 * Math.PI * ctx.P.pos('fo'), B = 2 * Math.PI * ctx.P.pos('fb'); return tf(ctx, [0, B, 0], [1, B, w * w], k); };
  M_.BS2 = ctx => { const k = ctx.P.num('k', 1), w = 2 * Math.PI * ctx.P.pos('fo'), B = 2 * Math.PI * ctx.P.pos('fb'); return tf(ctx, [1, 0, w * w], [1, B, w * w], k); };

  // Summers
  M_.SUM2 = ctx => { const k1 = ctx.P.num('k1', 1), k2 = ctx.P.num('k2', 1); return fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), (a, b) => k1 * a + k2 * b, [k1, k2]); };
  M_.SUB2 = ctx => fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), (a, b) => a - b, [1, -1]);
  M_.SUM3 = ctx => { const k1 = ctx.P.num('k1', 1), k2 = ctx.P.num('k2', 1), k3 = ctx.P.num('k3', 1); return fn(ctx, [ctx.c(0), ctx.c(1), ctx.c(2)], ctx.c(3), (a, b, c) => k1 * a + k2 * b + k3 * c, [k1, k2, k3]); };

  // Comparator: pins [+ , -, out]
  class Comp extends Blk {
    constructor(ctx) { super(ctx, [ctx.c(0), ctx.c(1)], [ctx.c(2)]); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) {
      const a = sim.cv[this.ins[0]], b = sim.cv[this.ins[1]];
      const y = a > b ? 1 : (a < b ? 0 : this.s[0]);
      sim.cv[this.outs[0]] = y;
      sim.cev[this.outs[0]] = (y !== this.s[0]) ? crossT(sim, t, h, this.ins[0], this.ins[1]) : NaN;
    }
    commit(t, h, sim) { this.s[0] = sim.cv[this.outs[0]]; }
  }
  M_.COMP = ctx => new Comp(ctx);

  M_.LIM = ctx => { const lo = ctx.P.num('lo'), up = ctx.P.num('hi'); if (lo > up) throw ctx.err('下限不可大於上限'); return fn(ctx, [ctx.c(0)], ctx.c(1), u => u < lo ? lo : (u > up ? up : u)); };
  class DvdtLim extends Blk {
    constructor(ctx, L) { super(ctx, [ctx.c(0)], [ctx.c(1)]); this.L = Math.abs(L); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = NaN; }
    eval(t, h, sim) { const u = sim.cv[this.ins[0]]; const yp = this.s[0]; let y = u; if (yp === yp) { const d = this.L * h; y = Math.min(yp + d, Math.max(yp - d, u)); } this.y = y; sim.cv[this.outs[0]] = y; sim.cev[this.outs[0]] = NaN; }
    commit() { this.s[0] = this.y; }
  }
  M_.LIMDVDT = ctx => new DvdtLim(ctx, ctx.P.num('L'));

  // Computational blocks
  const safeDiv = (a, b) => b === 0 ? (a === 0 ? 0 : Math.sign(a) * 1e300) : a / b;
  M_.MULT = ctx => fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), (a, b) => a * b);
  M_.DIVD = ctx => fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), safeDiv);
  M_.SQRT = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.sqrt(Math.max(0, u)));
  M_.EXP = ctx => { const k1 = ctx.P.num('k1', 1), k2 = ctx.P.num('k2', Math.E); return fn(ctx, [ctx.c(0)], ctx.c(1), u => k1 * Math.pow(k2, u)); };
  M_.POW = ctx => { const k1 = ctx.P.num('k1', 1), k2 = ctx.P.num('k2', 2); return fn(ctx, [ctx.c(0)], ctx.c(1), u => k1 * Math.pow(u, k2)); };
  M_.LOG = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.log(u));
  M_.LOG10 = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.log10(u));
  M_.ABS = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), Math.abs);
  M_.SIGN = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => u > 0 ? 1 : (u < 0 ? -1 : 0));
  M_.SIN = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.sin(u * D2R));
  M_.SINR = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), Math.sin);
  M_.ASIN = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.asin(Math.max(-1, Math.min(1, u))) * R2D);
  M_.COS = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.cos(u * D2R));
  M_.COSR = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), Math.cos);
  M_.ACOS = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.acos(Math.max(-1, Math.min(1, u))) * R2D);
  M_.TAN = ctx => fn(ctx, [ctx.c(0)], ctx.c(1), u => Math.tan(u * D2R));
  // TG_1: pins [real(dot), imag, out] -> atan(imag/real) in deg
  M_.ATAN = ctx => fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), (re, im) => Math.atan(safeDiv(im, re)) * R2D);
  // ATAN2: pins [y, x, out] -> deg in (-180,180]
  M_.ATAN2 = ctx => fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), (y, x) => Math.atan2(y, x) * R2D);
  M_.MAXMIN = ctx => { const f = ctx.P.int('flag', 0); return fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), f ? Math.min : Math.max); };

  // Sample instants at k*T + 0: helper for sampled / period blocks
  function nextSample(t, fs) { let k = Math.floor(t * fs) + 1; let ts = k / fs; while (ts <= t) { k++; ts = k / fs; } return ts; }
  function isSample(t, fs, eps) { const k = Math.round(t * fs); return Math.abs(t - k / fs) <= eps; }

  // RMS over each period 1/fb; output updated at the start of each period
  class RMS extends Blk {
    constructor(ctx, fb) { super(ctx, [ctx.c(0)], [ctx.c(1)]); this.fb = fb; ctx.sim.reqC(this, 's', 3); }
    init() { this.s.fill(0); }
    eval(t, h, sim) {
      const u = sim.cv[this.ins[0]]; let I = this.s[0] + 0.5 * h * (u * u + this.s[1] * this.s[1]); let y = this.s[2];
      if (t > 0 && isSample(t, this.fb, sim.epsT)) { y = Math.sqrt(Math.max(0, I * this.fb)); I = 0; }
      this.p = [I, u, y]; setOut(sim, this.outs[0], y, t);
    }
    commit() { this.s.set(this.p); }
    nextBreak(t) { return nextSample(t, this.fb); }
  }
  M_.RMS = ctx => new RMS(ctx, ctx.P.pos('fb'));

  // FFT fundamental: pins [in, amp, phase]
  class FFTB extends Blk {
    constructor(ctx, N, fb) {
      super(ctx, [ctx.c(0)], [ctx.c(1), ctx.c(2)]); this.N = N; this.fb = fb; this.fs = N * fb;
      ctx.sim.reqC(this, 's', 2 * N + 3);
    }
    init() { this.s.fill(0); }
    eval(t, h, sim) {
      const N = this.N, s = this.s;
      let amp = s[2 * N], ph = s[2 * N + 1];
      this.p = null;
      if (isSample(t, this.fs, sim.epsT)) {
        const k = Math.round(t * this.fs); const u = sim.cv[this.ins[0]];
        const buf = s.slice(0, N); buf[k % N] = u;
        let re = 0, im = 0;
        for (let m = 0; m < N; m++) { const th = 2 * Math.PI * m / N; re += buf[m] * Math.cos(th); im -= buf[m] * Math.sin(th); }
        re *= 2 / N; im *= 2 / N;
        amp = Math.hypot(re, im); ph = Math.atan2(im, re) * R2D + 90; if (ph > 180) ph -= 360;
        this.p = { k, u, amp, ph };
      }
      setOut(sim, this.outs[0], amp, t); setOut(sim, this.outs[1], ph, t);
    }
    commit() { if (this.p) { this.s[this.p.k % this.N] = this.p.u; this.s[2 * this.N] = this.p.amp; this.s[2 * this.N + 1] = this.p.ph; } }
    nextBreak(t) { return nextSample(t, this.fs); }
  }
  M_.FFT = ctx => { const N = ctx.P.int('N', 64), fb = ctx.P.pos('fb'); if (N < 4 || N > 1024 || (N & (N - 1))) throw ctx.err('取樣點數必須是 4~1024 之間的 2 的次方'); return new FFTB(ctx, N, fb); };

  /* ------------------------------------------------------------------
   * Other function blocks
   * ------------------------------------------------------------------ */
  // Sample-and-hold: pins [in, ctrl, out]
  class SampH extends Blk {
    constructor(ctx) { super(ctx, [ctx.c(0), ctx.c(1)], [ctx.c(2)]); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) {
      let y = this.s[0]; const te = rise(sim, t, h, this.ins[1]);
      if (te !== null) { const ua = sim.cvP[this.ins[0]], ub = sim.cv[this.ins[0]]; const f = h > 0 ? (te - (t - h)) / h : 1; y = ua + (ub - ua) * Math.max(0, Math.min(1, f)); }
      if (t === 0 && h === 0 && hi(sim.cv[this.ins[1]])) y = sim.cv[this.ins[0]];
      this.y = y; setOut(sim, this.outs[0], y, te !== null ? te : NaN);
    }
    commit() { this.s[0] = this.y; }
  }
  M_.SAMP = ctx => new SampH(ctx);

  // 1-D lookup table, data "x1 y1 x2 y2 ..."
  M_.LKUP = ctx => {
    const L = ctx.P.list('tab'); if (L.length < 4 || L.length % 2) throw ctx.err('請以 x1 y1 x2 y2 ... 的成對格式輸入表格');
    const xs = [], ys = []; for (let i = 0; i < L.length; i += 2) { xs.push(L[i]); ys.push(L[i + 1]); }
    for (let i = 1; i < xs.length; i++) if (!(xs[i] > xs[i - 1])) throw ctx.err('輸入值必須嚴格遞增');
    return fn(ctx, [ctx.c(0)], ctx.c(1), u => { if (u <= xs[0]) return ys[0]; const n = xs.length; if (u >= xs[n - 1]) return ys[n - 1]; let j = 1; while (u > xs[j]) j++; return ys[j - 1] + (ys[j] - ys[j - 1]) * (u - xs[j - 1]) / (xs[j] - xs[j - 1]); });
  };
  M_.LKUPTZ = ctx => {
    const th = ctx.P.num('th'), vp = ctx.P.num('vpk');
    return fn(ctx, [ctx.c(0)], ctx.c(1), u => {
      let a = ((u % 360) + 360) % 360; let sg = 1; if (a >= 180) { a -= 180; sg = -1; }
      if (a > 90) a = 180 - a;
      return sg * (a < th ? vp * a / th : vp);
    });
  };
  M_.LKUPSQ = ctx => {
    const th = ctx.P.num('th');
    return fn(ctx, [ctx.c(0)], ctx.c(1), u => {
      let a = ((u % 360) + 360) % 360; let sg = 1; if (a >= 180) { a -= 180; sg = -1; }
      return Math.abs(a - 90) < th / 2 ? sg : 0;
    });
  };
  M_.ROUND = ctx => {
    const N = ctx.P.int('N', 0), tr = ctx.P.int('flag', 0); const sc = Math.pow(10, N);
    return fn(ctx, [ctx.c(0)], ctx.c(1), u => (tr ? Math.trunc(u * sc) : Math.round(u * sc)) / sc);
  };

  // Continuous time delay with exact reproduction of jumps
  class TDelay extends Blk {
    constructor(ctx, Td) { super(ctx, [ctx.c(0)], [ctx.c(1)], Td < ctx.sim.dt * 0.999); this.Td = Td; this.buf = null; }
    init() { this.buf = { t: [], v: [] }; }
    at(t, uNow, tNow, uP, tP) {
      const b = this.buf; const n = b.t.length;
      if (t <= (n ? b.t[0] : tNow)) return n ? b.v[0] : uNow;
      if (t >= tP && tNow > tP) return uP + (uNow - uP) * (t - tP) / (tNow - tP);
      // binary search in history
      let lo = 0, hi2 = n - 1;
      if (t >= b.t[hi2]) return b.v[hi2];
      while (hi2 - lo > 1) { const m = (lo + hi2) >> 1; if (b.t[m] <= t) lo = m; else hi2 = m; }
      const t0 = b.t[lo], t1 = b.t[hi2];
      if (t1 === t0) return b.v[hi2];
      return b.v[lo] + (b.v[hi2] - b.v[lo]) * (t - t0) / (t1 - t0);
    }
    eval(t, h, sim) {
      const u = sim.cv[this.ins[0]];
      const tP = t - h, uP = sim.cvP[this.ins[0]];
      const y = this.at(t - this.Td, u, t, uP, tP);
      // if the input jumped inside this interval, remember the exact jump
      const e = sim.cev[this.ins[0]];
      this.pend = { t, u, e: (e === e && u !== uP) ? e : NaN, uP };
      setOut(sim, this.outs[0], y, t);
    }
    commit(t, h, sim) {
      const b = this.buf, p = this.pend; if (!p) return;
      if (!b.t.length) { b.t.push(t); b.v.push(p.u); return; }
      if (p.e === p.e) { b.t.push(p.e); b.v.push(p.uP); b.t.push(p.e); b.v.push(p.u); }
      if (b.t[b.t.length - 1] !== t || b.v[b.v.length - 1] !== p.u) { b.t.push(t); b.v.push(p.u); }
      const keep = t - this.Td - 2 * sim.dt; let c = 0; while (c < b.t.length - 2 && b.t[c + 1] < keep) c++;
      if (c > 4096) { b.t.splice(0, c); b.v.splice(0, c); }
    }
    nextBreak(t) {
      const b = this.buf; for (let i = 1; i < b.t.length; i++) if (b.t[i] === b.t[i - 1]) { const te = b.t[i] + this.Td; if (te > t) return te; }
      return Infinity;
    }
    saveX() { return { t: this.buf.t.slice(), v: this.buf.v.slice() }; }
    restoreX(x) { this.buf = { t: x.t.slice(), v: x.v.slice() }; }
    stampAC(C) { const r = C.ci(this.outs[0]); C.a(r, r, 1); const ph = -C.w * this.Td; C.a(r, C.ci(this.ins[0]), -Math.cos(ph), -Math.sin(ph)); }
  }
  M_.TDELAY = ctx => { const Td = ctx.P.num('Td'); if (!(Td >= 0)) throw ctx.err('延遲時間必須 ≥ 0'); return new TDelay(ctx, Td); };

  // Unit time delay: output = input at the previous simulation step
  class UTD extends Blk {
    constructor(ctx) { super(ctx, [ctx.c(0)], [ctx.c(1)], false); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) { sim.cv[this.outs[0]] = this.s[0]; sim.cev[this.outs[0]] = NaN; }
    commit(t, h, sim) { this.s[0] = sim.cv[this.ins[0]]; }
  }
  M_.UTD = ctx => new UTD(ctx);

  // Moving-window RMS helper
  class MovRMS {
    constructor(T) { this.T = T; this.t = [0]; this.I = [0]; }
    val(t, I) {
      const t0 = t - this.T; if (t0 <= 0) return Math.sqrt(Math.max(0, I / Math.max(t, 1e-300)));
      const b = this; let j = b.t.length - 1; while (j > 0 && b.t[j] > t0) j--;
      const ta = b.t[j], tb = j + 1 < b.t.length ? b.t[j + 1] : t, Ia = b.I[j], Ib = j + 1 < b.I.length ? b.I[j + 1] : I;
      const I0 = Ia + (Ib - Ia) * (tb > ta ? (t0 - ta) / (tb - ta) : 0);
      return Math.sqrt(Math.max(0, (I - I0) / this.T));
    }
    push(t, I) { this.t.push(t); this.I.push(I); const keep = t - 1.5 * this.T; let c = 0; while (c < this.t.length - 2 && this.t[c + 1] < keep) c++; if (c > 2048) { this.t.splice(0, c); this.I.splice(0, c); } }
    save() { return { t: this.t.slice(), I: this.I.slice() }; }
    load(x) { this.t = x.t.slice(); this.I = x.I.slice(); }
  }
  // THD block: pins [in, thd, v1]
  class THD extends Blk {
    constructor(ctx, f, fb) {
      super(ctx, [ctx.c(0)], [ctx.c(1), ctx.c(2)]);
      const w = 2 * Math.PI * f, B = 2 * Math.PI * fb; this.L = new LTI([0, B, 0], [1, B, w * w], 1); this.T = 1 / f;
      ctx.sim.reqC(this, 's', 6); this.xo = new Float64Array(2);
    }
    init() { this.s.fill(0); this.r1 = new MovRMS(this.T); this.r2 = new MovRMS(this.T); }
    eval(t, h, sim) {
      const u = sim.cv[this.ins[0]], s = this.s;
      const v1 = this.L.step(s, s[2], u, h, this.xo);
      const Iu = s[3] + 0.5 * h * (u * u + s[2] * s[2]), I1 = s[4] + 0.5 * h * (v1 * v1 + s[5] * s[5]);
      const Vr = this.r1.val(t, Iu), V1 = this.r2.val(t, I1);
      const thd = V1 > 0 ? Math.min(10, Math.sqrt(Math.max(0, Vr * Vr - V1 * V1)) / V1) : 0; // capped at 1000 % (start-up, before the fundamental has settled)
      this.p = { u, v1, Iu, I1 };
      sim.cv[this.outs[0]] = thd; sim.cv[this.outs[1]] = v1; sim.cev[this.outs[0]] = sim.cev[this.outs[1]] = NaN;
    }
    commit(t, h) { const p = this.p, s = this.s; s[0] = this.xo[0]; s[1] = this.xo[1]; s[2] = p.u; s[3] = p.Iu; s[4] = p.I1; s[5] = p.v1; if (h > 0) { this.r1.push(t, p.Iu); this.r2.push(t, p.I1); } }
    saveX() { return [this.r1.save(), this.r2.save()]; }
    restoreX(x) { this.r1.load(x[0]); this.r2.load(x[1]); }
  }
  M_.THD = ctx => new THD(ctx, ctx.P.pos('f'), ctx.P.pos('fb'));

  // Multiplexers: pins [d0..dn-1, s0..sk-1, Y]
  function muxMk(n, k) {
    return ctx => {
      const d = [], s = []; for (let i = 0; i < n; i++) d.push(ctx.c(i)); for (let i = 0; i < k; i++) s.push(ctx.c(n + i));
      const b = new Blk(ctx, d.concat(s), [ctx.c(n + k)]);
      b.eval = (t, h, sim) => { let idx = 0; for (let i = 0; i < k; i++) if (hi(sim.cv[s[i]])) idx |= (1 << i); const o = b.outs[0]; const v = sim.cv[d[idx]]; sim.cv[o] = v; sim.cev[o] = v !== sim.cvP[o] ? inEvt(sim, b.ins) : NaN; };
      return b;
    };
  }
  M_.MUX2 = muxMk(2, 1); M_.MUX4 = muxMk(4, 2); M_.MUX8 = muxMk(8, 3);

  // abc <-> dqo : pins [a,b,c,theta, d,q,o] / [d,q,o,theta, a,b,c] (theta in rad)
  M_.ABC2DQO = ctx => {
    const ins = [0, 1, 2, 3].map(i => ctx.c(i)), outs = [ctx.c(4), ctx.c(5), ctx.cOpt(6)]; // the zero-sequence output may be left open
    const b = new Blk(ctx, ins, outs);
    b.eval = (t, h, sim) => { const [va, vb, vc, th] = ins.map(i => sim.cv[i]); const k = 2 * Math.PI / 3;
      sim.cv[outs[0]] = 2 / 3 * (va * Math.cos(th) + vb * Math.cos(th - k) + vc * Math.cos(th + k));
      sim.cv[outs[1]] = 2 / 3 * (va * Math.sin(th) + vb * Math.sin(th - k) + vc * Math.sin(th + k));
      sim.cv[outs[2]] = (va + vb + vc) / 3; outs.forEach(o => { sim.cev[o] = NaN; }); };
    return b;
  };
  M_.DQO2ABC = ctx => {
    const ins = [ctx.c(0), ctx.c(1), ctx.cOpt(2), ctx.c(3)], outs = [4, 5, 6].map(i => ctx.c(i)); // the zero-sequence input may be left open (= 0)
    const b = new Blk(ctx, ins, outs);
    b.eval = (t, h, sim) => { const vd = sim.cv[ins[0]], vq = sim.cv[ins[1]], vo = ins[2] > 0 ? sim.cv[ins[2]] : 0, th = sim.cv[ins[3]]; const k = 2 * Math.PI / 3;
      sim.cv[outs[0]] = vd * Math.cos(th) + vq * Math.sin(th) + vo;
      sim.cv[outs[1]] = vd * Math.cos(th - k) + vq * Math.sin(th - k) + vo;
      sim.cv[outs[2]] = vd * Math.cos(th + k) + vq * Math.sin(th + k) + vo; outs.forEach(o => { sim.cev[o] = NaN; }); };
    return b;
  };

  /* ------------------------------------------------------------------
   * Logic
   * ------------------------------------------------------------------ */
  function gateMk(nIn, f) {
    return ctx => {
      const ins = []; for (let i = 0; i < nIn; i++) ins.push(ctx.c(i));
      const b = new Blk(ctx, ins, [ctx.c(nIn)]);
      b.eval = (t, h, sim) => { const v = f(ins.map(i => hi(sim.cv[i]))) ? 1 : 0; setOut(sim, b.outs[0], v, inEvt(sim, ins)); };
      return b;
    };
  }
  M_.AND = gateMk(2, a => a[0] && a[1]); M_.AND3 = gateMk(3, a => a[0] && a[1] && a[2]);
  M_.OR = gateMk(2, a => a[0] || a[1]); M_.OR3 = gateMk(3, a => a[0] || a[1] || a[2]);
  M_.XOR = gateMk(2, a => a[0] !== a[1]); M_.NOT = gateMk(1, a => !a[0]);
  M_.NAND = gateMk(2, a => !(a[0] && a[1])); M_.NOR = gateMk(2, a => !(a[0] || a[1]));

  // flip-flop helper: outputs [Q, Qn]
  class FF extends Blk {
    constructor(ctx, ins, outs, logic) { super(ctx, ins, outs); this.logic = logic; ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) {
      const r = this.logic(sim, t, h, this.s[0]); const q = r[0], e = r[1];
      this.q = q; setOut(sim, this.outs[0], q, e); if (this.outs[1] > 0) setOut(sim, this.outs[1], 1 - q, e);
    }
    commit() { this.s[0] = this.q; }
  }
  // SRFF pins [S, R, Q, Qn]
  M_.SRFF = ctx => {
    const S_ = ctx.c(0), R = ctx.c(1), lvl = ctx.P.int('flag', 0) === 1;
    return new FF(ctx, [S_, R], [ctx.c(2), ctx.cOpt(3)], (sim, t, h, q) => {
      if (lvl) { const s = hi(sim.cv[S_]), r = hi(sim.cv[R]); if (s && !r) return [1, inEvt(sim, [S_])]; if (r && !s) return [0, inEvt(sim, [R])]; return [q, NaN]; }
      const es = rise(sim, t, h, S_), er = rise(sim, t, h, R);
      if (es !== null && er === null) return [1, es]; if (er !== null && es === null) return [0, er]; return [q, NaN];
    });
  };
  // JKFF pins [J, CLK, K, Q, Qn]
  M_.JKFF = ctx => {
    const J = ctx.c(0), CK = ctx.c(1), K = ctx.c(2);
    return new FF(ctx, [J, CK, K], [ctx.c(3), ctx.cOpt(4)], (sim, t, h, q) => {
      const e = rise(sim, t, h, CK); if (e === null) return [q, NaN];
      const j = hi(sim.cvP[J]), k = hi(sim.cvP[K]);
      if (j && k) return [1 - q, e]; if (j) return [1, e]; if (k) return [0, e]; return [q, NaN];
    });
  };
  // JKFF with set/reset pins [S, J, CLK, K, R, Q, Qn]
  M_.JKFFSR = ctx => {
    const S_ = ctx.c(0), J = ctx.c(1), CK = ctx.c(2), K = ctx.c(3), R = ctx.c(4);
    return new FF(ctx, [S_, J, CK, K, R], [ctx.c(5), ctx.cOpt(6)], (sim, t, h, q) => {
      if (hi(sim.cv[S_])) return [1, inEvt(sim, [S_])]; if (hi(sim.cv[R])) return [0, inEvt(sim, [R])];
      const e = rise(sim, t, h, CK); if (e === null) return [q, NaN];
      const j = hi(sim.cvP[J]), k = hi(sim.cvP[K]);
      if (j && k) return [1 - q, e]; if (j) return [1, e]; if (k) return [0, e]; return [q, NaN];
    });
  };
  // DFF pins [D, CLK, Q, Qn]
  M_.DFF = ctx => {
    const Dn = ctx.c(0), CK = ctx.c(1);
    return new FF(ctx, [Dn, CK], [ctx.c(2), ctx.cOpt(3)], (sim, t, h, q) => { const e = rise(sim, t, h, CK); if (e === null) return [q, NaN]; return [hi(sim.cvP[Dn]) ? 1 : 0, e]; });
  };
  // DFF with set/reset pins [S, D, CLK, R, Q, Qn]
  M_.DFFSR = ctx => {
    const S_ = ctx.c(0), Dn = ctx.c(1), CK = ctx.c(2), R = ctx.c(3);
    return new FF(ctx, [S_, Dn, CK, R], [ctx.c(4), ctx.cOpt(5)], (sim, t, h, q) => {
      if (hi(sim.cv[S_])) return [1, inEvt(sim, [S_])]; if (hi(sim.cv[R])) return [0, inEvt(sim, [R])];
      const e = rise(sim, t, h, CK); if (e === null) return [q, NaN]; return [hi(sim.cvP[Dn]) ? 1 : 0, e];
    });
  };

  // Monostable: pins [in, Q, Qn] (+ width input for MONOC: [in, W, Q, Qn])
  class Mono extends Blk {
    constructor(ctx, inp, wIn, outs, W) { super(ctx, wIn >= 0 ? [inp, wIn] : [inp], outs); this.inp = inp; this.wIn = wIn; this.W = W; ctx.sim.reqC(this, 's', 2); }
    init() { this.s[0] = -Infinity; this.s[1] = -Infinity; }
    eval(t, h, sim) {
      let ts = this.s[0], te = this.s[1];
      const e = rise(sim, t, h, this.inp);
      if (e !== null && !(t < te - sim.epsT && t - h < te)) { ts = e; const W = this.wIn >= 0 ? sim.cv[this.wIn] : this.W; te = e + Math.max(0, W); }
      this.p = [ts, te];
      const on = (t >= ts - sim.epsT && t < te - sim.epsT) ? 1 : 0;
      const ev = on ? ts : te;
      setOut(sim, this.outs[0], on, ev); if (this.outs[1] > 0) setOut(sim, this.outs[1], 1 - on, ev);
    }
    commit() { this.s[0] = this.p[0]; this.s[1] = this.p[1]; }
    nextBreak(t) { return this.s[1] > t ? this.s[1] : Infinity; }
  }
  M_.MONO = ctx => new Mono(ctx, ctx.c(0), -1, [ctx.c(1), ctx.cOpt(2)], ctx.P.num('W'));
  M_.MONOC = ctx => new Mono(ctx, ctx.c(0), ctx.c(1), [ctx.c(2), ctx.cOpt(3)], 0);

  // Pulse width counter: pins [in, out]
  class PWCT extends Blk {
    constructor(ctx) { super(ctx, [ctx.c(0)], [ctx.c(1)]); ctx.sim.reqC(this, 's', 2); }
    init() { this.s[0] = NaN; this.s[1] = 0; }
    eval(t, h, sim) {
      let ts = this.s[0], y = this.s[1]; const er = rise(sim, t, h, this.ins[0]), ef = fall(sim, t, h, this.ins[0]);
      let ev = NaN;
      if (er !== null) ts = er;
      if (ef !== null && ts === ts) { y = ef - ts; ev = ef; }
      this.p = [ts, y]; setOut(sim, this.outs[0], y, ev);
    }
    commit() { this.s[0] = this.p[0]; this.s[1] = this.p[1]; }
  }
  M_.PWCT = ctx => new PWCT(ctx);

  // Up/down counter: pins [U, D, R, out]
  class UpDn extends Blk {
    constructor(ctx, bits) { super(ctx, [ctx.c(0), ctx.c(1), ctx.cOpt(2)].filter(v => v >= 0), [ctx.c(3)]); this.U = ctx.c(0); this.D = ctx.c(1); this.R = ctx.cOpt(2); this.mod = Math.pow(2, bits); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) {
      let c = this.s[0]; let ev = NaN;
      const eu = rise(sim, t, h, this.U), ed = rise(sim, t, h, this.D);
      if (eu !== null) { c++; ev = eu; } if (ed !== null) { c--; ev = ed; }
      c = ((c % this.mod) + this.mod) % this.mod;
      if (this.R >= 0 && hi(sim.cv[this.R])) { c = 0; ev = inEvt(sim, [this.R]); }
      this.c = c; setOut(sim, this.outs[0], c, ev);
    }
    commit() { this.s[0] = this.c; }
  }
  M_.UPDN = ctx => new UpDn(ctx, ctx.P.int('bits', 8));

  // A/D: pins [Vin, Vref, CLK, out] ; D/A: pins [Vin, Vref, out]
  class ADC extends Blk {
    constructor(ctx, N) { super(ctx, [ctx.c(0), ctx.c(1), ctx.c(2)], [ctx.c(3)]); this.N = N; ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) {
      let y = this.s[0]; const e = rise(sim, t, h, this.ins[2]);
      if (e !== null) { const vr = sim.cv[this.ins[1]]; const M = Math.pow(2, this.N); y = Math.max(0, Math.min(M - 1, Math.floor(M / vr * sim.cv[this.ins[0]]))); }
      this.y = y; setOut(sim, this.outs[0], y, e !== null ? e : NaN);
    }
    commit() { this.s[0] = this.y; }
  }
  M_.ADC = ctx => new ADC(ctx, ctx.P.int('bits', 8));
  M_.DAC = ctx => { const N = ctx.P.int('bits', 8); return fn(ctx, [ctx.c(0), ctx.c(1)], ctx.c(2), (v, vr) => vr / Math.pow(2, N) * v); };

  /* ------------------------------------------------------------------
   * Digital control module (sampled blocks)
   * ------------------------------------------------------------------ */
  class Samp extends Blk {
    constructor(ctx, ins, outs, fs, ft) { super(ctx, ins, outs, ft); this.fs = fs; }
    hit(t, sim) { return isSample(t, this.fs, sim.epsT); }
    nextBreak(t) { return nextSample(t, this.fs); }
  }
  // ZOH
  class ZOH extends Samp {
    constructor(ctx, fs) { super(ctx, [ctx.c(0)], [ctx.c(1)], fs); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) { let y = this.s[0]; if (this.hit(t, sim)) y = sim.cv[this.ins[0]]; this.y = y; setOut(sim, this.outs[0], y, t); }
    commit() { this.s[0] = this.y; }
  }
  M_.ZOH = ctx => new ZOH(ctx, ctx.P.pos('fs'));
  // Unit delay (one sampling period)
  class UDelay extends Samp {
    constructor(ctx, fs) { super(ctx, [ctx.c(0)], [ctx.c(1)], fs, false); ctx.sim.reqC(this, 's', 2); }
    init() { this.s[0] = 0; this.s[1] = 0; }
    eval(t, h, sim) { let y = this.s[0]; this.isHit = this.hit(t, sim); if (this.isHit) y = this.s[1]; this.y = y; setOut(sim, this.outs[0], y, t); }
    commit(t, h, sim) { this.s[0] = this.y; if (this.isHit) this.s[1] = sim.cv[this.ins[0]]; }
  }
  M_.UDELAY = ctx => new UDelay(ctx, ctx.P.pos('fs'));

  // Discrete integrator (flag 0 trapezoidal, 1 backward Euler, 2 forward Euler), optional reset
  class IntD extends Samp {
    constructor(ctx, fs, alg, y0, rst, rflag, wrap) {
      super(ctx, rst >= 0 ? [ctx.c(0), rst] : [ctx.c(0)], [ctx.c(rst >= 0 ? 2 : 1)], fs);
      Object.assign(this, { alg, y0, rst, rflag, wrap }); ctx.sim.reqC(this, 's', 3); // y, uPrev, started
    }
    init() { this.s[0] = this.y0; this.s[1] = 0; this.s[2] = 0; }
    eval(t, h, sim) {
      let y = this.s[0]; this.isHit = this.hit(t, sim);
      if (this.isHit) {
        const u = sim.cv[this.ins[0]], T = 1 / this.fs;
        if (this.s[2]) {
          if (this.alg === 0) y = y + 0.5 * T * (u + this.s[1]);
          else if (this.alg === 1) y = y + T * u;
          else y = y + T * this.s[1];
        }
        if (this.rst >= 0) { const r = this.ins[1]; if (this.rflag === 1 ? hi(sim.cv[r]) : (hi(sim.cv[r]) && !hi(this.rPrev || 0))) y = 0; }
        if (this.wrap) { const [lo, up] = this.wrap, span = up - lo; if (span > 0) { while (y > up) y -= span; while (y < lo) y += span; } }
      }
      this.y = y; setOut(sim, this.outs[0], y, t);
    }
    commit(t, h, sim) { this.s[0] = this.y; if (this.isHit) { this.s[1] = sim.cv[this.ins[0]]; this.s[2] = 1; } if (this.rst >= 0 && this.isHit) this.rPrev = sim.cv[this.ins[1]]; }
  }
  M_.ID = ctx => new IntD(ctx, ctx.P.pos('fs'), ctx.P.int('alg', 0), ctx.P.num('y0', 0), -1, 0, null);
  M_.IRSTD = ctx => new IntD(ctx, ctx.P.pos('fs'), ctx.P.int('alg', 0), ctx.P.num('y0', 0), ctx.c(1), ctx.P.int('flag', 0), null);
  M_.IRSTI = ctx => { const lo = ctx.P.num('lo'), up = ctx.P.num('hi'); if (!(up > lo)) throw ctx.err('上限必須大於下限'); return new IntD(ctx, ctx.P.pos('fs'), ctx.P.int('alg', 0), ctx.P.num('y0', 0), -1, 0, [lo, up]); };
  // Discrete differentiator
  class DD extends Samp {
    constructor(ctx, fs) { super(ctx, [ctx.c(0)], [ctx.c(1)], fs); ctx.sim.reqC(this, 's', 3); }
    init() { this.s.fill(0); }
    eval(t, h, sim) { let y = this.s[0]; this.isHit = this.hit(t, sim); if (this.isHit) y = this.s[2] ? (sim.cv[this.ins[0]] - this.s[1]) * this.fs : 0; this.y = y; setOut(sim, this.outs[0], y, t); }
    commit(t, h, sim) { this.s[0] = this.y; if (this.isHit) { this.s[1] = sim.cv[this.ins[0]]; this.s[2] = 1; } }
  }
  M_.DD = ctx => new DD(ctx, ctx.P.pos('fs'));
  // General difference equation: y(n) = (sum b_i u(n-i) - sum_{i>=1} a_i y(n-i)) / a0
  class DiffEq extends Samp {
    constructor(ctx, b, a, fs) {
      super(ctx, [ctx.c(0)], [ctx.c(1)], fs);
      const N = Math.max(b.length, a.length) - 1; while (b.length < N + 1) b.push(0); while (a.length < N + 1) a.push(0);
      this.b = b; this.a = a; this.N = N; ctx.sim.reqC(this, 's', 2 * N + 1); // u(n-1..n-N), y(n-1..n-N), out
    }
    init() { this.s.fill(0); }
    eval(t, h, sim) {
      let y = this.s[2 * this.N]; this.isHit = this.hit(t, sim);
      if (this.isHit) { const N = this.N, s = this.s, u = sim.cv[this.ins[0]]; let acc = this.b[0] * u; for (let i = 1; i <= N; i++) acc += this.b[i] * s[i - 1] - this.a[i] * s[N + i - 1]; y = acc / this.a[0]; this.u = u; }
      this.y = y; setOut(sim, this.outs[0], y, t);
    }
    commit() {
      const N = this.N, s = this.s;
      if (this.isHit) { for (let i = N - 1; i > 0; i--) { s[i] = s[i - 1]; s[N + i] = s[N + i - 1]; } if (N > 0) { s[0] = this.u; s[N] = this.y; } }
      s[2 * N] = this.y;
    }
  }
  function deMk(useA) {
    return ctx => {
      const P = ctx.P; const b = P.list('b'); const a = useA ? P.list('a') : [1];
      if (!b.length) throw ctx.err('至少需要一個分子係數');
      if (useA && !a[0]) throw ctx.err('a0 不可為 0');
      return new DiffEq(ctx, b.slice(), a.slice(), P.pos('fs'));
    };
  }
  M_.TFZ = deMk(true); M_.FILTD = deMk(true); M_.FIR = deMk(false);

  // Quantization: pins [in, out]
  class Quant extends Samp {
    constructor(ctx, N, a, b, c, d, fs, rnd) { super(ctx, [ctx.c(0)], [ctx.c(1)], fs); Object.assign(this, { N, a, b, c, d, rnd }); ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = 0; }
    eval(t, h, sim) {
      let y = this.s[0];
      if (this.hit(t, sim)) {
        const vin = sim.cv[this.ins[0]]; const vox = this.c + (vin - this.a) * (this.d - this.c) / (this.b - this.a);
        const dV = (this.d - this.c) / (Math.pow(2, this.N) - 1);
        const q = (vox - this.c) / dV; const n = this.rnd ? Math.round(q) : Math.floor(q + 1e-12);
        y = Math.max(this.c, Math.min(this.d, this.c + n * dV));
      }
      this.y = y; setOut(sim, this.outs[0], y, t);
    }
    commit() { this.s[0] = this.y; }
  }
  function qMk(rnd) { return ctx => { const P = ctx.P; const a = P.num('vimin'), b = P.num('vimax'); if (!(b > a)) throw ctx.err('Vin_max 必須大於 Vin_min'); return new Quant(ctx, P.int('N'), a, b, P.num('vomin'), P.num('vomax'), P.pos('fs'), rnd); }; }
  M_.DIGIT = qMk(false); M_.DIGIT2 = qMk(true);

  // Vector blocks ------------------------------------------------------
  class CBuf extends Samp {
    constructor(ctx, L, fs) { super(ctx, [ctx.c(0)], [ctx.c(1)], fs); this.L = L; ctx.sim.reqC(this, 's', L + 1); }
    init(sim) { this.s.fill(0); sim.cvec[this.outs[0]] = this.s.subarray(0, this.L); }
    eval(t, h, sim) {
      this.isHit = this.hit(t, sim);
      let v = this.s.slice(0, this.L);
      if (this.isHit) v[this.s[this.L] % this.L] = sim.cv[this.ins[0]];
      this.v = v; sim.cvec[this.outs[0]] = v; setOut(sim, this.outs[0], v[0], t);
    }
    commit() { if (this.isHit) { this.s.set(this.v, 0); this.s[this.L] = (this.s[this.L] + 1) % this.L; } }
  }
  M_.CBUF = ctx => new CBuf(ctx, Math.max(1, ctx.P.int('L')), ctx.P.pos('fs'));
  M_.ARRAY = ctx => {
    const vals = ctx.P.list('vals'); const L = ctx.P.int('L', vals.length);
    const arr = new Float64Array(L); for (let i = 0; i < L; i++) arr[i] = vals[i] || 0;
    const b = new Blk(ctx, [], [ctx.c(0)]);
    b.eval = (t, h, sim) => { sim.cvec[b.outs[0]] = arr; sim.cv[b.outs[0]] = arr[0]; sim.cev[b.outs[0]] = NaN; };
    return b;
  };
  M_.CONV = ctx => {
    const b = new Blk(ctx, [ctx.c(0), ctx.c(1)], [ctx.c(2)]);
    b.eval = (t, h, sim) => {
      const A = sim.cvec[b.ins[0]] || [sim.cv[b.ins[0]]], B = sim.cvec[b.ins[1]] || [sim.cv[b.ins[1]]];
      const m = A.length, n = B.length, C = new Float64Array(m + n - 1);
      for (let i = 0; i < m; i++) for (let j = 0; j < n; j++) C[i + j] += A[i] * B[j];
      sim.cvec[b.outs[0]] = C; sim.cv[b.outs[0]] = C[0]; sim.cev[b.outs[0]] = NaN;
    };
    return b;
  };
  M_.MEMREAD = ctx => {
    const off = ctx.P.int('off', 0);
    const b = new Blk(ctx, [ctx.c(0)], [ctx.c(1)]);
    b.eval = (t, h, sim) => { const v = sim.cvec[b.ins[0]]; const y = v ? (v[off] || 0) : (off === 0 ? sim.cv[b.ins[0]] : 0); setOut(sim, b.outs[0], y, inEvt(sim, b.ins)); };
    return b;
  };
  // Stack: pins [Vin, push, pop, out]
  class Stack extends Blk {
    constructor(ctx, depth) { super(ctx, [ctx.c(0), ctx.c(1), ctx.c(2)], [ctx.c(3)]); this.depth = depth; this.st = []; }
    init() { this.st = []; this.y = 0; }
    eval(t, h, sim) {
      const st = this.st.slice(); const ep = rise(sim, t, h, this.ins[1]), eo = rise(sim, t, h, this.ins[2]);
      if (ep !== null) { st.push(sim.cv[this.ins[0]]); if (st.length > this.depth) st.shift(); }
      if (eo !== null && st.length) st.pop();
      this.pst = st; const y = st.length ? st[st.length - 1] : (this.lastY || 0);
      this.y = y; setOut(sim, this.outs[0], y, ep !== null ? ep : (eo !== null ? eo : NaN));
    }
    commit() { this.st = this.pst; this.lastY = this.y; }
    saveX() { return { st: this.st.slice(), y: this.lastY }; }
    restoreX(x) { this.st = x.st.slice(); this.lastY = x.y; }
  }
  M_.STACK = ctx => new Stack(ctx, Math.max(1, ctx.P.int('depth', 8)));

  /* ------------------------------------------------------------------
   * Switch controllers and gating
   * ------------------------------------------------------------------ */
  // On-off controller: pins [in, out]
  M_.ONCTRL = ctx => {
    const b = new Blk(ctx, [ctx.c(0)], [ctx.c(1)]);
    b.eval = (t, h, sim) => { const v = hi(sim.cv[b.ins[0]]) ? 1 : 0; sim.cv[b.outs[0]] = v; sim.cev[b.outs[0]] = sim.cev[b.ins[0]]; if (v !== sim.cvP[b.outs[0]] && !(sim.cev[b.ins[0]] === sim.cev[b.ins[0]])) sim.cev[b.outs[0]] = crossT(sim, t, h, b.ins[0], -1, 0.5); };
    return b;
  };
  // Switch gating block: output pin only
  class Gating extends Blk {
    constructor(ctx, f, pts) {
      super(ctx, [], [ctx.c(0)]); this.f = f; this.pts = pts.slice(); this.gateFreq = f;
      this.T = f > 0 ? 1 / f : 0; this.tp = f > 0 ? pts.map(p => p / 360 * this.T) : pts.slice();
    }
    valAt(t) {
      const u = this.T > 0 ? t - Math.floor(t / this.T) * this.T : t;
      let v = 0; for (const p of this.tp) if (p <= u) v = 1 - v; else break;
      return v;
    }
    eval(t, h, sim) { const v = this.valAt(t + 0.5 * sim.epsT); setOut(sim, this.outs[0], v, t); }
    nextBreak(t) {
      if (this.T > 0) return S.nextPeriodic(t, this.T, this._offs || (this._offs = this.tp.map(p => p - Math.floor(p / this.T) * this.T)));
      for (const p of this.tp) if (p > t) return p; return Infinity;
    }
  }
  M_.GATING = ctx => {
    const f = ctx.P.num('f'); const pts = ctx.P.list('pts');
    const n = ctx.P.int('n', pts.length);
    if (n !== pts.length) throw ctx.err('No. of Points（' + n + '）與實際切換點數（' + pts.length + '）不符');
    for (let i = 1; i < pts.length; i++) if (pts[i] < pts[i - 1]) throw ctx.err('切換點必須遞增');
    if (f > 0 && pts.some(p => p < 0 || p > 360)) throw ctx.err('頻率大於 0 時，切換點以度數表示，須介於 0~360');
    return new Gating(ctx, f, pts);
  };
  // Alpha controller: pins [alpha, sync, enable, out]
  class Alpha extends Blk {
    constructor(ctx, f, pw) { super(ctx, [ctx.c(0), ctx.c(1), ctx.c(2)], [ctx.c(3)]); this.f = f; this.pw = pw; this.gateFreq = f; ctx.sim.reqC(this, 's', 1); }
    init() { this.s[0] = -Infinity; }
    win(ts, alpha) { const T = 1 / this.f; const a = ts + alpha / 360 * T; return [a, a + this.pw / 360 * T]; }
    eval(t, h, sim) {
      let ts = this.s[0]; const e = rise(sim, t, h, this.ins[1]); if (e !== null) ts = e; this.ts = ts;
      const alpha = sim.cv[this.ins[0]]; const en = hi(sim.cv[this.ins[2]]);
      let on = 0, ev = t;
      if (en && ts > -Infinity) { const [a, b] = this.win(ts, alpha); if (t >= a - sim.epsT && t < b - sim.epsT) on = 1; ev = on ? Math.max(a, t - h) : Math.max(b, t - h); if (ev > t) ev = t; }
      setOut(sim, this.outs[0], on, ev);
    }
    commit() { this.s[0] = this.ts; }
    nextBreak(t, sim) {
      if (!(this.s[0] > -Infinity)) return Infinity;
      const [a, b] = this.win(this.s[0], sim.cv[this.ins[0]]);
      if (a > t) return a; if (b > t) return b; return Infinity;
    }
  }
  M_.ACTRL = ctx => new Alpha(ctx, ctx.P.pos('f'), ctx.P.num('pw'));

  /* ------------------------------------------------------------------
   * Function blocks
   * ------------------------------------------------------------------ */
  function mathMk(n) {
    return ctx => {
      const ins = []; for (let i = 0; i < n; i++) ins.push(ctx.c(i));
      const names = ['t']; for (let i = 1; i <= n; i++) names.push('x' + i); if (n === 1) names.push('x');
      const f = ctx.P.fn('expr', names);
      const b = new Blk(ctx, ins, [ctx.c(n)]);
      b.eval = (t, h, sim) => { const a = [t]; for (const i of ins) a.push(sim.cv[i]); if (n === 1) a.push(a[1]); const y = f.apply(null, a); sim.cv[b.outs[0]] = y; sim.cev[b.outs[0]] = NaN; };
      return b;
    };
  }
  M_.MATH1 = mathMk(1); M_.MATH2 = mathMk(2); M_.MATH3 = mathMk(3); M_.MATH5 = mathMk(5); M_.MATH10 = mathMk(10);

  // Simplified C block: C subset translated to JavaScript.
  function cToJs(src) {
    let s = String(src || '');
    s = s.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/.*$/gm, '');
    s = s.replace(/#include[^\n]*/g, '');
    s = s.replace(/\(\s*(?:double|float)\s*\)/g, '');
    s = s.replace(/\(\s*(?:int|long|short|unsigned)\s*\)\s*([A-Za-z_][\w]*(?:\[[^\]]*\])*|\([^()]*\)|\d+\.?\d*)/g, 'Math.trunc($1)');
    // array declarations: double a[10] = {...};
    s = s.replace(/\b(?:static\s+)?(?:const\s+)?(?:unsigned\s+)?(?:double|float|int|long|short|char|bool)\s+([A-Za-z_]\w*)\s*\[\s*([^\]]+)\]\s*(=\s*\{([^}]*)\})?/g,
      (m, name, len, _e, init) => 'let ' + name + ' = _arr(' + len + (init !== undefined ? ',[' + init + ']' : '') + ')');
    s = s.replace(/\b(?:static\s+)?(?:const\s+)?(?:unsigned\s+)?(?:double|float|int|long|short|char|bool)\s+/g, 'let ');
    s = s.replace(/\bfabs\s*\(/g, 'Math.abs(').replace(/\bfmod\s*\(/g, '_fmod(');
    s = s.replace(/\b(sqrt|sin|cos|tan|asin|acos|atan|atan2|exp|log|log10|pow|floor|ceil|sinh|cosh|tanh|fmin|fmax|round|trunc)\s*\(/g, (m, f) => 'Math.' + (f === 'fmin' ? 'min' : f === 'fmax' ? 'max' : f) + '(');
    s = s.replace(/\bM_PI\b/g, 'Math.PI').replace(/\bTRUE\b|\btrue\b/g, '1').replace(/\bFALSE\b|\bfalse\b/g, '0');
    return s;
  }
  S.cToJs = cToJs;
  class CBlock extends Blk {
    constructor(ctx, nin, nout, gsrc, ssrc) {
      const ins = [], outs = []; for (let i = 0; i < nin; i++) ins.push(ctx.c(i)); for (let i = 0; i < nout; i++) outs.push(ctx.c(nin + i));
      super(ctx, ins, outs); this.nin = nin; this.nout = nout;
      const gjs = cToJs(gsrc), sjs = cToJs(ssrc);
      const gnames = []; gjs.replace(/\blet\s+([^;]+);/g, (m, list) => { list.split(',').forEach(p => { const nm = /^\s*([A-Za-z_]\w*)/.exec(p); if (nm) gnames.push(nm[1]); }); return m; });
      this.gnames = gnames;
      const xs = [], ys = []; for (let i = 1; i <= nin; i++) xs.push('x' + i); for (let i = 1; i <= nout; i++) ys.push('y' + i);
      const pre = 'const _arr=(n,v)=>{const a=new Array(Math.max(0,n|0)).fill(0);if(v)v.forEach((x,i)=>{a[i]=x;});return a;};const _fmod=(a,b)=>a%b;';
      try {
        this.gfn = Function(pre + gjs + ';return {' + gnames.map(n => n + ':' + n).join(',') + '};');
        const body = pre + 'let {' + gnames.join(',') + '}=G;' + ys.map(y => 'let ' + y + '=0;').join('') + sjs + ';\nreturn [[' + ys.join(',') + '],{' + gnames.map(n => n + ':' + n).join(',') + '}];';
        this.sfn = Function('G', 't', 'delt', ...xs, body);
      } catch (e) { throw ctx.err('C 程式碼語法錯誤：' + e.message); }
    }
    init() { try { this.G = this.gfn(); } catch (e) { throw this.ctx.err('C 程式碼初始化錯誤：' + e.message); } }
    clone(G) { const o = {}; for (const k in G) o[k] = Array.isArray(G[k]) ? G[k].slice() : G[k]; return o; }
    eval(t, h, sim) {
      let r;
      try { r = this.sfn(this.clone(this.G), t, h, ...this.ins.map(i => sim.cv[i])); } catch (e) { throw this.ctx.err('C 程式碼執行錯誤：' + e.message); }
      this.pend = r[1];
      r[0].forEach((v, j) => { const o = this.outs[j]; const y = +v; sim.cv[o] = y; sim.cev[o] = NaN; });
    }
    commit() { if (this.pend) this.G = this.pend; }
    saveX() { return this.clone(this.G); }
    restoreX(x) { this.G = this.clone(x); }
  }
  M_.CBLOCK = ctx => {
    const nin = ctx.P.int('nin', 1), nout = ctx.P.int('nout', 1);
    if (nin < 0 || nin > 10 || nout < 1 || nout > 10) throw ctx.err('輸入/輸出數量需在 0~10 / 1~10 之間');
    return new CBlock(ctx, nin, nout, ctx.P.raw('glob', ''), ctx.P.raw('code', ''));
  };

})(SIM);

