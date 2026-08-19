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

# IS IT WORTH WAITING? The verdict below says only yes or no, and a caller who has just read a
# reassuring `uptime` or a single %idle deserves to know WHY those disagree with it and whether
# retrying in ten minutes is likely to help. Two cheap statistics answer that:
#
#   * the longest run of CONSECUTIVE samples that agree within tolerance — a window that never holds
#     still for more than two seconds is oscillating, and waiting for the same length of run to
#     appear again will not make it measurable;
#   * first half vs second half — a host that is SETTLING is worth another sweep; one that is
#     steady-but-busy or worsening is not.
#
# Reported on every run rather than only on refusal, so a passing window still shows its margin.
best_run=1
run=1
for (( k = 1; k < SAMPLES; k++ )); do
  lo=${counts[k]}; hi=${counts[k]}
  for (( j = k - run; j <= k; j++ )); do
    (( counts[j] < lo )) && lo=${counts[j]}
    (( counts[j] > hi )) && hi=${counts[j]}
  done
  if (( hi - lo <= SPREAD_TOLERANCE )); then
    run=$(( run + 1 ))
  else
    run=1
  fi
  (( run > best_run )) && best_run=$run
done

half=$(( SAMPLES / 2 ))
first_sum=0; second_sum=0
for (( k = 0; k < half; k++ )); do first_sum=$(( first_sum + counts[k] )); done
for (( k = half; k < SAMPLES; k++ )); do second_sum=$(( second_sum + counts[k] )); done
first_mean=$(( first_sum / half ))
second_mean=$(( second_sum / (SAMPLES - half) ))
if (( second_mean * 4 < first_mean * 3 )); then
  trend="SETTLING (busy $first_mean -> $second_mean) — another sweep may qualify"
elif (( second_mean * 3 > first_mean * 4 )); then
  trend="WORSENING (busy $first_mean -> $second_mean) — do not queue a run behind this"
else
  trend="STEADY (busy $first_mean -> $second_mean)"
fi
echo "  longest agreeing run: $best_run of $SAMPLES samples   trend: $trend"
# The single number a spot check would have produced, next to what the sweep saw. This is the
# disagreement that keeps sending callers to the wrong conclusion: the quietest sample is a real
# observation, it is just not a description of the window.
echo "  a one-shot spot check could have reported as few as $min busy cpus (best sample) or as many as $max"

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
