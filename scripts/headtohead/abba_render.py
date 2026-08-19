#!/usr/bin/env python3
"""Render-scoped A/B/B/A against pinned mermaid-js, in ONE invocation.

This is the script that produced the PROVISIONAL 362.4x row in docs/NEGATIVE_EVIDENCE.md
("PROVISIONAL MEASUREMENT: sequence_20 render, worst bound 362.4x"). It lived in a scratchpad, which
made that row unreproducible by anyone else; it is here so the number can be re-derived or refuted.

WHAT IT IS NOT
--------------
It is NOT a certification harness and it must not be used as one. `scripts/headtohead/run.mjs` owns
certification, and its host-wide exclusivity gate -- every one of 64 CPUs below 20% busy in a single
1-second sample -- has refused eight consecutive attempts on this host and has no override flag. This
script deliberately runs the same two arms WITHOUT that gate so a number exists at all, and records
the conditions per arm so a reader can discount it appropriately. Any row it produces is PROVISIONAL.

WHY A/B/B/A IN ONE INVOCATION
-----------------------------
Interleaved in a single process so both arms see the same machine state, and so fm is measured on
BOTH sides of the incumbent. The bracket yields a drift figure: if the two fm observations disagree
by much, the window was not stable and the ratio is not worth quoting.

⚠️ DRIFT IS NOT SUFFICIENT, and this paragraph used to imply it was. Drift compares the two fm arms
against EACH OTHER, so it is blind whenever BOTH are degraded the same way. Measured: a run whose fm
arms were 138,338 and 145,397 ns passed with drift 1.0510x and would have quoted a bound of 242.1x
— while the same binary in the same window produced 91,000-96,000 ns whenever its arm was not
contended. A sibling run caught the same contention only because it happened to hit ONE arm
(149,974 vs 92,576, drift 1.6200x).

The absolute tell is the CALIBRATED BATCH, which every record already carries. Calibration targets
3 ms, so a contended core shrinks the batch and raises per-op time together: every contended
observation ran at batch 20-25, every clean one at 37-39. Compare the batch against the clean norm
before quoting, and treat a collapsed batch as "this arm was contended" no matter what drift says.

WHY THE RATIO IS A BOUND, NOT A POINT ESTIMATE
----------------------------------------------
It divides the FASTER mermaid observation by the SLOWER fm observation -- the worst bound either arm
produced, per the fleet's replicated-standing convention. The headline median/median is printed too,
but the bound is the number to quote. Per-core clocks on this host span ~1429-4300 MHz simultaneously
(2.88-3.01x spread), and that confounder is unaccounted for, which is the other reason for a bound.

RENDER MODE, NOT PARSE
----------------------
This measures RENDER and asserts its work proof in band rather than trusting the timing.

Do NOT reach for `parse_accepted_revisions` as that work proof. It is a parse-QUALITY counter, not a
liveness counter: it increments only when a revision has no errors, no recovery, no warnings, AND
`support_label() == "full"` -- and `Sequence` is the one type labelled `partial`, so it is
identically ZERO for every sequence diagram however well the parse ran. Gating on it would silently
refuse every sequence row, including `sequence_20`, the project's worst measured ratio. See the
RETRACTED entry in docs/NEGATIVE_EVIDENCE.md; I made exactly that mistake.

Separately and still unexplained: `FM_H2H_MODE=parse` reports `parse_ns.p50 = 8` for a 1,257-byte
diagram, which is ~25 cycles and not plausible. That is a reason to distrust the parse arm's timing
on its own merits, not a reason to trust the counter above.

USAGE
-----
    python3 scripts/headtohead/abba_render.py --fm-bin <path>

`--fm-bin` should be a binary you have PINNED BY CONTENT (copy it to `<exe>.<agent>.<sha8>` and pass
that), because the shared build path can be rebuilt by a peer mid-run -- that has happened to this
harness before.

DO NOT PASS `--corpus`. It is still accepted, for deliberately pinning an input, but the default now
GENERATES the case from `corpus.mjs` -- the same module the incumbent arm consumes -- and prints its
sha256. The old usage line said `--corpus <corpus.json>`, and the obvious file to reach for,
`.benchmarks/headtohead/corpus.json`, is a stale local artifact written when the schema field was
`text`; the binary now requires `texts`. That combination cost a whole A/B/B/A invocation: the fm arm
returned `ns=None`, and the byte-level preflight the ledger prescribes had already PASSED, because
the text bytes were identical (sha 31c0dd6b) and only the CONTAINER had moved. Generating the corpus
here removes both the staleness and the divergence by construction rather than by assertion.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import statistics
import subprocess
import sys
import tempfile

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "mermaid_bench.mjs")
PICK_PINS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pick_pins.mjs")
CORPUS_MJS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "corpus.mjs")

# Physics ceiling from the work-proof gate: no single observed thread emits more than this.
MAX_BYTES_PER_NS = 512.0

# The fm bracket's own drift IS an A/A null on our arm: two runs of the SAME binary over the SAME
# input inside ONE invocation. Until now this script computed it, printed it, and quoted the ratio
# anyway -- the identical defect this file already names for `incumbent_starved` further down
# ("a guard that exists and is never read is the same as no guard"). Observed live: a bracket whose
# fm arm drifted 1.7078x still printed "WORST-BOUND RATIO: 228.1x".
#
# The ceiling is empirical, so publish the split rather than asserting a physics bound:
#
#   honest brackets (unpinned)   1.0094, 1.0119, 1.0201, 1.0204, 1.0481   worst EXCESS 0.0481
#   pinned brackets              1.7078, 1.7175                           worst EXCESS 0.7175
#
# 1.10 sits ~2.1x above the worst honest excess and refuses the pinned ones by ~7x. That gap is what
# makes it a separator and not a tuned filter; if an honest bracket ever lands near it, widen only
# with the measurement that justifies it, and record the new observation in this table.
MAX_FM_DRIFT = 1.10
# Worst per-arm iowait an A/B/B/A may carry and still quote a ratio. See the refusal in `main`.
MAX_ARM_IOWAIT_PCT = 5.0

# The fleet's hard disk brake. A timed run writes corpus and record artifacts, and a host that runs
# out mid-invocation produces a half-written record that looks like a measurement.
MIN_FREE_GIB = 42.0


def free_gib(path: str = "/data") -> float | None:
    """Space actually available to this user, or `None` if the path cannot be stat'd.

    ⚠️ `f_bavail`, NOT `shutil.disk_usage().free`. The latter counts the root-reserved blocks an
    unprivileged writer cannot touch: measured on this host once at 126.7 GiB free while `df`
    reported 20G, which is the difference between "plenty" and "about to fail". The two agree when
    the reserve is not in play, so the bug is invisible until exactly the moment it matters.
    """
    try:
        stat = os.statvfs(path)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return stat.f_bavail * stat.f_frsize / (1024**3)


AGENT_MAIL_PROJECTS = os.path.expanduser("~/.mcp_agent_mail_git_mailbox_repo/projects")


def live_build_slots(root: str = AGENT_MAIL_PROJECTS, now: str | None = None) -> list[dict] | None:
    """Every unexpired exclusive build slot across ALL projects, or `None` if the store is absent.

    MEASUREMENTS ARE ONE-AT-A-TIME FLEET-WIDE, and this is what makes that checkable rather than
    remembered. Nine projects benchmarking simultaneously took this host to run queue 122, and every
    ratio measured in that window was a contention artifact -- indistinguishable, from inside a
    single invocation, from a real result.

    Scans EVERY project, not just this one. A slot held by frankenlibc contends for the same 64 cpus
    as one held here, so a per-project check would enforce nothing about the fleet.

    Returns `None` when the store does not exist, which is deliberately different from returning an
    empty list: absent means "cannot enforce", empty means "verified nobody holds one". The caller
    treats them differently -- refusing on the strength of a store you cannot read would brick the
    harness on any host without agent-mail.
    """
    if not os.path.isdir(root):
        return None
    now_ts = now or _utc_now_iso()
    slots: list[dict] = []
    for project in sorted(os.listdir(root)):
        slots_dir = os.path.join(root, project, "build_slots")
        if not os.path.isdir(slots_dir):
            continue
        for dirpath, _dirnames, filenames in os.walk(slots_dir):
            for filename in filenames:
                if not filename.endswith(".json"):
                    continue
                try:
                    with open(os.path.join(dirpath, filename), encoding="utf-8") as handle:
                        record = json.load(handle)
                except (OSError, ValueError):
                    continue
                expires = str(record.get("expires_ts") or "")
                if record.get("exclusive") and expires and expires > now_ts:
                    slots.append({**record, "project": project})
    return slots


def _utc_now_iso() -> str:
    """Current UTC instant in the store's timestamp format, for string comparison.

    Compared as STRINGS on purpose. The stored stamps carry nanosecond precision
    (`...388083894+00:00`) which `datetime.fromisoformat` rejects, and every stamp in the store is
    UTC with the same `+00:00` suffix and zero-padded fields, so lexical order IS chronological
    order for this set. Parsing them would add a failure mode to a comparison that does not need one.
    """
    import time

    return time.strftime("%Y-%m-%dT%H:%M:%S", time.gmtime()) + ".000000000+00:00"


# EVERY CHILD GETS A DEADLINE. A measurement harness that blocks forever on a wedged child is worse
# than one that fails: it holds the machine, produces nothing, and looks like a long run. The fleet
# has already lost 3h38m to an unbounded wait elsewhere, and until now every subprocess.run here was
# unbounded -- a hung node or a hung fm binary would have hung the invocation with no upper bound.
#
# Bounds are per-KIND, sized to the work rather than one global number: a pin probe that takes
# milliseconds and a measured arm that legitimately runs minutes need different deadlines, and a
# single generous timeout would make the fast ones useless as guards.
PROBE_TIMEOUT_S = 120.0
ARM_TIMEOUT_S = 1800.0


def run_bounded(argv: list[str], timeout: float, what: str) -> subprocess.CompletedProcess | None:
    """Run a child with a deadline, and on expiry REPORT WHAT IT MANAGED TO SAY.

    Returns `None` when the child was killed by the deadline. The partial stdout/stderr is printed
    rather than discarded: a child that wedged after emitting a diagnostic is telling you why, and
    `TimeoutExpired` carries that output. Throwing it away turns a diagnosable hang into a bare
    timeout message, which is how a wait failure becomes an afternoon.
    """
    try:
        return subprocess.run(
            argv, capture_output=True, text=True, check=False, timeout=timeout
        )
    except subprocess.TimeoutExpired as expired:
        print(f"REFUSING TO MEASURE: {what} exceeded its {timeout:.0f}s deadline and was killed")
        for stream_name, blob in (("stdout", expired.stdout), ("stderr", expired.stderr)):
            text = blob.decode("utf-8", "replace") if isinstance(blob, bytes) else (blob or "")
            tail = text.strip().splitlines()[-5:]
            if tail:
                print(f"  last {stream_name} from the child before it died:")
                for line in tail:
                    print(f"    {line}")
        return None


def loadavg() -> list[float]:
    with open("/proc/loadavg", encoding="utf-8") as handle:
        return [float(x) for x in handle.read().split()[:3]]


def cpu_mhz() -> dict | None:
    """Observed per-core clock right now, not the governor's policy limits.

    The policy limits are what an environment block usually records, and they cannot distinguish an
    arm that ran at 1.4 GHz from one that ran at 4.3 GHz. Both are reachable within a single run.
    """
    vals = []
    for entry in os.listdir("/sys/devices/system/cpu"):
        if not re.fullmatch(r"cpu\d+", entry):
            continue
        path = f"/sys/devices/system/cpu/{entry}/cpufreq/scaling_cur_freq"
        try:
            with open(path, encoding="utf-8") as handle:
                vals.append(int(handle.read()) // 1000)
        except OSError:
            pass
    if not vals:
        return None
    lo, hi = min(vals), max(vals)
    return {
        "min_mhz": lo,
        "max_mhz": hi,
        "mean_mhz": round(sum(vals) / len(vals)),
        "spread": round(hi / max(lo, 1), 3),
        "cores": len(vals),
    }


def proc_stat() -> dict:
    """Cumulative CPU jiffies plus the instantaneous D-state count, from ONE read of `/proc/stat`.

    Cumulative rather than a rate on purpose: each arm already captures conditions before and after
    itself, so the DIFFERENCE across those two reads is the iowait accrued during that arm and
    nothing else. A spot rate (`mpstat 1 1`) would cost a second per capture and would still sample
    a moment rather than the arm.

    `procs_blocked` is the kernel's own count of tasks in uninterruptible sleep — the D-state number
    — and it comes free from the same read.
    """
    iowait = total = blocked = running = 0
    with open("/proc/stat", encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("cpu "):
                fields = [int(x) for x in line.split()[1:]]
                total = sum(fields)
                iowait = fields[4] if len(fields) > 4 else 0
            elif line.startswith("procs_blocked"):
                blocked = int(line.split()[1])
            elif line.startswith("procs_running"):
                running = int(line.split()[1])
    return {
        "iowait_jiffies": iowait,
        "total_jiffies": total,
        "procs_blocked": blocked,
        "procs_running": running,
    }


def proc_cpu_snapshot() -> dict[int, tuple[str, int]]:
    """`pid -> (comm, utime+stime jiffies)` for every readable process.

    THE FLEET SLOT CANNOT SEE AN UNSLOTTED PEER, which is the gap this fills. The slot check refuses
    when another agent DECLARES a benchmark; it is blind when one simply runs. Observed on this host:
    run queue 109 with a frankenpandas python at 3195% CPU and a frankenscipy one at 778%, neither
    holding a slot. A row measured in that window is a contention artifact, and nothing in the row
    would have said so afterwards.

    Captured before and after each arm so the DIFFERENCE attributes CPU time to the interval the arm
    actually ran in, rather than sampling whatever happened to be scheduled at one instant.
    """
    snapshot: dict[int, tuple[str, int]] = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            with open(f"/proc/{entry}/stat", encoding="utf-8") as handle:
                fields = handle.read().rsplit(") ", 1)
            comm = fields[0].split("(", 1)[1] if "(" in fields[0] else "?"
            rest = fields[1].split()
            # `man 5 proc`: after the trailing ')' the fields are state, ppid, ... and utime/stime
            # are the 12th and 13th of that remainder.
            snapshot[int(entry)] = (comm, int(rest[11]) + int(rest[12]))
        except (OSError, ValueError, IndexError):
            continue
    return snapshot


def top_cpu_during(arm: dict, limit: int = 3) -> str:
    """The heaviest CPU consumers during this arm, in cores, excluding this process tree.

    Reported in CORES rather than percent-of-one-cpu because "3195%" needs dividing by 100 before it
    means anything on a 64-core box, and the number that matters is how much of the machine was gone.
    """
    before = (arm.get("before") or {}).get("procs")
    after = (arm.get("after") or {}).get("procs")
    total_before = ((arm.get("before") or {}).get("stat") or {}).get("total_jiffies")
    total_after = ((arm.get("after") or {}).get("stat") or {}).get("total_jiffies")
    if not before or not after or total_before is None or total_after is None:
        return "top=n/a"
    total_delta = total_after - total_before
    if total_delta <= 0:
        return "top=n/a"
    cores = len(os.sched_getaffinity(0)) or 1
    per_core = total_delta / cores
    mine = {os.getpid(), os.getppid()}
    deltas = []
    for pid, (comm, ticks) in after.items():
        if pid in mine or pid not in before:
            continue
        used = ticks - before[pid][1]
        if used > 0 and per_core > 0:
            deltas.append((used / per_core, comm))
    deltas.sort(reverse=True)
    if not deltas:
        return "top=none"

    # ⚠️ THE TOP N UNDERSELLS A SWARM, and this line exists because it did. Observed 2026-08-19:
    # run queue 79-86 on a 64-cpu host while the top five processes accounted for ~7 cores between
    # them — the load was many small processes, not one hog, and a reader seeing only the top five
    # would have concluded the machine was nearly free. The total and the count are what separate
    # "one peer benchmark is holding 44 cores" from "eighty things are each holding one".
    total_cores = sum(cores_used for cores_used, _ in deltas)
    busy_procs = sum(1 for cores_used, _ in deltas if cores_used >= 0.25)
    head = ",".join(f"{comm}:{cores_used:.1f}c" for cores_used, comm in deltas[:limit])
    return f"top={head} (total {total_cores:.1f}c across {busy_procs} procs over 0.25c)"


def arm_iowait_pct(arm: dict) -> float | None:
    """Percentage of CPU time the host spent in iowait DURING this arm.

    Returns `None` when either capture is missing, so an older record decides as it always did
    rather than being retroactively refused by a field it never carried.
    """
    before = (arm.get("before") or {}).get("stat")
    after = (arm.get("after") or {}).get("stat")
    if not before or not after:
        return None
    delta_total = after["total_jiffies"] - before["total_jiffies"]
    if delta_total <= 0:
        return None
    return 100.0 * (after["iowait_jiffies"] - before["iowait_jiffies"]) / delta_total


def io_note(arm: dict) -> str:
    """Per-arm iowait and D-state count, formatted for the arm's printed row.

    Printed on EVERY arm, not only on refusal: a row banked from a clean window has to be able to
    show it was clean, and 'iowait was fine' is not something a reader can verify after the fact.
    """
    pct = arm_iowait_pct(arm)
    before_stat = (arm.get("before") or {}).get("stat") or {}
    after_stat = (arm.get("after") or {}).get("stat") or {}
    blocked = after_stat.get("procs_blocked")
    # RUN QUEUE, reported as the pair straddling the arm rather than one number. `procs_running` is
    # an INSTANT, not an average, so a single reading beside a 100 ms arm describes a moment; two
    # readings at least bound it, and a wide pair is itself the signal that the host was moving.
    #
    # ⚠️ RECORDED, NOT GATED, and the distinction is deliberate. A threshold needs a calibrated
    # separation between runq values that spoil a measurement and ones that do not, and I do not
    # have one: the iowait ceiling could be justified because clean windows read 0.00% and a
    # saturated host read 53%, an order of magnitude apart. For runq I have observations (47 while
    # the sample sweep spread 51; 12-20 busy cpus on windows that produced clean brackets) but no
    # paired runq-versus-drift evidence, and inventing a bound would be the elastic-threshold
    # mistake bd-ecjg already documents. Record it now so the calibration becomes possible.
    runq = f"{before_stat.get('procs_running')}/{after_stat.get('procs_running')}"
    if pct is None:
        return f"iowait=n/a runq={runq}"
    return f"iowait={pct:.2f}% blocked={blocked} runq={runq}"


def conditions() -> dict:
    return {"loadavg": loadavg(), "mhz": cpu_mhz(), "stat": proc_stat(), "procs": proc_cpu_snapshot()}


def pick_pins(size: int = 8) -> dict | None:
    """Cores for both arms, chosen by the SAME rule run.mjs uses.

    Delegates to `pick_pins.mjs`, which imports `cpu_selection.mjs`, rather than reimplementing the
    choice here. Two implementations of "which core" is precisely how the arms ended up under
    different clock regimes (bd-hmfi): ours pinned to the 1429 MHz floor while the incumbent ran
    unpinned on boosted cores.
    """
    try:
        out = run_bounded(["node", PICK_PINS, str(size)], PROBE_TIMEOUT_S, "the pin selector")
        if out is None:
            return None
        return json.loads(out.stdout.strip().splitlines()[-1])
    except (OSError, ValueError, IndexError):
        return None


def _records(stdout: str, case_id: str):
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            record = json.loads(line)
        except ValueError:
            continue
        if record.get("id") == case_id:
            yield record


def fm_arm(fm_bin: str, corpus: str, case_id: str, pins: dict | None = None) -> dict:
    """FrankenMermaid full pipeline, with the counted work proof read from the same record."""
    before = conditions()
    argv = [fm_bin, corpus]
    if pins and pins.get("fm_cpu") is not None:
        argv = ["taskset", "-c", str(pins["fm_cpu"]), *argv]
    proc = run_bounded(argv, ARM_TIMEOUT_S, "the fm arm")
    after = conditions()
    if proc is None:
        # Same shape an unparseable record yields, so the caller's existing "INCOMPLETE" path
        # reports it rather than this raising through the middle of a bracket.
        return {"ns": None, "work": None, "before": before, "after": after, "code": None}
    ns = None
    work = None
    for record in _records(proc.stdout, case_id):
        ns = (record.get("pipeline_ns") or {}).get("p50")
        work = {
            "bytes": record.get("svg_bytes") or record.get("output_bytes"),
            "batch": record.get("batch"),
            "revisions": record.get("revisions"),
        }
    return {"ns": ns, "work": work, "before": before, "after": after, "code": proc.returncode}


def mermaid_arm(case_id: str, reps: int, pins: dict | None = None) -> dict:
    """Pinned mermaid-js render through chromium via CDP -- the same boundary as the fm arm."""
    before = conditions()
    argv = ["node", BENCH, "--only", case_id, "--reps", str(reps)]
    if pins and pins.get("incumbent_cpus"):
        # A cpuset, not a single core: Chromium is multi-process, and starving it would slow the
        # INCUMBENT and inflate our ratio -- an over-claim in our own favour.
        argv = ["taskset", "-c", ",".join(str(c) for c in pins["incumbent_cpus"]), *argv]
    proc = run_bounded(argv, ARM_TIMEOUT_S, "the mermaid arm")
    after = conditions()
    if proc is None:
        return {
            "ns": None,
            "null": None,
            "null_ratios": [],
            "before": before,
            "after": after,
            "code": None,
        }
    ns = None
    null_ci = None
    null_ratios: list[float] = []
    for record in _records(proc.stdout, case_id):
        if record.get("status") != "ok":
            continue
        # render_ns, NOT parse_ns: the fm arm above is a full pipeline, so the incumbent must be
        # measured at the same boundary or the ratio compares two different quantities.
        ns = (record.get("render_ns") or {}).get("p50")
        null = record.get("null_control") or {}
        # `n` IS THE SAMPLE COUNT THE NULL WAS ACTUALLY COMPUTED FROM, and it was missing here,
        # which cost a conclusion. A 4x-reps experiment was run to test whether the incumbent null
        # bias shrinks with more samples; `nullReps` derives from `reps`
        # (mermaid_bench.mjs:1532) so it SHOULD have risen -- but a budget clamp at :1664 can reset
        # it to MIN_NULL_ROUNDS = 9, and without `n` in the row there is no way to tell whether the
        # experiment raised the count or silently ran the floor. The refuted-hypothesis row in
        # PERF_LEDGER.md carries that caveat because of this omission.
        null_ci = (
            null.get("median"),
            null.get("ci95_lo"),
            null.get("ci95_hi"),
            null.get("n"),
        )
        # The RAW per-round ratios, kept alongside the summary. A summary cannot be re-analysed;
        # these can, and the reps question is answered by subsampling them rather than by running
        # twice in two windows that this host has not offered in weeks.
        null_ratios = null.get("ratios") or []
    return {
        "ns": ns,
        "null": null_ci,
        "null_ratios": null_ratios,
        "before": before,
        "after": after,
        "code": proc.returncode,
    }


def null_reps_report(ratios: list[float], label: str) -> list[str]:
    """Does the incumbent's null bias shrink with SAMPLE COUNT, from one run's own observations.

    THE QUESTION THIS SETTLES. The incumbent's A/A null has exceeded the 2% clause-3 bound in 3 of
    10 banked observations, which either means the incumbent genuinely does not repeat itself, or
    means a 9-round median is too noisy an estimator and more rounds would settle it. Those two have
    opposite consequences -- the first blocks certification, the second is fixed by a config change
    -- and they were never separated because the test on offer was "run twice with different reps
    and compare", which needs two comparable windows.

    Subsampling one run's own rounds needs none. Every draw comes from the SAME window, process and
    ELF, so sample count is the only thing that differs, which is a cleaner control than two runs
    could ever be.

    HOW TO READ IT. Random subsets of size k, so `typical` is the bias a k-round run would have
    reported. If the bias is estimator noise it falls roughly as 1/sqrt(k) and `over2pct` collapses
    toward zero; if it is a real offset the median sits still while k rises and only the spread
    narrows around it.

    The prefix/suffix line is a CONFOUND CHECK, not decoration: random subsets deliberately destroy
    round order, so if early rounds differ from late ones (JIT warmup, thermal drift) the subsets
    average over a real trend and the ladder would understate. When those two halves disagree, the
    ladder is not trustworthy on its own and the row must say so.
    """
    n = len(ratios)
    if n < 8:
        return []
    lines = [f"  {label} null reps ladder from n={n} rounds (random subsets, seed fixed):"]
    rng = random.Random(20260819)
    draws = 400
    for k in sorted({k for k in (4, 9, 18, 27, 36, n) if k <= n}):
        biases = []
        for _ in range(draws):
            subset = rng.sample(ratios, k)
            biases.append(abs(statistics.median(subset) - 1.0) * 100)
        typical = statistics.median(biases)
        over = sum(1 for b in biases if b > 2.0) / draws * 100
        lines.append(f"    k={k:>3}  typical |bias| {typical:5.2f}%   over2pct {over:5.1f}%")
    half = n // 2
    first = (statistics.median(ratios[:half]) - 1.0) * 100
    last = (statistics.median(ratios[half:]) - 1.0) * 100
    drift = abs(first - last)
    verdict = "ORDER EFFECT — ladder understates" if drift > 1.0 else "no order effect"
    lines.append(
        f"    first-half bias {first:+.2f}%  last-half {last:+.2f}%  ({verdict})"
    )
    return lines


def head_revision() -> str | None:
    """The checked-out revision, or None if this is not a git tree."""
    try:
        # Bounded like everything else here. `git rev-parse` is not a plausible hang, but an
        # unbounded call in a file whose point is that unbounded calls cost afternoons is an
        # invitation to copy it.
        #
        # The comment sits ABOVE the call rather than inside the arguments so the options stay
        # compact: the self-test looks for the bound within a few lines of the call, and a tight
        # window is a stronger check than one widened to accommodate prose.
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
            timeout=PROBE_TIMEOUT_S,
        )
        rev = out.stdout.strip()
        return rev if len(rev) == 40 and all(c in "0123456789abcdef" for c in rev) else None
    except OSError:
        return None


def check_elf_provenance(fm_bin: str, expected_rev: str | None) -> str | None:
    """Refuse a binary that was not built from the checked-out revision.

    THIS EXISTS BECAUSE ITS ABSENCE COST A CERTIFICATION. `run.mjs` enforces it -- "INVALID: benchmark
    ELF build revision must match both --fm-build-base and checked-out HEAD" -- and this script did
    not, so a row could be taken here with a binary built from a different revision and nothing would
    say so. I hit exactly that: a binary at rev 07768c81 measured against a tree at 6b78da29, and only
    the certified driver caught it.

    Checked by searching the ELF for the revision string the build embeds, which is the same evidence
    the provenance gate uses -- the rev must be IN the binary, not merely claimed beside it. An
    unreadable binary or a non-git tree returns a reason rather than passing silently: a check that
    cannot run must not report success.
    """
    if expected_rev is None:
        return "cannot determine HEAD revision, so provenance cannot be established"
    try:
        with open(fm_bin, "rb") as handle:
            blob = handle.read()
    except OSError as error:
        return f"cannot read {fm_bin}: {error}"
    if expected_rev.encode("ascii") not in blob:
        return (
            f"the binary does not embed HEAD {expected_rev[:8]}; it was built from a different "
            "revision, so any ratio it produces describes code that is not checked out"
        )
    return None


def fm_reps_expression(fm_reps: int | None) -> str:
    """The JS expression for the fm arm's rep count.

    Returns the corpus lookup `item.reps_rs` when nothing is overridden, so the un-overridden path
    reads its value from the corpus at run time and cannot drift from it by transcription. A
    literal is substituted only when the caller asked for one.

    Refuses a non-positive count rather than emitting `reps: 0`, which would time an arm that
    rendered nothing and report it as a very fast engine.
    """
    if fm_reps is None:
        return "item.reps_rs"
    if int(fm_reps) < 1:
        raise SystemExit(f"--fm-reps must be at least 1, got {fm_reps}")
    return str(int(fm_reps))


def build_corpus(case_id: str, dest: str, fm_reps: int | None = None) -> dict:
    """Generate the fm arm's corpus from `corpus.mjs` -- the SAME module the incumbent arm uses.

    THIS EXISTS BECAUSE A STALE CORPUS COST A MEASUREMENT INVOCATION, and it failed in the one way
    the campaign's input-divergence rule does not catch. I passed `.benchmarks/headtohead/corpus.json`
    -- the obvious in-repo candidate, and what this script's own usage line invites you to pass --
    and preflighted it the way the ledger says to: I hashed its `sequence_20` text against the live
    generator's and they were byte-IDENTICAL, sha 31c0dd6b. The check passed and the input was still
    unusable, because the CONTAINER schema had moved: the file was written when the field was `text`
    and the binary now requires `texts`. The arm produced `ns=None`, the work proof refused to quote a
    ratio, and the whole A/B/B/A was spent finding that out.

    So the fix is not another check on a supplied file; it is to stop supplying one. Generating here
    means the two arms cannot consume different bytes OR different shapes, by construction rather
    than by assertion -- the failure mode the ledger records as "two harnesses can fail to share an
    INPUT, which is worse than disagreeing, because no null sees it".

    Returns the item's provenance so the caller can print the input sha on the row. A row that cites
    the input it measured can be re-derived; one that does not, cannot.
    """
    # The fm arm's rep count is the CORPUS value unless overridden. Substituted as a JS EXPRESSION
    # rather than a literal, so the un-overridden path still evaluates `item.reps_rs` and cannot
    # drift from the corpus by transcription.
    reps_expr = fm_reps_expression(fm_reps)

    # `import(%s)` with a json-quoted file:// URL, NOT `import('%s')` -- json.dumps supplies its own
    # quotes, and wrapping them again makes node resolve a package literally named `"`.
    script = (
        "import(%s).then(async m => {"
        "  const fs = await import('node:fs');"
        "  const item = m.CORPUS.find(i => i.id === %s);"
        "  if (!item) { console.error('no such corpus case'); process.exit(3); }"
        "  const gen = m.generate(item);"
        "  const texts = Array.isArray(gen) ? gen : (gen.texts ?? [gen]);"
        "  fs.writeFileSync(%s, JSON.stringify(["
        "    { id: item.id, texts, reps: %s, warmup: item.warmup_rs }"
        "  ]));"
        "  console.log(JSON.stringify({"
        "    id: item.id, revisions: texts.length,"
        "    sha256: texts.map(t => m.sha256(t)), bytes: texts.reduce((n, t) => n + t.length, 0),"
        "    reps: %s, warmup: item.warmup_rs, corpus_reps: item.reps_rs"
        "  }));"
        "});"
    # ORDER MUST MATCH THE PLACEHOLDERS IN SOURCE ORDER, and they are not in the order the fields
    # read: `writeFileSync(%s` (line 311) comes BEFORE the handoff's `reps: %s` (line 312). Passing
    # reps ahead of dest made node call writeFileSync(100, ...) -- writing to file descriptor 100 --
    # which fails as EBADF rather than as a type error, so nothing points at the real cause.
    ) % (
        json.dumps("file://" + CORPUS_MJS),
        json.dumps(case_id),
        json.dumps(dest),
        reps_expr,
        reps_expr,
    )
    out = run_bounded(["node", "-e", script], PROBE_TIMEOUT_S, "corpus generation")
    if out is None:
        sys.exit("corpus generation timed out; refusing to measure against an unknown input")
    if out.returncode != 0:
        raise SystemExit(f"corpus generation failed: {out.stderr.strip() or out.returncode}")
    return json.loads(out.stdout.strip().splitlines()[-1])


def check_work_proof(arm: dict) -> str | None:
    """Refuse to quote a timing the arm did not earn.

    A gate that only checks "did it produce a number" passes while the arm measures nothing. The two
    load-bearing checks here are properties of the WORK: bytes actually emitted, and a
    bytes-per-nanosecond rate below what one thread can physically sustain. The `revisions` check is
    only a malformed-record guard -- that field is the corpus item's revision count, so it says the
    record is well formed, NOT that the engine did anything.
    """
    work = arm.get("work") or {}
    ns = arm.get("ns")
    if not ns:
        return "no timing"
    revisions = work.get("revisions")
    if not revisions:
        return f"revisions={revisions!r} -- malformed record, the corpus item claims no revisions"
    written = work.get("bytes")
    if not written:
        return f"bytes={written!r} -- nothing was emitted"
    rate = written / ns
    if rate > MAX_BYTES_PER_NS:
        return f"{rate:.1f} bytes/ns exceeds {MAX_BYTES_PER_NS} -- a memo hit, not real work"
    return None


def check_drift_control(fm_vals: list[int], arms: list[dict]) -> str | None:
    """Refuse to quote a ratio our own arm could not reproduce.

    A margin is only as trustworthy as the numerator's repeatability. When the two fm observations
    disagree by more than `MAX_FM_DRIFT`, the bracket is measuring the environment, not the engine,
    and the printed bound is arithmetic on noise.

    The calibrated batch is reported alongside because it names the usual cause without a second
    run: the harness records `batch` on every arm and nothing has ever read it. Single-core pinning
    drives it into the low 20s while unpinned brackets sit at 37-39, measured independently twice
    (bd-hmfi, bd-8557). A drifting bracket whose batch is in the low band is almost certainly the
    pinning artifact rather than a busy host.
    """
    if len(fm_vals) < 2:
        return None
    drift = max(fm_vals) / min(fm_vals)
    if drift <= MAX_FM_DRIFT:
        return None
    batches = [(arm.get("work") or {}).get("batch") for arm in arms]
    return (
        f"the fm arm drifted {drift:.4f}x between two runs of the same binary on the same input "
        f"inside this invocation, over the {MAX_FM_DRIFT}x ceiling -- calibrated batch {batches}"
    )


def self_test() -> int:
    """Prove the drift control separates the brackets that produced it.

    Every row below is a real bracket measured on this host, not a fixture invented to pass. A gate
    that has never been shown to REFUSE anything is indistinguishable from a gate that is never
    reached, which is the failure this whole file keeps rediscovering.
    """
    # (label, fm observations ns, batch, must_refuse)
    CASES = [
        # Pinned to one core -- the artifact this gate exists to catch. Measured twice, independently.
        ("pinned bd-8557", [93454, 159599], 22, True),
        ("pinned bd-hmfi", [156370, 91043], 20, True),
        # PINNED YET HEALTHY -- measured live on 2026-08-17 in a window the pin selector reported as
        # 6/64 cpus busy, fm on cpu50. Pinning is a BET ON ONE CORE, and it only loses when that core
        # is contended; unpinned, the scheduler migrates away from a competing tenant. This case is
        # here to stop the gate being "simplified" into "refuse if pinned": the gate must key on the
        # SYMPTOM (drift), which the batch tracks directly, not on the policy that sometimes causes it.
        ("pinned but healthy", [90443, 92758], 38, False),
        # Unpinned brackets in the same windows, same ELF and input.
        ("no-pin bd-8557", [88899, 89739], 39, False),
        ("no-pin bd-hmfi 1st", [96557, 94654], 38, False),
        ("no-pin bd-hmfi 2nd", [92143, 91060], 37, False),
        # The banked sequence_20 row's two brackets: the loosest HONEST drifts on record (1.0204x,
        # 1.0481x). If the ceiling ever refuses these, it has become a filter and not a separator.
        ("banked bracket A", [100000, 102040], 39, False),
        ("banked bracket B", [100000, 104810], 38, False),
    ]

    failures = 0
    for label, vals, batch, must_refuse in CASES:
        arms = [{"work": {"batch": batch}}, {"work": {"batch": batch}}]
        why = check_drift_control(vals, arms)
        refused = why is not None
        drift = max(vals) / min(vals)
        if refused != must_refuse:
            verb = "REFUSED" if refused else "ADMITTED"
            print(f"  FAIL {label}: drift {drift:.4f}x was {verb}, expected the opposite")
            failures += 1
        else:
            print(f"  ok   {label}: drift {drift:.4f}x {'refused' if refused else 'admitted'}")

    # A one-observation bracket cannot drift, and must not be refused for it -- absence of evidence
    # is not a failed check, it is an incomplete one handled elsewhere.
    if check_drift_control([88899], [{"work": {"batch": 39}}]) is not None:
        print("  FAIL a single observation was refused for drift it cannot have")
        failures += 1
    else:
        print("  ok   single observation not refused for drift it cannot have")

    # The refusal must NAME the batch, since that is what tells the operator to try --no-pin.
    why = check_drift_control([93454, 159599], [{"work": {"batch": 22}}, {"work": {"batch": 22}}])
    if "22" not in (why or ""):
        print("  FAIL the refusal does not report the calibrated batch")
        failures += 1
    else:
        print("  ok   the refusal reports the calibrated batch")

    # The fm-reps substitution decides how much work the timed arm does; a silent change here
    # changes every number the script produces.
    for value, expected in [(None, "item.reps_rs"), (1, "1"), (322_000, "322000")]:
        got = fm_reps_expression(value)
        ok = got == expected
        print(("  ok   " if ok else "  FAIL ") + f"fm_reps_expression({value!r}) -> {got!r}")
        failures += 0 if ok else 1

    for bad in (0, -1):
        try:
            fm_reps_expression(bad)
        except SystemExit:
            print(f"  ok   fm_reps_expression({bad}) refused")
        else:
            print(f"  FAIL fm_reps_expression({bad}) was accepted; a zero-rep arm renders nothing")
            failures += 1

    # Fleet-slot scanner, against a SYNTHETIC store. Testing the predicate rather than the refusal
    # path because the refusal sits behind the pin and ELF gates, so an end-to-end check would
    # exercise those instead and pass for the wrong reason.
    with tempfile.TemporaryDirectory() as root:
        def _write(project: str, name: str, record: dict) -> None:
            slot_dir = os.path.join(root, project, "build_slots", "host")
            os.makedirs(slot_dir, exist_ok=True)
            with open(os.path.join(slot_dir, f"{name}.json"), "w", encoding="utf-8") as handle:
                json.dump(record, handle)

        far_future = "2999-01-01T00:00:00.000000000+00:00"
        long_past = "2000-01-01T00:00:00.000000000+00:00"
        _write("proj-a", "peer", {"agent": "Peer", "slot": "s1", "exclusive": True, "expires_ts": far_future})
        _write("proj-b", "stale", {"agent": "Peer", "slot": "s2", "exclusive": True, "expires_ts": long_past})
        _write("proj-c", "shared", {"agent": "Peer", "slot": "s3", "exclusive": False, "expires_ts": far_future})
        _write("proj-d", "junk", {"agent": "Peer"})

        found = live_build_slots(root=root)
        cases = [
            ("an absent store is None, not empty", live_build_slots(root="/nonexistent") is None),
            ("a live exclusive slot is found", any(s["slot"] == "s1" for s in found or [])),
            ("an EXPIRED slot is ignored", not any(s["slot"] == "s2" for s in found or [])),
            ("a NON-exclusive slot is ignored", not any(s["slot"] == "s3" for s in found or [])),
            ("a record with no expiry is ignored", len(found or []) == 1),
            ("the project is recorded with the slot", (found or [{}])[0].get("project") == "proj-a"),
        ]
        for label, ok in cases:
            print(f"  {'ok  ' if ok else 'FAIL'} {label}")
            failures += 0 if ok else 1


    # Every child spawn in this harness pair must carry a deadline (see `run_bounded`). This scans
    # for the call shapes and requires a bound near each one.
    #
    # ⚠️ THE NEEDLES ARE ASSEMBLED, NOT WRITTEN, and that is not decoration. The include_str! gate
    # in gpu_plan.rs passed while checking nothing because it scanned a file containing its own
    # search terms; a checker whose source contains the pattern it hunts finds itself and reports
    # clean. Building them from fragments keeps this file free of the literals.
    py_call = "subprocess" + ".run("
    js_calls = ("spawn" + "Sync(", "execFile" + "Sync(")
    unbounded: list[str] = []
    for path, needles, bound in (
        (os.path.abspath(__file__), (py_call,), "timeout="),
        (os.path.join(os.path.dirname(os.path.abspath(__file__)), "run.mjs"), js_calls, "timeout:"),
    ):
        try:
            with open(path, encoding="utf-8") as handle:
                lines = handle.read().splitlines()
        except OSError:
            continue
        for index, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("#") or stripped.startswith("//"):
                continue
            if not any(needle in line for needle in needles):
                continue
            # The bound may sit on the call line or in the options that follow it.
            window = "\n".join(lines[index : index + 8])
            if bound not in window:
                unbounded.append(f"{os.path.basename(path)}:{index + 1}: {stripped[:70]}")

    print(f"  {'ok  ' if not unbounded else 'FAIL'} every child spawn carries a deadline")
    if unbounded:
        for site in unbounded:
            print(f"       unbounded: {site}")
        failures += len(unbounded)

    # THE REPS LADDER MUST TELL THE TWO HYPOTHESES APART, or it is a decoration that will be read
    # as evidence. Synthetic inputs with KNOWN answers: pure noise centred on 1.0 (where more
    # samples must shrink the reported bias) and a real 3% offset (where they must not). A ladder
    # that reported the same shape for both would let estimator noise be recorded as incumbent
    # instability, which is the exact conclusion this analysis exists to prevent.
    ladder_rng = random.Random(11)
    noise = [1.0 + ladder_rng.gauss(0, 0.04) for _ in range(40)]
    offset = [1.03 + ladder_rng.gauss(0, 0.004) for _ in range(40)]

    def typical_at(rows: list[str], k: int) -> float | None:
        for row in rows:
            if row.strip().startswith(f"k={k:>3}".strip()) and "typical" in row:
                return float(row.split("typical |bias|")[1].split("%")[0])
        return None

    noise_rows = null_reps_report(noise, "T")
    offset_rows = null_reps_report(offset, "T")
    noise_lo, noise_hi = typical_at(noise_rows, 4), typical_at(noise_rows, 36)
    offset_lo, offset_hi = typical_at(offset_rows, 4), typical_at(offset_rows, 36)
    if noise_lo is None or noise_hi is None or offset_lo is None or offset_hi is None:
        print("  FAIL reps ladder did not report the k values it was asked for")
        failures += 1
    elif not noise_hi < noise_lo / 2:
        print(f"  FAIL reps ladder: pure noise did not shrink ({noise_lo:.2f}% -> {noise_hi:.2f}%)")
        failures += 1
    elif abs(offset_hi - offset_lo) > 0.5:
        print(f"  FAIL reps ladder: a real offset moved ({offset_lo:.2f}% -> {offset_hi:.2f}%)")
        failures += 1
    else:
        print(
            f"  ok   reps ladder separates noise ({noise_lo:.2f}->{noise_hi:.2f}%) "
            f"from a real offset ({offset_lo:.2f}->{offset_hi:.2f}%)"
        )

    # Too few rounds to say anything must produce NOTHING, not a ladder built from 3 numbers.
    if null_reps_report([1.0, 1.01, 1.02], "T"):
        print("  FAIL reps ladder reported from a sample too small to subset")
        failures += 1
    else:
        print("  ok   reps ladder stays silent below the minimum sample")

    print("self-test PASSED" if not failures else f"self-test FAILED ({failures})")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    # NOT `required=True`: --self-test measures nothing and needs no binary, and a self-test that
    # can only run by passing a fake path is a self-test CI will not run. Absence is checked below,
    # after the self-test has had its chance to return.
    parser.add_argument("--fm-bin", help="content-pinned frankenmermaid h2h binary")
    parser.add_argument(
        "--corpus",
        help="corpus json for --fm-bin. OMIT IT: the default generates the case from corpus.mjs, "
        "the same module the incumbent arm uses, so the two arms cannot consume different bytes "
        "or a different schema. Pass a path only to measure an input you are pinning deliberately.",
    )
    parser.add_argument("--case", default="sequence_20", help="corpus case id (default sequence_20)")
    parser.add_argument("--reps", type=int, default=9, help="mermaid reps per arm")
    parser.add_argument(
        "--fm-reps",
        type=int,
        help="override the fm arm's reps (corpus reps_rs otherwise). COST: every rep runs one "
        "calibrated ~3 ms batch, so arm wall time is about reps x 3 ms -- 10,000 reps is a ~30 s "
        "arm and 100,000 is ~5 MINUTES per arm. It is NOT reps x the per-op median; that model is "
        "out by the batch factor and is how a 100,000-rep run came to be mistaken for a 9-second "
        "one. An overridden row states the override and is not comparable against a corpus-reps row.",
    )
    parser.add_argument(
        "--incumbent-cpus", type=int, default=8, help="cpuset size for the incumbent arm"
    )
    parser.add_argument(
        "--no-pin", action="store_true", help="run both arms unpinned (the pre-bd-hmfi behaviour)"
    )
    parser.add_argument(
        "--allow-starved-incumbent",
        action="store_true",
        help="quote a ratio even when the pin selector reports the incumbent starved; the row must "
        "then say so, because the bias runs in our favour",
    )
    parser.add_argument(
        "--self-test", action="store_true", help="check the gates against real measured brackets"
    )
    parser.add_argument(
        "--allow-drifting-arm",
        action="store_true",
        help="quote a ratio even when the fm bracket fails its own drift control; the row must then "
        "state the drift, because the margin is then arithmetic on noise",
    )
    parser.add_argument(
        "--top-consumers",
        type=float,
        metavar="SECONDS",
        help="print the heaviest CPU consumers over an interval and exit, measuring nothing else. "
        "Exists so window_check.sh can answer 'who is eating the machine' with THIS implementation "
        "rather than a second copy of it -- two /proc parsers would drift the moment one learned "
        "about a new field, and the answer would differ between the tool that refuses a window and "
        "the tool that records one",
    )
    parser.add_argument(
        "--allow-unslotted",
        action="store_true",
        help="measure without holding the fleet-wide build slot, or while another agent holds one; "
        "the row must then state it, because concurrent benchmarks on this host produced run queue "
        "122 and every ratio taken in that window was a contention artifact",
    )
    parser.add_argument(
        "--allow-io-saturation",
        action="store_true",
        help="quote a ratio even when an arm ran while the host was waiting on storage; the row must "
        "then state the per-arm iowait, because a disk-bound host does not slow both engines equally",
    )
    parser.add_argument(
        "--allow-stale-elf",
        action="store_true",
        help="measure a binary not built from HEAD; the row must then state which revision it was",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if args.top_consumers is not None:
        # Placed before every other gate: this mode measures nothing, needs no binary and no slot,
        # and its whole purpose is to be runnable on exactly the contended host where a measurement
        # is refused.
        import time

        before = conditions()
        time.sleep(max(args.top_consumers, 0.1))
        after = conditions()
        print(top_cpu_during({"before": before, "after": after}, limit=5))
        return 0

    if not args.fm_bin:
        parser.error("--fm-bin is required unless --self-test is given")

    print("=== A/B/B/A, one invocation, RENDER-scoped, UNCERTIFIED (no host-exclusivity gate) ===")
    rev = head_revision()
    provenance = check_elf_provenance(args.fm_bin, rev)
    if provenance is not None and not args.allow_stale_elf:
        print(f"REFUSING TO MEASURE: {provenance}")
        print("Rebuild with FM_H2H_BUILD_GIT_REV=$(git rev-parse HEAD), or pass --allow-stale-elf")
        print("if you are deliberately measuring an older revision and will say so in the row.")
        return 2
    if provenance is None:
        print(f"PROVENANCE: binary embeds HEAD {rev[:8]}")
    else:
        print(f"PROVENANCE OVERRIDDEN: {provenance}")
    if args.corpus:
        corpus_path = args.corpus
        print(f"CORPUS: supplied {corpus_path} -- its provenance is yours to state on the row")
    else:
        corpus_path = os.path.join(
            tempfile.gettempdir(), f"fm-abba-corpus-{os.getpid()}-{args.case}.json"
        )
        info = build_corpus(args.case, corpus_path, args.fm_reps)
        # The input sha belongs on the row: it is what makes the number re-derivable, and it is the
        # one field that proves both arms were fed the same document.
        print(
            f"CORPUS: generated {args.case} from corpus.mjs -- {info['revisions']} revision(s), "
            f"{info['bytes']} bytes, sha256 {', '.join(s[:16] for s in info['sha256'])}"
        )
        # An overridden rep count MUST appear on the row. Two rows measured at different rep counts
        # are not comparable, and the difference is otherwise invisible: same case, same input sha,
        # same binary, different amount of work. Printed whenever it differs from the corpus rather
        # than whenever the flag was passed, so `--fm-reps` set to the corpus value says nothing.
        corpus_reps = info.get("corpus_reps")
        if corpus_reps is not None and info.get("reps") != corpus_reps:
            print(
                f"FM REPS OVERRIDDEN: {info['reps']} against the corpus value {corpus_reps} -- "
                "this row measured a different amount of work and may not be compared against a "
                "row taken at the corpus reps"
            )

    pins = None if args.no_pin else pick_pins(args.incumbent_cpus)
    if pins is None:
        print("PINS: none -- both arms run unpinned (symmetric, but clocks uncontrolled)")
    else:
        print(
            f"PINS: fm cpu{pins['fm_cpu']} @ {pins['fm_mhz']} MHz ({pins['fm_rule']}); "
            f"incumbent {len(pins['incumbent_cpus'])} cpus {pins['incumbent_cpus']}, "
            f"slowest {pins['incumbent_min_mhz']} MHz, starved={pins['incumbent_starved']}; "
            f"host spread {pins['host_spread']}x, {pins['busy_cpus_over_20pct']}/{pins['total_cpus']} cpus busy"
        )
    # A STARVED INCUMBENT INFLATES THE RATIO IN OUR FAVOUR, and until now this script computed that
    # fact, printed it, and quoted the ratio anyway -- a guard that exists and is never read is the
    # same as no guard. `mermaid_arm` already explains why the incumbent gets a cpuset rather than one
    # core: "starving it would slow the INCUMBENT and inflate our ratio -- an over-claim in our own
    # favour". The selector detects exactly that condition, so refusing on it is enforcing a rule this
    # file already stated. Observed live: a run that returned 6 cpus for a requested 8 and flagged
    # starved=True still printed a 447.8x bound.
    #
    # Refusable rather than fatal, and the override stamps the row, because a gate with no escape is
    # how this campaign has repeatedly frozen itself.
    if pins and pins.get("incumbent_starved") and not args.allow_starved_incumbent:
        print()
        print("REFUSING TO QUOTE A RATIO: the pin selector reports the incumbent arm STARVED")
        print(
            f"  it received {len(pins['incumbent_cpus'])} cpus, and a starved incumbent runs slower, "
            "which inflates the ratio in our own favour"
        )
        print("  re-run in a window with more comparable idle cores, or pass")
        print("  --allow-starved-incumbent and state the starvation on the row")
        return 2

    # DISK BRAKE, checked before anything is written or timed. Observed 2026-08-19: free space fell
    # 227 -> 211 -> 190 -> 149 GiB inside about ten minutes while four peer builds ran, i.e. faster
    # than a single A/B/B/A invocation takes. A `df` taken when the run was queued is not evidence
    # about the run, which is why this is here and not in the operator's head.
    free = free_gib()
    if free is None:
        print("NOTE: could not stat /data, so the disk brake cannot be checked here")
    elif free < MIN_FREE_GIB:
        print()
        print(f"REFUSING TO MEASURE: {free:.1f} GiB free, under the {MIN_FREE_GIB} GiB brake")
        print("  a run that fills the disk mid-invocation leaves a half-written record, which is")
        print("  worse than no record: it looks like a measurement")
        return 2

    # FLEET SERIALISATION (one timed run at a time, across ALL projects).
    #
    # Nine projects benchmarking at once took this host to run queue 122, and every ratio taken in
    # that window was a contention artifact — which from inside one invocation looks exactly like a
    # result. The A/A null does not save you either: both fm arms are degraded together, so drift
    # stays small while both numbers are wrong, which is the failure mode bd-ecjg documents.
    #
    # Verified against the agent-mail store rather than asserted by a flag: a self-declared "I hold
    # the slot" is worth nothing, and the records carry agent, exclusivity and expiry.
    slots = live_build_slots()
    if slots is None:
        print("NOTE: agent-mail store not found, so fleet serialisation cannot be verified here")
    else:
        me = os.environ.get("AGENT_NAME", "")
        mine = [s for s in slots if s.get("agent") == me]
        others = [s for s in slots if s.get("agent") != me]
        if others and not args.allow_unslotted:
            print()
            print("REFUSING TO MEASURE: another agent holds an exclusive build slot")
            for slot in others[:5]:
                print(f"  {slot.get('agent')} holds {slot.get('project')}/{slot.get('slot')} until {slot.get('expires_ts')}")
            print("  measurements are ONE AT A TIME FLEET-WIDE; wait for release, or pass")
            print("  --allow-unslotted and state the contention on the row")
            return 2
        if not mine and not args.allow_unslotted:
            print()
            print("REFUSING TO MEASURE: this agent holds no build slot")
            print("  ⚠️ IF acquire_build_slot ANSWERS 'Build slots are disabled. Enable")
            print("     WORKTREES_ENABLED', this refusal is UNSATISFIABLE on this host: the slot")
            print("     cannot be taken at all. Observed 2026-08-19, and it is also why peers were")
            print("     measuring unslotted rather than ignoring the rule. In that state the honest")
            print("     options are source work, or --allow-unslotted with the row recording that")
            print("     serialisation was UNAVAILABLE rather than skipped -- the per-arm `top=`")
            print("     consumers being the only evidence of what else ran that survives afterwards.")
            print(f"  acquire_build_slot first (AGENT_NAME={me or 'unset'}), then re-run")
            print("  or pass --allow-unslotted and state it on the row")
            return 2
        held = ", ".join(f"{s.get('project')}/{s.get('slot')}" for s in mine) or "none"
        if not mine:
            # Reached only via --allow-unslotted. The old wording here was
            # "FLEET SLOT: held by BlackThrush -- none", which reads as a slot report and is worse
            # than silence: a row banked from it would look serialised. Say the actual state, so the
            # line copied onto a ledger row says what it means.
            print(
                "FLEET SLOT: NOT HELD -- serialisation BYPASSED via --allow-unslotted. "
                "Record on the row whether it was unavailable (acquire_build_slot disabled) or "
                "merely skipped; the per-arm `top=` consumers are the substitute evidence."
            )
        else:
            print(f"FLEET SLOT: held by {me or 'unknown'} -- {held}; no other agent holds one")

    a1 = fm_arm(args.fm_bin, corpus_path, args.case, pins)
    print(f"A1 fm      ns={a1['ns']} work={a1['work']} load={a1['before']['loadavg']} {io_note(a1)} {top_cpu_during(a1)} mhz={a1['before']['mhz']}")
    b1 = mermaid_arm(args.case, args.reps, pins)
    print(f"B1 mermaid ns={b1['ns']} load={b1['before']['loadavg']} {io_note(b1)} {top_cpu_during(b1)} mhz={b1['before']['mhz']}")
    b2 = mermaid_arm(args.case, args.reps, pins)
    print(f"B2 mermaid ns={b2['ns']} load={b2['before']['loadavg']} {io_note(b2)} {top_cpu_during(b2)} mhz={b2['before']['mhz']}")
    a2 = fm_arm(args.fm_bin, corpus_path, args.case, pins)
    print(f"A2 fm      ns={a2['ns']} work={a2['work']} load={a2['before']['loadavg']} {io_note(a2)} {top_cpu_during(a2)} mhz={a2['before']['mhz']}")

    # IO SATURATION REFUSAL. A disk-bound host does not slow the two engines equally -- the arm that
    # happens to straddle a flush pays for it -- so a timing taken while the machine is waiting on
    # storage compares queue depth, not code. Observed on this host: 0.00% iowait through every
    # clean window all session, then 53% with 37 tasks in D-state. 5% is an order of magnitude above
    # the clean observation and an order below the saturated one, so it is not knife-edge.
    #
    # Measured as a DELTA across each arm's own before/after captures, which is the iowait accrued
    # during that arm rather than a spot sample taken next to it. An arm with no captures returns
    # None and decides as before, so this cannot retroactively refuse an older record.
    io_offenders = [
        (name, pct)
        for name, arm in (("A1", a1), ("B1", b1), ("B2", b2), ("A2", a2))
        for pct in [arm_iowait_pct(arm)]
        if pct is not None and pct > MAX_ARM_IOWAIT_PCT
    ]
    if io_offenders and not args.allow_io_saturation:
        print()
        print("REFUSING TO QUOTE A RATIO: the host was waiting on storage during a measured arm")
        for name, pct in io_offenders:
            print(f"  {name} ran at {pct:.1f}% iowait, over the {MAX_ARM_IOWAIT_PCT}% ceiling")
        print("  a disk-bound host does not slow both engines equally, so this compares queue depth")
        print("  re-run in a quiet window, or pass --allow-io-saturation and state it on the row")
        return 2

    for name, arm in (("A1", a1), ("A2", a2)):
        why = check_work_proof(arm)
        if why is not None:
            print(f"\nREFUSING TO QUOTE A RATIO: fm arm {name} failed its work proof -- {why}")
            return 2

    fm_vals = [arm["ns"] for arm in (a1, a2) if arm["ns"]]
    mj_vals = [arm["ns"] for arm in (b1, b2) if arm["ns"]]
    if not fm_vals or not mj_vals:
        print(f"\nINCOMPLETE fm={fm_vals} mjs={mj_vals}")
        return 2

    print()
    print(f"fm  observations ns: {fm_vals}  drift {max(fm_vals) / min(fm_vals):.4f}x")
    print(f"mjs observations ns: {mj_vals}")

    # Gate BEFORE the bounds are printed. A refusal that still prints the number it is refusing gets
    # quoted anyway -- that is how the 228.1x bound escaped into a doc.
    why = check_drift_control(fm_vals, [a1, a2])
    if why is not None and not args.allow_drifting_arm:
        print()
        print(f"REFUSING TO QUOTE A RATIO: {why}")
        print("  a bracket whose numerator cannot reproduce itself is measuring the environment")
        print("  if the batch is in the low 20s, re-run with --no-pin before blaming the host")
        print("  or pass --allow-drifting-arm and state the drift on the row")
        return 2
    if why is not None:
        print(f"\nDRIFT OVERRIDDEN: {why}")
    # Worst bound: slower fm against faster mermaid.
    print(f"WORST-BOUND RATIO: {min(mj_vals) / max(fm_vals):.1f}x")
    print(f"headline (median/median): {statistics.median(mj_vals) / statistics.median(fm_vals):.1f}x")
    # Printed as (median, lo, hi, n). The n is what makes a null comparable ACROSS runs: a median
    # from 9 pairs and one from 60 are different measurements wearing the same shape, and the row
    # cannot say which it was unless the count travels with it.
    print(f"mermaid A/A null (median, lo, hi, n): {b1['null']}  {b2['null']}")
    for label, arm in (("B1", b1), ("B2", b2)):
        null = arm.get("null") or ()
        if len(null) == 4 and null[3] is not None and null[0] is not None:
            bias_pct = (null[0] - 1.0) * 100
            note = " ⚠️ over the 2% clause-3 bound" if abs(bias_pct) > 2.0 else ""
            print(f"  {label} null bias {bias_pct:+.2f}% from n={null[3]} pairs{note}")
    for label, arm in (("B1", b1), ("B2", b2)):
        for line in null_reps_report(arm.get("null_ratios") or [], label):
            print(line)
    print(f"conditions at end: load={loadavg()} mhz={cpu_mhz()}")
    print()
    print("PROVISIONAL. Quote the worst bound, cite the executing ELF sha, and record per-arm loadavg")
    print("and CPU MHz -- a cross-core spread near 3x is why this is a bound and not an estimate.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
