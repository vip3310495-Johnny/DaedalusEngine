"""Example: a subcircuit (RC low-pass with parameters R, C and a probe inside) used by two blocks.

usage: python subcircuit_rc.py [out.psimx]
then:  python ../scripts/verify_psimx.py rc.psimx --expect Vo1:final:9.99:10.01 --expect X1.Vc:final:9.99:10.01 --expect Vo2:final:9.92:9.94

X1 uses the defaults (tau = R*C = 1 ms), X2 sets R = 2k (tau = 2 ms); a 10 V step for 10 ms:
Vo1 = 10(1 - e^-10) = 9.9995 V, Vo2 = 10(1 - e^-5) = 9.9326 V. The probe Vc inside the definition is recorded
once per block as X1.Vc and X2.Vc.
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'scripts'))
from psimx_builder import Doc  # noqa: E402

d = Doc()

# ------------------------------------------------------------ the definition (finish it before placing blocks)
s = d.subckt('RCF')
s.text(0, -9, '子電路 RCF：in → R → out，C 接地；參數 R、C')
s.E('PARAMS', 4, -5, 0, {'text': 'R = 1k\nC = 1u'})
pin_in = s.port(0, 0, 'in', 'L')                     # pin (0,0), outline to the left
r1 = s.E('R', 3, 0, 0, {'R': 'R'}, 'R1')              # (1,0) - (5,0)
c1 = s.E('C', 8, 2, 1, {'C': 'C'}, 'C1')              # rot 1: (8,0) - (8,4)
s.port(12, 0, 'out', 'R')                             # pin (12,0), outline to the right
s.W(s.pin(pin_in, 'port'), s.pin(r1, '1'))
s.W(s.pin(r1, '2'), (12, 0))                          # output rail: C1 and the probe sit on it (T junctions)
s.gnd(s.pin(c1, '2'))
s.E('VP', 10, -2, 0, {}, 'Vc')                        # pin (10,0) inside the rail
rep = s.check()
assert not (rep['errors'] or rep['open_pins'] or rep['single_labels']), rep

# ------------------------------------------------------------ the main circuit
d.text(0, -9, '子電路範例：兩個 RCF 方塊，X1 用預設值（τ = 1 ms），X2 指定 R = 2k（τ = 2 ms）')
d.E('SIMCTRL', 30, -6, 0, {'dt': '1u', 'tt': '10m'})
v1 = d.E('VDC', 0, 4, 0, {'V': 10}, 'V1')             # + (0,2), - (0,6)
d.gnd(d.pin(v1, '-'))
x1 = d.E('SUBCKT', 10, 2, 0, {'def': 'RCF'}, 'X1')
x2 = d.E('SUBCKT', 10, 10, 0, {'def': 'RCF', 'vars': 'R = 2k'}, 'X2')
d.W(d.pin(v1, '+'), d.pin(x1, 'in'))
bus = (4, d.pin(v1, '+')[1])                          # a T on the input wire, down to X2
d.W(bus, (4, d.pin(x2, 'in')[1]), d.pin(x2, 'in'))
for blk, probe in ((x1, 'Vo1'), (x2, 'Vo2')):
    o = d.pin(blk, 'out')
    d.W(o, (o[0] + 3, o[1]))
    d.E('VP', o[0] + 3, o[1] - 2, 0, {}, probe)       # VP pin is 2 below its origin: on the wire's end
d.screens = [['Vo1', 'Vo2'], ['X1.Vc', 'X2.Vc']]

if __name__ == '__main__':
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    rep = d.check()
    print({k: rep[k] for k in ('errors', 'open_pins', 'single_labels', 'warnings', 'subckts')})
    if rep['errors'] or rep['open_pins'] or rep['single_labels']:
        sys.exit(1)
    print('saved', d.save(sys.argv[1] if len(sys.argv) > 1 else 'rc.psimx'))
