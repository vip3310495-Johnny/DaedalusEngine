#!/usr/bin/env bash
# Physical / theoretical verification of the examples, AC sweeps and picosecond time precision.
# usage: tools/verify.sh   (requires node; builds the CLI first)
set -e
cd "$(dirname "$0")/.."
N="${NODE:-node}"
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p daedalus-cli --release -q
CLI=./target/release/daedalus-cli
D=tests/phys
mkdir -p $D
for e in buck rlc bridge tl431 doublepulse supervisor; do
  "$N" reference/harness.js netlist reference/examples/$e.psimx $D/$e.net.json
  $CLI reference/examples/$e.psimx $D/$e.csv >/dev/null
done
"$N" tools/variant.js $D/buck.net.json $D/buck_long.json '{"dt":2e-7,"tTotal":0.02}'
"$N" tools/variant.js $D/bridge.net.json $D/bridge_long.json '{"dt":1e-5,"tTotal":0.6}'
"$N" tools/variant.js $D/tl431.net.json $D/tl431_k.json '{"dt":1e-6,"tTotal":0.02,"printStep":5}' Vk=4 Va=3
for v in buck_long bridge_long tl431_k; do $CLI $D/$v.json $D/$v.csv >/dev/null; done
$CLI reference/examples/buckac.psimx $D/buckac_ac.csv --ac >/dev/null
$CLI reference/examples/rlc.psimx $D/rlc_ac.csv --ac >/dev/null
echo "== transient waveforms vs theory / exact piecewise solutions"
"$N" tools/phys_check.js $D
echo "== AC sweeps vs transfer functions"
"$N" tools/ac_check.js
echo "== picosecond time precision"
"$N" tools/ps_check.js $CLI tests/ps
