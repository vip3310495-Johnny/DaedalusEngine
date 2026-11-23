"""Build and check .psimx circuit files for DaedalusEngine (also opened by the web version).

    from psimx_builder import Doc
    d = Doc()
    vin = d.E('VDC', 4, 13, 0, {'V': 24}, 'Vin')
    d.W(d.pin(vin, '+'), (4, 6), (12, 6))
    ...
    report = d.check()          # open pins, single labels, unknown types, ...
    d.save('circuit.psimx')

Subcircuits (PSIM style): s = d.subckt('RC') returns the Doc of a definition; draw it like a circuit and give it
ports with s.port(x, y, 'in', 'L'); then place blocks with d.E('SUBCKT', x, y, 0, {'def': 'RC', 'vars': 'R = 2k'}, 'X1')
and use d.pin(block, 'in') like any pin. Finish a definition before placing blocks of it.

Pin offsets and parameter defaults come from elements.json, generated from the program's own element library
(tools/skill/gen.js), so every element type is covered. Coordinates are integer grid units, y grows downward.
Connectivity follows the program exactly: points with the same coordinates are joined, a point lying strictly
inside a wire joins that wire (T junction), labels with the same text are joined (inside a subcircuit a port
joins the labels of its name); crossing wires without a shared point are NOT joined.
"""
import json
import math
import os

_HERE = os.path.dirname(os.path.abspath(__file__))
with open(os.path.join(_HERE, 'elements.json'), encoding='utf-8') as _f:
    LIB = json.load(_f)['elements']

# probes and meters: their names are the waveform names
PROBES = {'VP', 'VP2', 'IP', 'VDCM', 'VACM', 'ADCM', 'AACM', 'WATT', 'WH3', 'DEVP', 'ACP', 'ACP2'}
# elements of a definition that the main circuit's simulation does not use
MAIN_ONLY = {'SIMCTRL', 'ACSWEEP', 'PSWEEP', 'MONTECARLO'}
VERSION = 'V1.6.0'
# subcircuit port kinds (as stored in a block's "ports") <-> pin kinds of elements.json
_KIND_IN = {'p': 'P', 'ci': 'Ci', 'co': 'Co', 'a': 'A'}


def cblock_pins(nin, nout):
    """pins of the simplified C block: inputs x1..xn on the left, outputs y1..ym on the right"""
    n, m = int(nin), int(nout)
    tn, tm = -((n - 1) // 2), -((m - 1) // 2)
    return [['x%d' % (i + 1), -2, tn + i, 'Ci'] for i in range(n)] + [['y%d' % (i + 1), 2, tm + i, 'Co'] for i in range(m)]


def _text_units(s, sz):
    return sum(0.56 if ord(c) < 128 else 1.0 for c in s) * sz


def sub_geom(params):
    """pins and box of a subcircuit block: the same rules as the program (daedalus_core::library::sub_geom).
    Returns (pins [[name, x, y, kind]], bb [x0, y0, x1, y1])."""
    try:
        raw = json.loads(params.get('ports') or '[]')
    except ValueError:
        raw = []
    ports = [{'name': q[0], 'side': (q[1] if len(q) > 1 and q[1] in ('L', 'R', 'T', 'B') else 'L'),
              'kind': (q[2] if len(q) > 2 and q[2] in _KIND_IN else 'a')} for q in raw if q]
    dname = params.get('def', '')
    l, r, t, b = ([p for p in ports if p['side'] == s] for s in 'LRTB')

    def start(n):
        return -math.floor((n - 1) / 2)

    def reach(n):
        return max(0.0, math.ceil((n - 1) / 2))
    nv, nh = max(len(l), len(r), 1), max(len(t), len(b), 1)

    def wmax(v):
        return max([_text_units(p['name'], 0.42) for p in v] + [0.0])
    inner_w = max(wmax(l) + wmax(r) + _text_units(dname, 0.6) + 1.2, 2.8)
    ht = max(reach(nv) + 1, 2)
    if t:
        ht += 1
    if b:
        ht += 1
    tw = max([_text_units(p['name'], 0.42) for p in t + b] + [0.0])
    step = math.ceil(tw + 0.3) if nh > 1 and tw > 0.9 else 1
    hw = max(math.ceil(inner_w / 2 + 0.6), reach(nh) * step + 1, 2)
    pins = []
    for i, p in enumerate(l):
        pins.append([p['name'], -hw, start(len(l)) + i, _KIND_IN[p['kind']]])
    for i, p in enumerate(r):
        pins.append([p['name'], hw, start(len(r)) + i, _KIND_IN[p['kind']]])
    for i, p in enumerate(t):
        pins.append([p['name'], (start(len(t)) + i) * step, -ht, _KIND_IN[p['kind']]])
    for i, p in enumerate(b):
        pins.append([p['name'], (start(len(b)) + i) * step, ht, _KIND_IN[p['kind']]])
    pins = [[n, int(x), int(y), k] for n, x, y, k in pins]
    return pins, [-hw, -ht, hw, ht]


def pins_of(el):
    if el['type'] == 'CBLOCK':
        p = el.get('params', {})
        return cblock_pins(float(p.get('nin', 1)), float(p.get('nout', 1)))
    if el['type'] == 'SUBCKT':
        return sub_geom(el.get('params', {}))[0]
    return LIB[el['type']]['pins']


def xf(el, x, y):
    """element-local -> world: mirror x first, then rotate clockwise by rot * 90 degrees (as the program)"""
    if el.get('mir'):
        x = -x
    r = el.get('rot', 0) & 3
    ex, ey = el['x'], el['y']
    return {0: (ex + x, ey + y), 1: (ex - y, ey + x), 2: (ex - x, ey - y), 3: (ex + y, ey - x)}[r]


def _port_name(e):
    return (e.get('params', {}).get('text') or '').strip()


def _nets(els, wires):
    """the program's connectivity: (find, pins [(element, pin name, kind, point)], label count by text)"""
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

    pins = []
    for e in els:
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
    ptset = set(pts)
    for w in wires:
        x1, y1, x2, y2 = w['x1'], w['y1'], w['x2'], w['y2']
        for (x, y) in ptset:
            if (x, y) in ((x1, y1), (x2, y2)):
                continue
            inside = (y == y1 == y2 and min(x1, x2) < x < max(x1, x2)) or (x == x1 == x2 and min(y1, y2) < y < max(y1, y2))
            if inside:
                uni((x, y), (x1, y1))
    # labels (and, inside a definition, ports) with the same text
    lbl, count = {}, {}
    for e, n, k, p in pins:
        if e['type'] in ('LABEL', 'SUBPORT'):
            t = _port_name(e)
            if not t:
                continue
            count.setdefault(t, {'LABEL': 0, 'SUBPORT': 0})[e['type']] += 1
            if t in lbl:
                uni(p, lbl[t])
            else:
                lbl[t] = p
    return find, pins, count


def def_ports(els, wires):
    """[[name, side, kind], ...] of a definition, in the program's pin order (left and right side top to bottom,
    top and bottom side left to right); kind from what the port connects to inside"""
    live = [e for e in els if not e.get('dis') and e.get('type') in LIB]
    ports = []
    for e in live:
        if e['type'] != 'SUBPORT':
            continue
        n = _port_name(e)
        if not n or any(p[3] == n for p in ports):
            continue
        side = e.get('params', {}).get('side', 'L')
        ports.append((side if side in ('L', 'R', 'T', 'B') else 'L', e['x'], e['y'], n))
    rank = 'LRTB'.index
    ports.sort(key=lambda p: (rank(p[0]), (p[2], p[1]) if p[0] in 'LR' else (p[1], p[2])))
    find, pins, _c = _nets(live, wires)
    kinds = {}
    for e, n, k, p in pins:
        kinds.setdefault(find(p), set()).add(k)
    out = []
    for side, _x, _y, name in ports:
        ks = set()
        for e in live:
            if e['type'] == 'SUBPORT' and _port_name(e) == name:
                ks |= kinds.get(find((e['x'], e['y'])), set())
        kind = 'p' if 'P' in ks else 'co' if 'Co' in ks else 'ci' if ks & {'Ci', 'G'} else 'a'
        out.append([name, side, kind])
    return out


def used_defs(subckts, els):
    """names of the definitions the elements use, directly or through other definitions"""
    out, todo = [], [e.get('params', {}).get('def', '') for e in els if e.get('type') == 'SUBCKT']
    while todo:
        n = todo.pop()
        if n in out:
            continue
        out.append(n)
        d = subckts.get(n)
        if d:
            todo += [e.get('params', {}).get('def', '') for e in d['elements'] if e.get('type') == 'SUBCKT']
    return out


class Doc:
    """Minimal .psimx builder (the main circuit, or a subcircuit definition made by subckt())."""
    DIRS = {'l': (-1, 0), 'r': (1, 0), 'u': (0, -1), 'd': (0, 1)}

    def __init__(self, _def_name=None, _parent=None):
        self.els, self.wires, self.nid, self.screens = [], [], 1, None
        self._names = set()
        self.subckts = {}          # name -> Doc (main circuit only)
        self.def_name = _def_name  # set for a subcircuit definition
        self._parent = _parent

    def _root(self):
        return self._parent or self

    # ---------------------------------------------------------------- elements and wires
    def E(self, typ, x, y, rot=0, params=None, name='', mir=False, dis=False):
        """place an element; params as strings or numbers (others keep their defaults)"""
        if typ not in LIB:
            raise KeyError('unknown element type %s' % typ)
        params = dict(params or {})
        if typ == 'SUBCKT':
            for k in params:
                if k not in ('def', 'vars'):
                    raise KeyError('SUBCKT has only def (the subcircuit name) and vars (this block\'s values, e.g. "R = 2k")')
            dn = params.get('def', '')
            root = self._root()
            if dn not in root.subckts:
                raise KeyError('no subcircuit %r: make it first with d.subckt(%r) (have: %s)' % (dn, dn, ', '.join(root.subckts) or 'none'))
            if self.def_name and self.def_name in [dn] + used_defs(root._raw_defs(), root.subckts[dn].els):
                raise ValueError('subcircuit %s cannot contain %s: it would contain itself' % (self.def_name, dn))
            params['ports'] = json.dumps(def_ports(root.subckts[dn].els, root.subckts[dn].wires), ensure_ascii=False, separators=(',', ':'))
        else:
            known = LIB[typ]['params']
            for k in params:
                if k not in known:
                    raise KeyError('%s has no parameter %r (has: %s)' % (typ, k, ', '.join(known) or 'none'))
        if typ == 'SUBPORT':
            if not self.def_name:
                raise ValueError('a subcircuit port belongs inside a subcircuit: s = d.subckt(name); s.port(...)')
            t = str(params.get('text', '')).strip()
            if not t or any(e['type'] == 'SUBPORT' and _port_name(e) == t for e in self.els):
                raise ValueError('a port needs a name of its own (text=%r)' % t)
        if self.def_name and typ in MAIN_ONLY:
            raise ValueError('%s belongs to the main circuit, not to subcircuit %s' % (typ, self.def_name))
        if 'noname' in LIB[typ]['flags']:
            name = ''
        elif not name:
            raise ValueError('%s needs a name (probe names are waveform names)' % typ)
        if name:
            if name.lower() in self._names:
                raise ValueError('duplicate element name %s (names ignore case)' % name)
            self._names.add(name.lower())
        el = {'id': self.nid, 'type': typ, 'x': int(x), 'y': int(y), 'rot': int(rot) & 3, 'mir': bool(mir), 'dis': bool(dis),
              'name': name, 'params': {k: str(v) for k, v in params.items()}}
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

    # ---------------------------------------------------------------- subcircuits
    def subckt(self, name):
        """a new subcircuit definition (a Doc to draw in); place blocks of it with E('SUBCKT', ..., {'def': name})"""
        if self.def_name:
            raise ValueError('definitions belong to the main circuit: call subckt() on it')
        name = str(name).strip()
        if not name or any(ord(c) < 32 for c in name) or len(name) > 40:
            raise ValueError('a subcircuit name: 1-40 characters, no control characters')
        if name in self.subckts:
            raise ValueError('subcircuit %s exists already' % name)
        d = Doc(name, self)
        self.subckts[name] = d
        return d

    def port(self, x, y, name, side='L', rot=None):
        """a subcircuit port: its pin is at (x, y) and becomes the block's pin `name` on side L, R, T or B;
        by default the outline points away from the circuit (L: to the left, R: right, T: up, B: down)"""
        if side not in ('L', 'R', 'T', 'B'):
            raise ValueError('side must be L, R, T or B')
        if rot is None:
            rot = {'L': 2, 'R': 0, 'T': 3, 'B': 1}[side]
        return self.E('SUBPORT', x, y, rot, {'text': name, 'side': side})

    def _raw_defs(self):
        return {k: {'elements': d.els, 'wires': d.wires} for k, d in self.subckts.items()}

    def _sync_ports(self):
        """every block's pin list from its definition; a block placed before its definition got its last ports
        would end on the wrong points, so that is an error"""
        stale = []
        for owner in [self] + list(self.subckts.values()):
            for e in owner.els:
                if e['type'] != 'SUBCKT':
                    continue
                d = self.subckts.get(e['params'].get('def', ''))
                if d is None:
                    continue
                p = json.dumps(def_ports(d.els, d.wires), ensure_ascii=False, separators=(',', ':'))
                if e['params'].get('ports') != p:
                    old = [q[0] for q in pins_of(e)]
                    e['params']['ports'] = p
                    if old != [q[0] for q in pins_of(e)]:
                        stale.append('%s%s' % (e['name'], ' (in %s)' % owner.def_name if owner.def_name else ''))
        return stale

    # ---------------------------------------------------------------- file
    def to_json(self, version=VERSION):
        if self.def_name:
            raise ValueError('save the main circuit (the definitions are stored in it)')
        stale = self._sync_ports()
        if stale:
            raise ValueError('ports of a subcircuit changed after its blocks were placed: %s; finish the definition first' % ', '.join(stale))
        doc = {'elements': self.els, 'wires': self.wires, 'nid': self.nid}
        if self.subckts:
            doc['subckts'] = {k: {'elements': d.els, 'wires': d.wires, 'nid': d.nid} for k, d in self.subckts.items()}
        out = {'app': 'psimweb', 'version': version, 'doc': doc}
        if self.screens:
            out['screens'] = self.screens
        return out

    def save(self, path, version=VERSION):
        j = self.to_json(version)
        with open(path, 'w', encoding='utf-8') as f:
            json.dump(j, f, ensure_ascii=False, indent=1)
        return path

    @classmethod
    def _from_raw(cls, doc, name=None, parent=None):
        d = cls(name, parent)
        d.els, d.wires = doc.get('elements', []), doc.get('wires', [])
        d.nid = max([doc.get('nid', 1)] + [e.get('id', 0) + 1 for e in d.els] + [w.get('id', 0) + 1 for w in d.wires])
        d._names = {e['name'].lower() for e in d.els if e.get('name')}
        return d

    @classmethod
    def load(cls, path):
        with open(path, encoding='utf-8') as f:
            j = json.load(f)
        doc = j.get('doc', j)
        d = cls._from_raw(doc)
        d.screens = j.get('screens')
        for k, s in (doc.get('subckts') or {}).items():
            d.subckts[k] = cls._from_raw(s, k, d)
        return d

    # ---------------------------------------------------------------- checks
    def check(self):
        if self.def_name:
            return check_doc(self.els, self.wires, self._parent._raw_defs(), is_def=True)
        self._sync_ports()
        return check_doc(self.els, self.wires, self._raw_defs())


def check_doc(els, wires, subckts=None, is_def=False):
    """Static checks with the program's connectivity rules. Returns a dict; 'errors' and 'open_pins' and
    'single_labels' must be empty before a circuit is handed over. `subckts` (name -> {elements, wires}) are the
    main circuit's subcircuit definitions: each is checked too, and its findings are prefixed with its name."""
    subckts = subckts or {}
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
    if not is_def:
        if not any(e['type'] == 'SIMCTRL' for e in live):
            errors.append('no Simulation Control (SIMCTRL) element')
        if not any(e['type'] in PROBES for e in live) and not any(any(x['type'] in PROBES for x in s['elements']) for s in subckts.values()):
            warnings.append('no probe: a transient simulation records nothing')
        if any(e['type'] == 'SUBPORT' for e in live):
            warnings.append('a subcircuit port in the main circuit does nothing (ports belong inside subcircuits)')
    else:
        for e in live:
            if e['type'] in MAIN_ONLY:
                warnings.append('%s inside a subcircuit is ignored (it belongs to the main circuit)' % e['type'])
        seen = set()
        for e in live:
            if e['type'] == 'SUBPORT':
                t = _port_name(e)
                if not t:
                    errors.append('a port without a name')
                elif t in seen:
                    errors.append('two ports named %s (they would be one pin)' % t)
                seen.add(t)
    # blocks: their definitions and pins
    for e in live:
        if e['type'] != 'SUBCKT':
            continue
        dn = e.get('params', {}).get('def', '')
        d = subckts.get(dn)
        if d is None:
            errors.append('block %s: no subcircuit %r' % (e.get('name'), dn))
            continue
        want = [q[0] for q in def_ports(d['elements'], d['wires'])]
        have = [q[0] for q in pins_of(e)]
        if sorted(want) != sorted(have):
            errors.append('block %s: pins %s but subcircuit %s has ports %s (place the block after finishing the definition)' % (e.get('name'), have, dn, want))

    find, pins, count = _nets(live, wires)
    # a label alone joins nothing; inside a definition a label named like a port is that port's net
    single = sorted(t for t, c in count.items() if c['LABEL'] + c['SUBPORT'] < 2 and c['LABEL'] > 0)
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
            who = 'port %s' % _port_name(e) if e['type'] == 'SUBPORT' else (e.get('name') or e['type'])
            open_pins.append('%s.%s at %s' % (who, n, p))
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
        if e['type'] in ('LABEL', 'GND', 'TEXT', 'SUBPORT'):
            continue
        k = (e['x'], e['y'])
        if k in spots:
            warnings.append('%s and %s placed on the same spot %s' % (spots[k], e.get('name') or e['type'], k))
        spots[k] = e.get('name') or e['type']
    rep = {'errors': errors, 'open_pins': open_pins, 'single_labels': single, 'warnings': warnings,
           'elements': len(live), 'wires': len(wires), 'probes': [e['name'] for e in live if e['type'] in PROBES]}
    if not is_def:
        # every definition, and no definition containing itself
        for k, d in subckts.items():
            if k in used_defs(subckts, d['elements']):
                errors.append('subcircuit %s contains itself (directly or through other blocks)' % k)
            r = check_doc(d['elements'], d['wires'], subckts, is_def=True)
            for key in ('errors', 'open_pins', 'single_labels', 'warnings'):
                rep[key] += ['subcircuit %s: %s' % (k, x) for x in r[key]]
        rep['subckts'] = {k: [q[0] for q in def_ports(d['elements'], d['wires'])] for k, d in subckts.items()}
    return rep


def check_file(path):
    with open(path, encoding='utf-8') as f:
        j = json.load(f)
    doc = j.get('doc', j)
    return check_doc(doc.get('elements', []), doc.get('wires', []), doc.get('subckts'))


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
