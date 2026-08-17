#!/usr/bin/env bash
# Decide whether a timed vs-incumbent run is worth starting on this host.
#
# WHY THIS EXISTS: three instruments disagree here, and two of them mislead.
#   * loadavg counts runnable AND uninterruptible tasks, so a widely-forking bench put the 1-minute
#     average at 73 while the machine was ~88% idle. It should gate nothing.
#   * a single %idle or per-CPU spot check is no better, because this host SWINGS between saturated
#     and nearly idle within seconds. Measured twice on 2026-08-17:
#         64 64 64 64 64 64 64 64 19 21 19 18
#         64 64 64 64 64 64 64 37 25 19 22 16 64 64
#     A spot check lands wherever it lands: "clean" at one second, "hammered" at the next.
#
# So the check is a SEQUENCE that must AGREE. A window whose samples disagree is unmeasurable no
# matter how good its best sample looks, because an A/B arm landing in a 64-busy second against one
# landing in a 16-busy second compares phases of someone else's job rather than two engines.
#
# Usage:  scripts/window_check.sh [samples]        (default 12)
# Exit:   0 = measurable, 1 = not. Prints the numbers to record on any banked row.

set -uo pipefail

SAMPLES="${1:-12}"
CEILING_PCT=20       # a CPU is "busy" above this
MAX_OVER=0           # the admission gate wants ALL cpus under the ceiling
SPREAD_TOLERANCE=4   # max-min across samples; above this the window is too volatile to trust

# ONE mpstat call per sample yields BOTH the per-cpu count and that sample's aggregate idle.
#
# The first version of this script took the count sequence first and the idle figure afterwards, and
# then printed them together — two different instants presented as one reading. It reported "64 cpus
# over the ceiling" beside "82.5% idle", which cannot both describe the same second, and the
# contradiction is what exposed the bug. Numbers compared against each other have to come from the
# same sample; that is the same trap as two harnesses that do not share an input.
counts=()
idles=()
while (( ${#counts[@]} < SAMPLES )); do
  read -r c i < <(mpstat -P ALL 1 1 2>/dev/null | awk -v ceil="$CEILING_PCT" '
    $2 ~ /^[0-9]+$/ { if ((100-$NF) > ceil) n++ }
    $2 == "all"     { agg = $NF }
    END { printf "%d %.1f\n", n+0, agg+0 }')
  counts+=("$c")
  idles+=("$i")
done

min=${counts[0]}; max=${counts[0]}; sum=0
for c in "${counts[@]}"; do
  (( c < min )) && min=$c
  (( c > max )) && max=$c
  sum=$(( sum + c ))
done
spread=$(( max - min ))
mean=$(( sum / SAMPLES ))

# Idle reported as the RANGE across the same samples, not a mean: a mean of 40% across samples of
# 82% and 3% describes neither, which is precisely how a volatile window reads as acceptable.
idle_min=${idles[0]}; idle_max=${idles[0]}
for i in "${idles[@]}"; do
  awk -v a="$i" -v b="$idle_min" 'BEGIN{exit !(a<b)}' && idle_min=$i
  awk -v a="$i" -v b="$idle_max" 'BEGIN{exit !(a>b)}' && idle_max=$i
done
idle="${idle_min}-${idle_max}"
iowait=$(mpstat 1 2 2>/dev/null | awk '/^Average:/ {print $(NF-3)}')
loadavg=$(awk '{print $1" / "$2" / "$3}' /proc/loadavg)
mhz=$(awk '/cpu MHz/ {print $4}' /proc/cpuinfo | sort -n | awk 'NR==1{lo=$1} {hi=$1} END {printf "%.0f-%.0f MHz (spread %.2fx)", lo, hi, hi/lo}')

echo "samples (cpus over ${CEILING_PCT}% busy): ${counts[*]}"
echo "  min=$min max=$max spread=$spread mean=$mean"
echo "  idle=${idle}% (range across the SAME samples)  iowait=${iowait}%  loadavg=${loadavg}"
echo "  per-sample idle: ${idles[*]}"
echo "  cpu $mhz"
echo
echo "RECORD THESE ON ANY BANKED ROW: loadavg, idle%, per-arm cpu MHz."

if (( spread > SPREAD_TOLERANCE )); then
  echo "VERDICT: NOT MEASURABLE — samples disagree (spread $spread > $SPREAD_TOLERANCE)."
  echo "  A window whose samples disagree is unmeasurable regardless of its best sample."
  exit 1
fi

if (( max > MAX_OVER )); then
  echo "VERDICT: GATE WOULD REFUSE — $max cpu(s) over the ${CEILING_PCT}% ceiling; it requires 0."
  echo "  Stable but not quiet. An ungated A/B/B/A row is defensible here IF it records these"
  echo "  conditions and is banked as UNCERTIFIED."
  exit 1
fi

echo "VERDICT: MEASURABLE — samples agree and all cpus are under the ceiling."
exit 0
