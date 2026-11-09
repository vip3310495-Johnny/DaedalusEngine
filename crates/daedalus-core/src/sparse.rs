//! Sparse LU for circuit matrices (dense-stamped A, sparse factorization).
//!
//! The simulator stamps into a dense n×n array (simple and fast for the
//! elements). Circuit matrices are very sparse, so the factorization works on
//! the structural pattern only:
//!   * symbolic phase (rare): Markowitz pivot selection with threshold partial
//!     pivoting on the actual values, recording the row/column permutation, the
//!     fill-in and a flat list of elimination operations;
//!   * numeric phase (every factorization): gather the pattern values and
//!     replay the recorded operations -- O(nnz(L+U)) instead of O(n³).
//! Each numeric factorization checks every pivot against its column
//! (|pivot| ≥ 1e-3·max) and asks for a new symbolic phase when a pivot has
//! become too small, so the result is as accurate as partial pivoting.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const PIV_SEL: f64 = 1e-2; // threshold for choosing a pivot (symbolic phase)
const PIV_OK: f64 = 1e-3; // threshold for keeping a recorded pivot (numeric phase)

static PAT_ID: AtomicU64 = AtomicU64::new(1);

/// Pattern of the stamped matrix: linear indices i*n+j that may be nonzero.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub n: usize,
    pub mask: Vec<u8>,
    pub list: Vec<u32>,
    pub ver: u64,
    pub dirty: bool,
    pub id: u64,
}

impl Pattern {
    pub fn new(n: usize) -> Self {
        Pattern { n, mask: vec![0; n * n], list: Vec::new(), ver: 0, dirty: false, id: PAT_ID.fetch_add(1, Ordering::Relaxed) }
    }
    #[inline]
    pub fn add(&mut self, k: usize) {
        if self.mask[k] == 0 {
            self.mask[k] = 1;
            self.list.push(k as u32);
            self.dirty = true;
        }
    }
    /// All positions that are nonzero now (a matrix with no stamping history).
    pub fn scan(&mut self, a: &[f64]) {
        for k in 0..self.n * self.n {
            if a[k] != 0.0 {
                self.add(k);
            }
        }
    }
    pub fn commit(&mut self) {
        if self.dirty {
            self.dirty = false;
            self.ver += 1;
        }
    }
}

/// Symbolic analysis shared by all factorizations of one pattern.
#[derive(Debug)]
pub struct Sym {
    pub ver: u64,
    pub pat_id: u64,
    pub nnz: usize,
    ns: usize,
    p: Vec<u32>,
    q: Vec<u32>,
    gather: Vec<u32>,
    ops: Vec<u32>,
    col_start: Vec<u32>,
    diag: Vec<u32>,
    lp: Vec<u32>,
    lj: Vec<u32>,
    ls: Vec<u32>,
    up: Vec<u32>,
    uj: Vec<u32>,
    us: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct Lu {
    pub n: usize,
    pub sym: Option<Arc<Sym>>,
    /// offending row when singular (-1 otherwise)
    pub bad: i64,
    v: Vec<f64>,
    y: Vec<f64>,
    xq: Vec<f64>,
    av: Vec<f64>,
    av_list: Vec<u32>,
    own: Option<Pattern>,
    pub min_piv: f64,
}

impl Lu {
    pub fn new(n: usize, sym: Option<Arc<Sym>>) -> Self {
        let mut lu = Lu { n, sym: None, bad: -1, v: Vec::new(), y: vec![0.0; n], xq: vec![0.0; n], av: Vec::new(), av_list: Vec::new(), own: None, min_piv: 0.0 };
        if let Some(s) = sym {
            lu.v = vec![0.0; s.ns];
            lu.sym = Some(s);
        }
        lu
    }

    fn sym_valid(&self, pat: &Pattern) -> bool {
        match &self.sym {
            Some(s) => s.ver == pat.ver && s.nnz == pat.list.len() && s.pat_id == pat.id,
            None => false,
        }
    }

    /* ---------- symbolic phase: Markowitz with threshold pivoting on the dense values ---------- */
    fn symbolic(&mut self, a_in: &[f64], pat: &Pattern) -> bool {
        let n = self.n;
        let mut st = vec![0u8; n * n];
        let mut a = a_in[..n * n].to_vec();
        for &k in &pat.list {
            st[k as usize] = 1;
        }
        let mut rc = vec![0i64; n];
        let mut cc = vec![0i64; n];
        let mut sc = vec![0f64; n];
        for i in 0..n {
            let mut m = 0.0f64;
            for j in 0..n {
                let k = i * n + j;
                if st[k] != 0 {
                    rc[i] += 1;
                    cc[j] += 1;
                    let v = a[k].abs();
                    if v > m {
                        m = v;
                    }
                }
            }
            if m == 0.0 {
                self.bad = i as i64;
                return false;
            }
            sc[i] = 1.0 / m;
        }
        let mut row_act = vec![true; n];
        let mut col_act = vec![true; n];
        let mut p = vec![0u32; n];
        let mut q = vec![0u32; n];
        // active rows of each column (structural), kept as a dense scan for fidelity of tie-breaking
        for k in 0..n {
            let (mut bi, mut bj, mut bm, mut bv) = (usize::MAX, usize::MAX, f64::INFINITY, 0.0f64);
            for j in 0..n {
                if !col_act[j] {
                    continue;
                }
                let mut cm = 0.0f64;
                for i in 0..n {
                    if row_act[i] && st[i * n + j] != 0 {
                        let v = a[i * n + j].abs();
                        if v > cm {
                            cm = v;
                        }
                    }
                }
                if cm == 0.0 {
                    continue;
                }
                for i in 0..n {
                    if !row_act[i] || st[i * n + j] == 0 {
                        continue;
                    }
                    let v = a[i * n + j].abs();
                    if v < PIV_SEL * cm || !(v * sc[i] > 1e-15) {
                        continue;
                    }
                    let m = ((rc[i] - 1) * (cc[j] - 1)) as f64;
                    if m < bm || (m == bm && v > bv) {
                        bm = m;
                        bv = v;
                        bi = i;
                        bj = j;
                    }
                }
            }
            if bi == usize::MAX {
                for i in 0..n {
                    if row_act[i] {
                        self.bad = i as i64;
                        break;
                    }
                }
                return false;
            }
            p[k] = bi as u32;
            q[k] = bj as u32;
            row_act[bi] = false;
            col_act[bj] = false;
            let piv = a[bi * n + bj];
            // columns of the pivot row (active)
            let prow: Vec<usize> = (0..n).filter(|&j| col_act[j] && st[bi * n + j] != 0).collect();
            for i in 0..n {
                if !row_act[i] || st[i * n + bj] == 0 {
                    continue;
                }
                let f = a[i * n + bj] / piv;
                a[i * n + bj] = f;
                rc[i] -= 1;
                for &j in &prow {
                    let kk = i * n + j;
                    if st[kk] == 0 {
                        st[kk] = 1;
                        rc[i] += 1;
                        cc[j] += 1;
                    }
                    a[kk] -= f * a[bi * n + j];
                }
            }
            cc[bj] = 0;
            for &j in &prow {
                cc[j] -= 1;
            }
        }
        // slots: one per structural entry of P·A·Q (L and U share the storage), in permuted coordinates
        let mut pr = vec![0usize; n];
        let mut qc = vec![0usize; n];
        for k in 0..n {
            pr[p[k] as usize] = k;
            qc[q[k] as usize] = k;
        }
        let mut slot = vec![-1i64; n * n];
        let mut ns = 0usize;
        for r in 0..n {
            let pr_ = p[r] as usize * n;
            for c in 0..n {
                if st[pr_ + q[c] as usize] != 0 {
                    slot[r * n + c] = ns as i64;
                    ns += 1;
                }
            }
        }
        let mut gather = Vec::with_capacity(pat.list.len());
        for &k in &pat.list {
            let k = k as usize;
            let i = k / n;
            let j = k - i * n;
            gather.push(slot[pr[i] * n + qc[j]] as u32);
        }
        // elimination operations per pivot k: for each row r > k with L(r,k): [slot(r,k), m, (slot(r,j), slot(k,j)) × m]
        let mut ops: Vec<u32> = Vec::new();
        let mut col_start = vec![0u32; n + 1];
        let mut diag = vec![0u32; n];
        for k in 0..n {
            diag[k] = slot[k * n + k] as u32;
            col_start[k] = ops.len() as u32;
            let uj: Vec<usize> = (k + 1..n).filter(|&j| slot[k * n + j] >= 0).collect();
            for r in k + 1..n {
                let sl = slot[r * n + k];
                if sl < 0 {
                    continue;
                }
                ops.push(sl as u32);
                ops.push(uj.len() as u32);
                for &j in &uj {
                    ops.push(slot[r * n + j] as u32);
                    ops.push(slot[k * n + j] as u32);
                }
            }
        }
        col_start[n] = ops.len() as u32;
        let (mut lp, mut up) = (vec![0u32; n + 1], vec![0u32; n + 1]);
        let (mut lj, mut ls, mut uj2, mut us) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for i in 0..n {
            lp[i] = lj.len() as u32;
            for j in 0..i {
                let s = slot[i * n + j];
                if s >= 0 {
                    lj.push(j as u32);
                    ls.push(s as u32);
                }
            }
            up[i] = uj2.len() as u32;
            for j in i + 1..n {
                let s = slot[i * n + j];
                if s >= 0 {
                    uj2.push(j as u32);
                    us.push(s as u32);
                }
            }
        }
        lp[n] = lj.len() as u32;
        up[n] = uj2.len() as u32;
        self.sym = Some(Arc::new(Sym { ver: pat.ver, pat_id: pat.id, nnz: pat.list.len(), ns, p, q, gather, ops, col_start, diag, lp, lj, ls, up, uj: uj2, us }));
        self.v = vec![0.0; ns];
        true
    }

    /// Numeric factorization. `pat` = pattern of A (committed). Returns false when singular.
    pub fn factor(&mut self, a: &[f64], pat: &Pattern) -> bool {
        let n = self.n;
        self.bad = -1;
        if !self.sym_valid(pat) && !self.symbolic(a, pat) {
            return false;
        }
        for attempt in 0..2 {
            let sy = self.sym.clone().unwrap();
            let v = &mut self.v;
            for x in v.iter_mut() {
                *x = 0.0;
            }
            for (e, &k) in pat.list.iter().enumerate() {
                v[sy.gather[e] as usize] += a[k as usize];
            }
            let ops = &sy.ops;
            let cs = &sy.col_start;
            let diag = &sy.diag;
            let mut ok = true;
            let mut min_piv = f64::INFINITY;
            for k in 0..n {
                let piv = v[diag[k] as usize];
                let ap = piv.abs();
                if ap < min_piv {
                    min_piv = ap;
                }
                // pivot check against the column below (the entries partial pivoting would consider)
                let mut cm = ap;
                let mut o = cs[k] as usize;
                let oe = cs[k + 1] as usize;
                while o < oe {
                    let av = v[ops[o] as usize].abs();
                    if av > cm {
                        cm = av;
                    }
                    o += 2 + 2 * ops[o + 1] as usize;
                }
                if !(ap >= PIV_OK * cm) || cm == 0.0 {
                    ok = false;
                    self.bad = sy.p[k] as i64;
                    break;
                }
                let inv = 1.0 / piv;
                o = cs[k] as usize;
                while o < oe {
                    let sl = ops[o] as usize;
                    let m = ops[o + 1] as usize;
                    let f = v[sl] * inv;
                    v[sl] = f;
                    o += 2;
                    if f != 0.0 {
                        let end = o + 2 * m;
                        while o < end {
                            let t = v[ops[o + 1] as usize];
                            v[ops[o] as usize] -= f * t;
                            o += 2;
                        }
                    } else {
                        o += 2 * m;
                    }
                }
            }
            if ok {
                self.bad = -1;
                self.min_piv = min_piv;
                return true;
            }
            // a recorded pivot became too small: redo the pivot selection with the present values
            if attempt == 0 && !self.symbolic(a, pat) {
                return false;
            }
        }
        false
    }

    /// Factor a matrix with no stamping history (pattern = present nonzeros).
    pub fn factor_dense(&mut self, a: &[f64]) -> bool {
        let mut own = self.own.take().unwrap_or_else(|| Pattern::new(self.n));
        own.scan(a);
        own.commit();
        let r = self.factor(a, &own);
        self.own = Some(own);
        r
    }

    pub fn solve(&mut self, b: &[f64], x: &mut [f64]) {
        let n = self.n;
        let sy = self.sym.as_ref().unwrap();
        let v = &self.v;
        let y = &mut self.y;
        let xq = &mut self.xq;
        for i in 0..n {
            let mut s = b[sy.p[i] as usize];
            for o in sy.lp[i] as usize..sy.lp[i + 1] as usize {
                s -= v[sy.ls[o] as usize] * y[sy.lj[o] as usize];
            }
            y[i] = s;
        }
        for i in (0..n).rev() {
            let mut s = y[i];
            for o in sy.up[i] as usize..sy.up[i + 1] as usize {
                s -= v[sy.us[o] as usize] * xq[sy.uj[o] as usize];
            }
            xq[i] = s / v[sy.diag[i] as usize];
        }
        for i in 0..n {
            x[sy.q[i] as usize] = xq[i];
        }
    }

    /// Keep a compact copy of A's pattern values (for the residual after A has been overwritten).
    pub fn keep(&mut self, a: &[f64], pat: &Pattern) {
        self.av.clear();
        self.av_list.clear();
        self.av_list.extend_from_slice(&pat.list);
        self.av.extend(pat.list.iter().map(|&k| a[k as usize]));
    }

    /// r = b - A x using the kept values; returns true when any component is nonzero.
    pub fn residual(&self, b: &[f64], x: &[f64], r: &mut [f64]) -> bool {
        let n = self.n;
        r[..n].copy_from_slice(&b[..n]);
        for (e, &k) in self.av_list.iter().enumerate() {
            let k = k as usize;
            let i = k / n;
            r[i] -= self.av[e] * x[k - i * n];
        }
        r[..n].iter().any(|&v| v != 0.0)
    }
}

/// Dense complex LU with scaled partial pivoting (AC analysis).
pub struct Clu {
    n: usize,
    re: Vec<f64>,
    im: Vec<f64>,
    p: Vec<usize>,
    pub bad: i64,
}

impl Clu {
    pub fn new(n: usize) -> Self {
        Clu { n, re: vec![0.0; n * n], im: vec![0.0; n * n], p: vec![0; n], bad: -1 }
    }
    pub fn factor(&mut self, ar: &[f64], ai: &[f64]) -> bool {
        let n = self.n;
        self.re.copy_from_slice(&ar[..n * n]);
        self.im.copy_from_slice(&ai[..n * n]);
        self.bad = -1;
        let (re, im, p) = (&mut self.re, &mut self.im, &mut self.p);
        let mut sc = vec![0f64; n];
        for i in 0..n {
            p[i] = i;
            let mut m = 0.0f64;
            for j in 0..n {
                let v = re[i * n + j].hypot(im[i * n + j]);
                if v > m {
                    m = v;
                }
            }
            if m == 0.0 {
                self.bad = i as i64;
                return false;
            }
            sc[i] = 1.0 / m;
        }
        for k in 0..n {
            let (mut piv, mut best) = (k, -1.0f64);
            for i in k..n {
                let v = re[i * n + k].hypot(im[i * n + k]) * sc[i];
                if v > best {
                    best = v;
                    piv = i;
                }
            }
            if !(best > 1e-15) {
                self.bad = p[piv] as i64;
                return false;
            }
            if piv != k {
                for j in 0..n {
                    re.swap(k * n + j, piv * n + j);
                    im.swap(k * n + j, piv * n + j);
                }
                sc.swap(k, piv);
                p.swap(k, piv);
            }
            let (pr, pi) = (re[k * n + k], im[k * n + k]);
            let d = pr * pr + pi * pi;
            let (ir, ii) = (pr / d, -pi / d);
            for i in k + 1..n {
                let (ar_, ai_) = (re[i * n + k], im[i * n + k]);
                if ar_ == 0.0 && ai_ == 0.0 {
                    continue;
                }
                let fr = ar_ * ir - ai_ * ii;
                let fi = ar_ * ii + ai_ * ir;
                re[i * n + k] = fr;
                im[i * n + k] = fi;
                for j in k + 1..n {
                    let (br, bi) = (re[k * n + j], im[k * n + j]);
                    re[i * n + j] -= fr * br - fi * bi;
                    im[i * n + j] -= fr * bi + fi * br;
                }
            }
        }
        true
    }
    pub fn solve(&self, br: &[f64], bi: &[f64], xr: &mut [f64], xi: &mut [f64]) {
        let n = self.n;
        let (re, im, p) = (&self.re, &self.im, &self.p);
        let mut yr = vec![0f64; n];
        let mut yi = vec![0f64; n];
        for i in 0..n {
            let (mut sr, mut si) = (br[p[i]], bi[p[i]]);
            for j in 0..i {
                let (ar, ai) = (re[i * n + j], im[i * n + j]);
                sr -= ar * yr[j] - ai * yi[j];
                si -= ar * yi[j] + ai * yr[j];
            }
            yr[i] = sr;
            yi[i] = si;
        }
        for i in (0..n).rev() {
            let (mut sr, mut si) = (yr[i], yi[i]);
            for j in i + 1..n {
                let (ar, ai) = (re[i * n + j], im[i * n + j]);
                sr -= ar * xr[j] - ai * xi[j];
                si -= ar * xi[j] + ai * xr[j];
            }
            let (dr, di) = (re[i * n + i], im[i * n + i]);
            let d = dr * dr + di * di;
            xr[i] = (sr * dr + si * di) / d;
            xi[i] = (si * dr - sr * di) / d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn solve_small() {
        let n = 3;
        let a = vec![4.0, 1.0, 0.0, 1.0, 3.0, 1.0, 0.0, 1.0, 2.0];
        let mut pat = Pattern::new(n);
        pat.scan(&a);
        pat.commit();
        let mut lu = Lu::new(n, None);
        assert!(lu.factor(&a, &pat));
        let b = vec![1.0, 2.0, 3.0];
        let mut x = vec![0.0; 3];
        lu.solve(&b, &mut x);
        lu.keep(&a, &pat);
        let mut r = vec![0.0; 3];
        lu.residual(&b, &x, &mut r);
        for v in r {
            assert!(v.abs() < 1e-14);
        }
    }
}
