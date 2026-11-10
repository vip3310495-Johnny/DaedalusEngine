"""Build and check .psimx circuit files for DaedalusEngine (also opened by the web version).

    from psimx_builder import Doc
    d = Doc()
    vin = d.E('VDC', 4, 13, 0, {'V': 24}, 'Vin')
    d.W(d.pin(vin, '+'), (4, 6), (12, 6))
    ...
    report = d.check()          # open pins, single labels, unknown types, ...
    d.save('circuit.psimx')

Pin offsets and parameter defaults come from elements.json, generated from the program's own element library
(tools/skill/gen.js), so every element type is covered. Coordinates are integer grid units, y grows downward.
Connectivity follows the program exactly: points with the same coordinates are joined, a point lying strictly
inside a wire joins that wire (T junction), labels with the same text are joined; crossing wires without a shared
point are NOT joined.
"""
import json
import os

_HERE = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(_HERE, 'elements.json'), encoding='utf-8') as _f:
    LIB = json.load(_f)['elements']

# probes and meters: their names are the waveform names
PROBES = {'VP', 'VP2', 'IP', 'VDCM', 'VACM', 'ADCM', 'AACM', 'WATT', 'WH3', 'DEVP', 'ACP', 'ACP2'}
VERSION = 'V1.4.0'


def cblock_pins(nin, nout):
    """pins of the simplified C block: inputs x1..xn on the left, outputs y1..ym on the right"""
    n, m = int(nin), int(nout)
    tn, tm = -((n - 1) // 2), -((m - 1) // 2)
    return [['x%d' % (i + 1), -2, tn + i, 'Ci'] for i in range(n)] + [['y%d' % (i + 1), 2, tm + i, 'Co'] for i in range(m)]


def pins_of(el):
    if el['type'] == 'CBLOCK':
        p = el.get('params', {})
        return cblock_pins(float(p.get('nin', 1)), float(p.get('nout', 1)))
    return LIB[el['type']]['pins']


def xf(el, x, y):
    """element-local -> world: mirror x first, then rotate clockwise by rot * 90 degrees (as the program)"""
    if el.get('mir'):
        x = -x
    r = el.get('rot', 0) & 3
    ex, ey = el['x'], el['y']
    return {0: (ex + x, ey + y), 1: (ex - y, ey + x), 2: (ex - x, ey - y), 3: (ex + y, ey - x)}[r]


class Doc:
    """Minimal .psimx builder."""
    DIRS = {'l': (-1, 0), 'r': (1, 0), 'u': (0, -1), 'd': (0, 1)}

    def __init__(self):
        self.els, self.wires, self.nid, self.screens = [], [], 1, None
        self._names = set()

    # ---------------------------------------------------------------- elements and wires
    def E(self, typ, x, y, rot=0, params=None, name='', mir=False, dis=False):
        """place an element; params as strings or numbers (others keep their defaults)"""
        if typ not in LIB:
            raise KeyError('unknown element type %s' % typ)
        known = LIB[typ]['params']
        for k in (params or {}):
            if k not in known:
                raise KeyError('%s has no parameter %r (has: %s)' % (typ, k, ', '.join(known) or 'none'))
        if 'noname' in LIB[typ]['flags']:
            name = ''
        elif not name:
            raise ValueError('%s needs a name (probe names are waveform names)' % typ)
        if name:
            if name.lower() in self._names:
                raise ValueError('duplicate element name %s (names ignore case)' % name)
            self._names.add(name.lower())
        el = {'id': self.nid, 'type': typ, 'x': int(x), 'y': int(y), 'rot': int(rot) & 3, 'mir': bool(mir), 'dis': bool(dis),
              'name': name, 'params': {k: str(v) for k, v in (params or {}).items()}}
        self.nid += 1
        self.els.append(el)
        return el

    def pin(self, el, pname):
        for n, px, py, _k in pins_of(el):
            if n == pname:
                return xf(el, px, py)
        raise KeyError('%s has no pin %s (has: %s)' % (el['type'], pname, ', '.join(p[0] for p in pins_of(el))))

    def W(self, *pts):
        """orthogonal polyline through grid points"""
        for a, b in zip(pts, pts[1:]):
            a, b = (int(a[0]), int(a[1])), (int(b[0]), int(b[1]))
            if a == b:
                continue
            if a[0] != b[0] and a[1] != b[1]:
                raise ValueError('wire segment must be horizontal or vertical: %s -> %s' % (a, b))
            self.wires.append({'id': self.nid, 'x1': a[0], 'y1': a[1], 'x2': b[0], 'y2': b[1]})
            self.nid += 1

    def label(self, pt, net):
        return self.E('LABEL', pt[0], pt[1], 0, {'text': net})

    def gnd(self, pt):
        return self.E('GND', pt[0], pt[1])

    def text(self, x, y, s):
        return self.E('TEXT', x, y, 0, {'text': s})

    def stub(self, pt, direction, net, length=1):
        """short wire from pt ('l', 'r', 'u', 'd') ending in a ground symbol (net='GND') or a label"""
        dx, dy = self.DIRS[direction]
        e = (pt[0] + dx * length, pt[1] + dy * length)
        self.W(pt, e)
        return self.gnd(e) if net == 'GND' else self.label(e, net)

    def tie(self, el, pname, net, direction, length=1):
        return self.stub(self.pin(el, pname), direction, net, length)

    # ---------------------------------------------------------------- file
    def to_json(self, version=VERSION):
        out = {'app': 'psimweb', 'version': version, 'doc': {'elements': self.els, 'wires': self.wires, 'nid': self.nid}}
        if self.screens:
            out['screens'] = self.screens
        return out

    def save(self, path, version=VERSION):
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(self.to_json(version), f, ensure_ascii=False, indent=1)
        return path

    @classmethod
    def load(cls, path):
        with open(path, encoding='utf-8') as f:
            j = json.load(f)
        doc = j.get('doc', j)
        d = cls()
        d.els, d.wires = doc.get('elements', []), doc.get('wires', [])
        d.nid = max([doc.get('nid', 1)] + [e.get('id', 0) + 1 for e in d.els] + [w.get('id', 0) + 1 for w in d.wires])
        d.screens = j.get('screens')
        d._names = {e['name'].lower() for e in d.els if e.get('name')}
        return d

    # ---------------------------------------------------------------- checks
    def check(self):
        return check_doc(self.els, self.wires)


def check_doc(els, wires):
    """Static checks with the program's connectivity rules. Returns a dict; 'errors' and 'open_pins' and
    'single_labels' must be empty before a circuit is handed over."""
    errors, warnings = [], []
    live = [e for e in els if not e.get('dis')]
    for e in els:
        if e.get('type') not in LIB:
            errors.append('unknown element type %s (the program drops it silently)' % e.get('type'))
    live = [e for e in live if e.get('type') in LIB]
    names = {}
    for e in els:
        n = (e.get('name') or '').lower()
        if n:
            if n in names:
                errors.append('duplicate element name %s' % e['name'])
            names[n] = e
    for w in wires:
        if w['x1'] != w['x2'] and w['y1'] != w['y2']:
            errors.append('wire %s is not horizontal or vertical' % w.get('id'))
    if not any(e['type'] == 'SIMCTRL' for e in live):
        errors.append('no Simulation Control (SIMCTRL) element')
    if not any(e['type'] in PROBES for e in live):
        warnings.append('no probe: a transient simulation records nothing')

    # union-find over points
    par = {}

    def find(a):
        par.setdefault(a, a)
        while par[a] != a:
            par[a] = par[par[a]]
            a = par[a]
        return a

    def uni(a, b):
        ra, rb = find(a), find(b)
        if ra != rb:
            par[ra] = rb

    pins = []  # (element, pin name, kind, point)
    for e in live:
        for n, px, py, k in pins_of(e):
            p = xf(e, px, py)
            pins.append((e, n, k, p))
            find(p)
    pts = [p for *_x, p in pins]
    for w in wires:
        a, b = (w['x1'], w['y1']), (w['x2'], w['y2'])
        uni(a, b)
        pts += [a, b]
    # T junctions: any point strictly inside a wire
    for w in wires:
        x1, y1, x2, y2 = w['x1'], w['y1'], w['x2'], w['y2']
        for (x, y) in set(pts):
            if (x, y) in ((x1, y1), (x2, y2)):
                continue
            inside = (y == y1 == y2 and min(x1, x2) < x < max(x1, x2)) or (x == x1 == x2 and min(y1, y2) < y < max(y1, y2))
            if inside:
                uni((x, y), (x1, y1))
    # labels with the same text
    lbl = {}
    count = {}
    for e, n, k, p in pins:
        if e['type'] == 'LABEL':
            t = e['params'].get('text', '').strip()
            if not t:
                continue
            count[t] = count.get(t, 0) + 1
            if t in lbl:
                uni(p, lbl[t])
            else:
                lbl[t] = p
    single = sorted(t for t, c in count.items() if c < 2)
    # members of each net: element pins and wires (as the program counts them)
    members = {}
    for e, n, k, p in pins:
        r = find(p)
        members[r] = members.get(r, 0) + 1
    for w in wires:
        r = find((w['x1'], w['y1']))
        members[r] = members.get(r, 0) + 1
    open_pins = []
    for e, n, k, p in pins:
        if e['type'] in ('LABEL', 'GND'):
            continue
        if members.get(find(p), 0) <= 1:
            open_pins.append('%s.%s at %s' % (e.get('name') or e['type'], n, p))
    # power and control on one net
    kinds = {}
    who = {}
    for e, n, k, p in pins:
        r = find(p)
        kinds.setdefault(r, set()).add(k)
        who.setdefault(r, []).append('%s.%s' % (e.get('name') or e['type'], n))
    for r, ks in kinds.items():
        if 'P' in ks and ks & {'Ci', 'Co', 'G'}:
            errors.append('power and control pins on one net: %s (connect them through VSEN / ISEN / ONCTRL / CTOP)' % ', '.join(who[r][:6]))
    drivers = {}
    for e, n, k, p in pins:
        if k == 'Co':
            drivers.setdefault(find(p), []).append('%s.%s' % (e.get('name') or e['type'], n))
    for r, outs in drivers.items():
        if len(outs) > 1:
            errors.append('two control outputs drive one net: %s' % ', '.join(outs))
    # layout warnings: a wire end or pin of one net lying on another net's wire is joined by the T rule,
    # so overlapping parallel wires and elements stacked on the same spot are suspicious
    seen = {}
    for w in wires:
        k = tuple(sorted([(w['x1'], w['y1']), (w['x2'], w['y2'])]))
        if k in seen:
            warnings.append('duplicate wire %s and %s' % (seen[k], w.get('id')))
        seen[k] = w.get('id')
    spots = {}
    for e in live:
        if e['type'] in ('LABEL', 'GND', 'TEXT'):
            continue
        k = (e['x'], e['y'])
        if k in spots:
            warnings.append('%s and %s placed on the same spot %s' % (spots[k], e.get('name') or e['type'], k))
        spots[k] = e.get('name') or e['type']
    return {'errors': errors, 'open_pins': open_pins, 'single_labels': single, 'warnings': warnings,
            'elements': len(live), 'wires': len(wires), 'probes': [e['name'] for e in live if e['type'] in PROBES]}


def check_file(path):
    d = Doc.load(path)
    return d.check()


if __name__ == '__main__':
    import sys
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    if len(sys.argv) != 2:
        print('usage: python psimx_builder.py <circuit.psimx>   (static check)')
        sys.exit(1)
    rep = check_file(sys.argv[1])
    print(json.dumps(rep, ensure_ascii=False, indent=1))
    sys.exit(1 if rep['errors'] or rep['open_pins'] or rep['single_labels'] else 0)
