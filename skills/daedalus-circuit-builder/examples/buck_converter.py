"""Example: 24 V -> 12 V buck converter, 50 kHz, open loop (D = 0.5), drawn by the layout rules of SKILL.md.

usage: python buck_converter.py [out.psimx]
then:  python ../scripts/verify_psimx.py buck.psimx --expect Vo:avg10:11.5:12.5 --expect IL:avg10:1.9:2.1

Power stage on rails (bus y=6, output y=14, ground y=20), wired directly; the control area below (y >= 26) is
wired directly too and reaches the power stage only through the interface label GATE (placed on both sides).
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'scripts'))
from psimx_builder import Doc  # noqa: E402

d = Doc()
E, W, P = d.E, d.W, d.pin

d.text(2, 1, '【主回路 Power Stage】24 V → 12 V、50 kHz、D = 0.5（開迴路）')
d.text(2, 25, '【控制區 Control】PWM：常數 0.5 與 0～1 V 三角載波比較　介面：GATE')
E('SIMCTRL', 46, 3, 0, {'dt': '0.2u', 'tt': '10m'})

# power stage
vin = E('VDC', 4, 13, 0, {'V': 24}, 'Vin')
q1 = E('MOSFET', 12, 10, 0, {}, 'Q1')                  # D (12,8), S (12,12), G (10,10)
dfw = E('DIODE', 12, 17, 3, {}, 'Dfw')                 # rot 3: K (12,15) on top, A (12,19)
l1 = E('L', 18, 14, 0, {'L': '100u'}, 'L1')            # (16,14) - (20,14)
il = E('IP', 22, 14, 0, {}, 'IL')                      # in (20,14), out (24,14)
co = E('RC', 28, 17, 1, {'R': '10m', 'C': '100u'}, 'Co')  # rot 1: (28,14) - (28,20)
rl = E('R', 34, 16, 1, {'R': 6}, 'RL')                 # rot 1: (34,14) - (34,18)

W(P(vin, '+'), (4, 6), (12, 6), P(q1, 'D'))            # bus
W(P(q1, 'S'), P(dfw, 'K'))                              # switch node, vertical
W((12, 14), P(l1, '1'))                                 # T junction on the switch node
W(P(l1, '2'), P(il, 'in'))
W(P(il, 'out'), (40, 14))                               # output rail: Co and RL hang on it
W(P(rl, '2'), (34, 20))
W(P(vin, '-'), (4, 20), (40, 20))                       # ground rail
W(P(dfw, 'A'), (12, 20))
d.gnd((4, 21))
W((4, 20), (4, 21))
d.tie(q1, 'G', 'GATE', 'l', 2)                          # interface label (power side)

# probes on the nodes they measure
E('VP', 40, 12, 0, {}, 'Vo')                            # pin (40,14) = end of the output rail
E('VP', 14, 12, 0, {}, 'Vsw')                           # pin (14,14) inside the switch-node wire

# control
duty = E('CONST', 1, 30, 0, {'v': 0.5}, 'Duty')         # out (3,30)
cmp1 = E('COMP', 8, 31, 0, {}, 'PWM')                   # in+ (5,30), in- (5,32), out (11,31)
car = E('VTRI', 2, 36, 0, {'Vpp': 1, 'f': '50k'}, 'Vcar')  # + (2,34), - (2,38)
W(P(duty, 'out'), P(cmp1, 'in+'))
W(P(car, '+'), (2, 32), P(cmp1, 'in-'))
d.tie(car, '-', 'GND', 'd')
d.tie(cmp1, 'out', 'GATE', 'r', 2)                      # interface label (control side)

d.screens = [['Vo'], ['IL'], ['Vsw']]

if __name__ == '__main__':
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    out = sys.argv[1] if len(sys.argv) > 1 else 'buck.psimx'
    rep = d.check()
    bad = rep['errors'] or rep['open_pins'] or rep['single_labels']
    print('check:', 'OK' if not bad else rep)
    d.save(out)
    print('saved', out)
    sys.exit(1 if bad else 0)
