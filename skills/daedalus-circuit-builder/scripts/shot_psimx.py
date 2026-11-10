"""Screenshot a .psimx file in DaedalusEngine for a visual check of the layout (Windows).

usage: python shot_psimx.py circuit.psimx out_prefix [--run] [--exe PATH]

Opens the circuit in DaedalusEngine, zoomed to fit, saves <out_prefix>_main.png and closes the program.
With --run the circuit is also simulated and the waveform viewer is saved as <out_prefix>_result.png.
The program is found with --exe, the DAEDALUS_EXE environment variable or the installed program
(%LOCALAPPDATA%\\Programs\\DaedalusEngine\\DaedalusEngine.exe). It needs a desktop session (it opens a window).
"""
import argparse
import os
import subprocess
import sys

if hasattr(sys.stdout, 'reconfigure'):
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')


def find_exe(arg):
    cands = [arg, os.environ.get('DAEDALUS_EXE')]
    la = os.environ.get('LOCALAPPDATA')
    if la:
        cands.append(os.path.join(la, 'Programs', 'DaedalusEngine', 'DaedalusEngine.exe'))
    for c in cands:
        if c and os.path.isfile(c):
            return c
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('file')
    ap.add_argument('prefix')
    ap.add_argument('--run', action='store_true')
    ap.add_argument('--exe')
    ap.add_argument('--timeout', type=float, default=300)
    a = ap.parse_args()
    exe = find_exe(a.exe)
    if not exe:
        print('DaedalusEngine.exe not found: pass --exe or set DAEDALUS_EXE')
        sys.exit(1)
    args = [exe, os.path.abspath(a.file), '--shot', os.path.abspath(a.prefix)] + (['--run'] if a.run else [])
    subprocess.run(args, timeout=a.timeout)
    shots = [p for p in (a.prefix + '_main.png', a.prefix + '_result.png') if os.path.isfile(p)]
    print('\n'.join(shots) if shots else 'no screenshot was written')
    sys.exit(0 if shots else 1)


if __name__ == '__main__':
    main()
