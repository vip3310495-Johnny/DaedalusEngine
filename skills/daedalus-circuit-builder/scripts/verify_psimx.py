"""Check a .psimx file and simulate it with DaedalusEngine's command-line simulator.

usage: python verify_psimx.py circuit.psimx [--cli PATH] [--tt 20m] [--ac] [--window 0.8 1.0]
                              [--expect NAME:STAT:LO:HI ...] [--keep out.csv]

1. static check (psimx_builder.check_doc): errors, open pins, labels placed only once
2. daedalus-cli runs the circuit (transient, or the AC sweep with --ac); netlist and simulation errors are shown
3. every probe: min, max, final value, mean over the last 10 % and over --window (fractions of the run)

STAT for --expect: min, max, final, avg10 (mean of the last 10 %), avg (mean over --window).
The command-line simulator is found with --cli, the DAEDALUS_CLI environment variable, the installed program
(%LOCALAPPDATA%\\Programs\\DaedalusEngine\\daedalus-cli.exe) or PATH.
Exit code 0 only when the static check is clean, the simulation ran and every --expect holds.
"""
import argparse
import csv
import json
import os
import shutil
import subprocess
import sys

if hasattr(sys.stdout, 'reconfigure'):
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from psimx_builder import check_doc  # noqa: E402


def find_cli(arg):
    cands = [arg, os.environ.get('DAEDALUS_CLI')]
    la = os.environ.get('LOCALAPPDATA')
    if la:
        cands.append(os.path.join(la, 'Programs', 'DaedalusEngine', 'daedalus-cli.exe'))
    for c in cands:
        if c and os.path.isfile(c):
            return c
    return shutil.which('daedalus-cli') or shutil.which('daedalus-cli.exe')


def stats(t, y, window):
    n = len(y)
    t0, t1 = t[0], t[-1]
    span = (t1 - t0) or 1.0

    def mean(a, b):
        vals = [v for tv, v in zip(t, y) if t0 + a * span <= tv <= t0 + b * span]
        return sum(vals) / len(vals) if vals else float('nan')
    return {'min': min(y), 'max': max(y), 'final': y[n - 1], 'avg10': mean(0.9, 1.0), 'avg': mean(window[0], window[1])}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('file')
    ap.add_argument('--cli')
    ap.add_argument('--tt', help='override the total simulation time (for example 20m)')
    ap.add_argument('--ac', action='store_true', help='run the AC sweep (ACSWEEP element) instead of a transient')
    ap.add_argument('--window', nargs=2, type=float, default=[0.8, 1.0], metavar=('FROM', 'TO'))
    ap.add_argument('--expect', action='append', default=[], metavar='NAME:STAT:LO:HI')
    ap.add_argument('--keep', help='keep the simulated CSV here')
    a = ap.parse_args()

    with open(a.file, encoding='utf-8') as f:
        j = json.load(f)
    doc = j.get('doc', j)
    rep = {'file': a.file, 'check': check_doc(doc.get('elements', []), doc.get('wires', []))}
    ok = not (rep['check']['errors'] or rep['check']['open_pins'] or rep['check']['single_labels'])

    cli = find_cli(a.cli)
    if not cli:
        rep['simulation'] = 'daedalus-cli not found: pass --cli or set DAEDALUS_CLI (the static check above still applies)'
        print(json.dumps(rep, ensure_ascii=False, indent=1))
        sys.exit(1)
    tmp = tempfile.mkdtemp(prefix='psimx_')
    src = os.path.join(tmp, 'circuit.psimx')
    if a.tt:
        for e in doc.get('elements', []):
            if e.get('type') == 'SIMCTRL':
                e.setdefault('params', {})['tt'] = a.tt
    with open(src, 'w', encoding='utf-8') as f:
        json.dump(j, f, ensure_ascii=False)
    out = a.keep or os.path.join(tmp, 'out.csv')
    cmd = [cli, src, out] + (['--ac'] if a.ac else [])
    p = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8', errors='replace')
    if p.returncode != 0:
        rep['simulation'] = {'exit': p.returncode, 'error': (p.stderr or p.stdout).strip().splitlines()[:20]}
        print(json.dumps(rep, ensure_ascii=False, indent=1))
        sys.exit(1)
    last = (p.stdout.strip().splitlines() or ['{}'])[-1]
    try:
        info = json.loads(last)
    except ValueError:
        info = {'output': last}
    rep['simulation'] = {k: info[k] for k in ('secs', 'steps', 'refined', 'nonConv', 'events') if k in info}
    rep['warnings'] = info.get('warnings', [])
    with open(out, encoding='utf-8') as f:
        rows = list(csv.reader(f))
    head, body = rows[0], rows[1:]
    cols = {h: [float(r[i]) for r in body] for i, h in enumerate(head)}
    probes = {}
    if a.ac:
        freq = cols['Freq']
        for h in head[1:]:
            if h.endswith('_re'):
                n = h[:-3]
                mag = [(re * re + im * im) ** 0.5 for re, im in zip(cols[n + '_re'], cols[n + '_im'])]
                probes[n] = {'f_min': freq[0], 'f_max': freq[-1], 'mag_first': mag[0], 'mag_last': mag[-1], 'mag_max': max(mag)}
    else:
        t = cols['Time']
        for h in head[1:]:
            probes[h] = stats(t, cols[h], a.window)
        rep['points'] = len(t)
    rep['probes'] = probes
    fails = []
    for x in a.expect:
        try:
            name, st, lo, hi = x.rsplit(':', 3)
            v = probes[name][st]
            if not (float(lo) <= v <= float(hi)):
                fails.append('%s %s = %g not in [%s, %s]' % (name, st, v, lo, hi))
        except (KeyError, ValueError) as e:
            fails.append('%s: %s' % (x, e))
    rep['expect_failed'] = fails
    print(json.dumps(rep, ensure_ascii=False, indent=1))
    if not a.keep:
        shutil.rmtree(tmp, ignore_errors=True)
    sys.exit(0 if ok and not fails else 1)


if __name__ == '__main__':
    main()
