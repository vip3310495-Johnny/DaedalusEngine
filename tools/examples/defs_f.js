// Example 27: subcircuits (definitions with ports and parameters, blocks with their own values, a definition that
// contains blocks of another one, probes inside blocks named block.probe).
module.exports = (EX, H) => {
  const { row } = H;

  // =================================================================== 27
  {
    const V = 10, R = 1e3, Cc = 1e-6, Rb = 10e3, Rf = 2.2e3;
    const rc = (t, r) => V * (1 - Math.exp(-t / (r * Cc)));
    // two RC sections in cascade (R, C then Rb, C): x' = A x + b, x(0) = 0, x(∞) = [V, V]
    const a11 = -(1 / (R * Cc) + 1 / (Rb * Cc)), a12 = 1 / (Rb * Cc), a21 = 1 / (Rb * Cc), a22 = -1 / (Rb * Cc);
    const tr = a11 + a22, det = a11 * a22 - a12 * a21, dsc = Math.sqrt(tr * tr / 4 - det);
    const l1 = tr / 2 + dsc, l2 = tr / 2 - dsc;
    const v1 = [a12, l1 - a11], v2 = [a12, l2 - a11];
    // c1 v1 + c2 v2 = -[V, V]
    const dd = v1[0] * v2[1] - v2[0] * v1[1];
    const c1 = (-V * v2[1] + V * v2[0]) / dd, c2 = (-V * v1[0] + V * v1[1]) / dd;
    const two = (t, k) => V + c1 * Math.exp(l1 * t) * v1[k] + c2 * Math.exp(l2 * t) * v2[k];
    EX.push({
      key: 'subcircuit_rc', cat: '子電路', name: '子電路：RC 低通方塊（各方塊參數、巢狀子電路、方塊內的探棒）',
      text: '子電路「RC」：R、C 與探棒 Vc，參數 R = 1k、C = 1u；埠 in（左）、out（右）。子電路「TWO」：兩個 RC 方塊串接，第二個指定 R = 10k。\nX1 用預設值（τ = 1 ms）；X3 指定 R = Rf（主電路參數檔 Rf = 2.2k，τ = 2.2 ms）；X2 為 TWO（兩節 RC 串接，理論解為 2×2 線性系統的指數解）。\n方塊裡的探棒在波形中以「方塊名稱.探棒名稱」出現：X1.Vc、X3.Vc、X2.XA.Vc（第一節）、X2.XB.Vc（第二節）。右鍵方塊「進入子電路」（Ctrl+E）看內容。',
      sc: { dt: '1u', tt: '10m' },
      params: 'Rf = 2.2k',
      subckts: {
        RC: {
          ports: [['in', 'L', 'A'], ['out', 'R', 'B']],
          params: 'R = 1k\nC = 1u',
          parts: [
            ['R', 'R1', { R: 'R' }, { 1: 'A', 2: 'B' }],
            ['C', 'C1', { C: 'C' }, { 1: 'B', 2: '0' }],
            ['VP', 'Vc', {}, { n: 'B' }],
          ],
        },
        TWO: {
          ports: [['i', 'L', 'I'], ['o', 'R', 'O']],
          parts: [
            ['SUBCKT', 'XA', { def: 'RC' }, { in: 'I', out: 'M' }],
            ['SUBCKT', 'XB', { def: 'RC', vars: 'R = 10k' }, { in: 'M', out: 'O' }],
          ],
        },
      },
      parts: [
        ['VDC', 'V1', { V }, { '+': 'vin', '-': '0' }],
        ['SUBCKT', 'X1', { def: 'RC' }, { in: 'vin', out: 'o1' }],
        ['SUBCKT', 'X2', { def: 'TWO' }, { i: 'vin', o: 'o2' }],
        ['SUBCKT', 'X3', { def: 'RC', vars: 'R = Rf' }, { in: 'vin', out: 'o3' }],
        ['VP', 'Vo1', {}, { n: 'o1' }],
        ['VP', 'Vo2', {}, { n: 'o2' }],
        ['VP', 'Vo3', {}, { n: 'o3' }],
      ],
      screens: [['Vo1', 'Vo3'], ['X2.XA.Vc', 'Vo2']],
      checks: h => {
        const r = [];
        const e1 = h.err('Vo1', t => rc(t, R));
        r.push(row('X1（預設 R = 1k）：Vo1 = 10(1 − e^(−t/1ms)) 的最大誤差', 0, e1.e, e1.e < 1e-4 * V, `t = ${e1.at.toExponential(2)}`));
        const e3 = h.err('Vo3', t => rc(t, Rf));
        r.push(row('X3（R = Rf = 2.2k）：Vo3 = 10(1 − e^(−t/2.2ms)) 的最大誤差', 0, e3.e, e3.e < 1e-4 * V, `t = ${e3.at.toExponential(2)}`));
        for (const t of [1e-3, 5e-3, 10e-3]) {
          r.push(row(`X2 巢狀：第一節 X2.XA.Vc(${t * 1e3} ms)`, two(t, 0), h.at('X2.XA.Vc', t), h.near(h.at('X2.XA.Vc', t), two(t, 0), 1e-4, 1e-6)));
          r.push(row(`X2 巢狀：第二節 Vo2(${t * 1e3} ms)`, two(t, 1), h.at('Vo2', t), h.near(h.at('Vo2', t), two(t, 1), 1e-4, 1e-6)));
        }
        let d = 0;
        h.t.forEach((t, i) => { d = Math.max(d, Math.abs(h.col('X1.Vc')[i] - h.col('Vo1')[i]), Math.abs(h.col('X2.XB.Vc')[i] - h.col('Vo2')[i])); });
        r.push(row('方塊內的探棒 X1.Vc、X2.XB.Vc 與方塊外的 Vo1、Vo2 相同（最大差）', 0, d, d < 1e-12));
        return r;
      },
    });
  }
};
