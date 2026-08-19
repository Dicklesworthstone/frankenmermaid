#!/usr/bin/env python3
"""Ordered verification plan for code committed during the build freeze.

STATUS AS OF 2026-08-18, so nobody re-runs this believing nothing is verified
-----------------------------------------------------------------------------
COMPILATION IS DONE. `cargo check --workspace --all-targets` exits 0 with no errors and no
warnings, so every crate of freeze-era code compiles. What remains below is test EXECUTION, which
is a different question and still open for most crates.

Already RUN and passing:
  * fm-core --lib                       428 passed, 0 failed
  * fm-parser --lib                     my three detection tests passed (in a sibling's run)
  * fm-parser --test parse_lens_laws    6 passed -- the lens obeys GetPut, PutGet, PutPut and
                                        complement preservation, which nothing had ever asserted

Two defects were found by that first compile, both mine, both fixed:
  * cga.rs carried a duplicate `#[must_use]` because inserting a function above `to_rotor` stranded
    its doc AND its attribute onto mine. rustc is phasing that into a hard error and CI runs
    -D warnings, so it would have failed the gate -- while rustfmt parsed it happily all freeze.
  * abba_render passed format arguments in FIELD order when `writeFileSync(%s` precedes the
    handoff's `reps: %s`, so node wrote to file descriptor 100 and died with EBADF.

WHY THIS EXISTS
---------------
A long freeze produced a backlog of commits that rustfmt has parsed and no compiler has seen. The
risk is not that they are wrong -- it is that the first person to build will hit several unrelated
failures at once, in crates they did not touch, and will not know which are EXPECTED.

Two of them are expected by design and must not be "fixed":

  * `renderer_agreement.rs` keeps a gantt_section KNOWN_GAPS entry marked EXPECTED TO GO STALE. That
    list is checked in BOTH directions, so the first build should fail with "now agrees - delete its
    KNOWN_GAPS entry". That failure is the intended signal that the terminal fix works.
  * Determinism suites assert properties nothing has ever checked. A failure there is a FINDING --
    the writer is emitting -0, or an ordering leaks -- not a test to relax.

DISK AND BUILD DISCIPLINE ARE ENFORCED, NOT DOCUMENTED
------------------------------------------------------
The freeze exists because /data ran to 99%. This refuses to start a build below the floor, and runs
ONE build at a time, because four panes building simultaneously is what took the host to load 465
earlier in this campaign. `--execute` is opt-in; the default prints the plan and touches nothing.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The user's standing floor. Not a suggestion: below this, a build is what fills the volume.
MIN_FREE_GIB = 42

# Ordered so the cheapest, most localised checks run first. A failure in step 1 makes every later
# step's output untrustworthy, so stopping early is the point.
PLAN = [
    (
        "fm-core",
        ["cargo", "test", "-p", "fm-core", "--lib"],
        "reuse_proof (counted work classes) and the CGA conformal guard + characterization.",
    ),
    (
        "fm-parser",
        ["cargo", "test", "-p", "fm-parser"],
        "style-target warnings, and the ParseLens LAW suite -- get/put, put/get, put/put, and "
        "complement preservation. If a law fails, the lens is wrong, not the test.",
    ),
    (
        "fm-layout",
        ["cargo", "test", "-p", "fm-layout"],
        "infeasibility deletion filter, FP determinism faults, and the e-graph width gate pin.",
    ),
    (
        "fm-render-canvas",
        ["cargo", "test", "-p", "fm-render-canvas"],
        "gpu_plan (colours, dash, atlas, layouts), cluster styling, canvas determinism.",
    ),
    (
        "fm-render-svg",
        ["cargo", "test", "-p", "fm-render-svg"],
        "output determinism: byte stability, and NO -0 / NaN in serialised SVG.",
    ),
    (
        "fm-render-term",
        ["cargo", "test", "-p", "fm-render-term"],
        "terminal output determinism and the trimming/control-character guards.",
    ),
    (
        "fm-wasm",
        ["cargo", "test", "-p", "fm-wasm", "--lib"],
        "canvas target routing ladder and the chooseCanvasTarget wrapper.",
    ),
    (
        "fm-cli",
        ["cargo", "test", "-p", "fm-cli"],
        "layout FP determinism, and renderer_agreement -- EXPECT the gantt_section KNOWN_GAPS entry "
        "to fail as stale. Delete the entry; do not re-add the gap.",
    ),
    (
        "workspace-clippy",
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        "CI gates this. Run it LAST: it is the slowest, and every earlier failure would surface here "
        "as noise. Note it compiles HEAD plus any uncommitted peer WIP in the shared tree.",
    ),
    (
        "workspace-fmt",
        ["cargo", "fmt", "--check"],
        "CI gates this too. Files I touched were formatted individually; this is the whole-tree check.",
    ),
]

NON_CARGO = [
    (
        "wgsl-validation",
        "The four WGSL constants in gpu_plan.rs have never been seen by a shader toolchain. "
        "Validate NODE_SDF_WGSL, EDGE_WGSL, ARROWHEAD_WGSL and TEXT_ATLAS_WGSL with naga. The "
        "existing tests only check that @location numbers match the Rust structs; they cannot tell "
        "whether the WGSL is well-formed.",
    ),
    (
        "browser-worker",
        "web/fm-render.worker.js and web/playground.html have never been loaded. Export names were "
        "checked by grep and message shapes by reading serde attributes -- neither was run.",
    ),
]


def free_gib(path: str = "/data") -> float:
    """Space available to a NON-ROOT writer, matching what df reports.

    NOT shutil.disk_usage().free, which returns f_bfree -- free blocks INCLUDING the filesystem's
    root reserve. On this 1.9 TiB volume that reserve is ~95 GiB, so shutil reported 126.7 GiB free
    while df reported 20 GiB and the freeze was in force. A guard reading the wrong one would have
    cleared every build the freeze exists to stop, and would have looked correct doing it.

    f_bavail is the number a cargo build can actually consume.
    """
    stat = os.statvfs(path)
    return (stat.f_bavail * stat.f_frsize) / (1024**3)


def project_build_running() -> str | None:
    """Whether THIS project already has a cargo build in flight.

    NOT `pgrep -x cargo`. That matches the cargo binary by exact name, which has two failure modes
    and I hit both within one minute:

      * FALSE POSITIVE -- it matches cargo builds belonging to OTHER projects on this shared host,
        so it reports the slot taken when this project's slot is free.
      * FALSE NEGATIVE, the dangerous one -- a sibling pane running its build through a shell
        wrapper does not appear as a process named exactly `cargo`, so `-x` reports the slot FREE
        while this project is mid-build. Acting on that starts the second concurrent build the
        one-build-per-project rule exists to prevent.

    Matching the full command line against this repository's path answers the question actually
    being asked: is anyone building THIS project.
    """
    result = subprocess.run(
        ["pgrep", "-a", "-f", "cargo"], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        return None

    # Exclude OUR OWN process tree. The caller's shell command line usually contains both the repo
    # path and the word "cargo" -- the guard invocation itself, or the very build being authorised --
    # so a naive match reports the slot taken by the caller. Measured: this probe said SLOT TAKEN
    # while nothing but my own wrapper was running, which is a false positive that would stall work
    # forever rather than prevent a collision.
    mine = {os.getpid(), os.getppid()}
    try:
        mine.add(int(open(f"/proc/{os.getppid()}/stat", encoding="utf-8").read().split()[3]))
    except (OSError, ValueError, IndexError):
        pass

    for line in result.stdout.splitlines():
        pid_text, _, command = line.partition(" ")
        if not pid_text.isdigit() or int(pid_text) in mine:
            continue
        # Require an actual cargo BUILD subcommand, not merely the word "cargo" beside the repo
        # path. Without this the probe fires on any sibling shell whose command line happens to
        # mention cargo -- a grep, a log tail, or a finished wrapper -- and reports a slot that
        # nothing is using. Measured: it flagged a zsh wrapper while no cargo process for this
        # project existed at all.
        if "pgrep" in command or REPO not in command:
            continue
        if any(
            f"cargo {sub}" in command
            for sub in ("test", "check", "build", "bench", "clippy", "run", "rustc")
        ):
            return line.strip()
    return None


def guard_build_slot() -> None:
    holder = project_build_running()
    if holder:
        raise SystemExit(
            "REFUSING TO BUILD: this project already has a build in flight.\n"
            f"  {holder[:160]}\n"
            "One build per project. Wait for it rather than starting a second."
        )


def guard_disk() -> None:
    free = free_gib()
    if free < MIN_FREE_GIB:
        raise SystemExit(
            f"REFUSING TO BUILD: {free:.1f} GiB free, floor is {MIN_FREE_GIB} GiB.\n"
            "The freeze exists because this volume ran to 99%; a build here is what fills it."
        )


def show(execute: bool) -> int:
    print("=== post-freeze verification plan ===")
    print(f"free space: {free_gib():.1f} GiB (floor {MIN_FREE_GIB} GiB)\n")

    for index, (name, argv, why) in enumerate(PLAN, start=1):
        print(f"{index:2d}. {name}\n    {' '.join(argv)}\n    {why}\n")

    print("NOT CARGO, and not covered by any step above:")
    for name, why in NON_CARGO:
        print(f"  - {name}: {why}")

    if not execute:
        print("\nDry run: nothing was executed. Pass --execute to run, one step at a time.")
        return 0

    guard_disk()
    guard_build_slot()
    for index, (name, argv, _why) in enumerate(PLAN, start=1):
        # Re-checked before EVERY step, not once at the start: earlier steps write target artifacts,
        # and this campaign has watched the volume fall 6 GiB inside one turn.
        guard_disk()
        guard_build_slot()
        print(f"\n--- [{index}/{len(PLAN)}] {name}: {' '.join(argv)}")
        result = subprocess.run(argv, cwd=REPO, check=False, capture_output=True, text=True)
        output = (result.stdout or "") + (result.stderr or "")

        # ⚠️ EXIT STATUS ALONE CANNOT TELL A PASS FROM A RUN THAT NEVER HAPPENED, which is fatal for
        # a script whose whole job is verification. `cargo` here is an rch shim, and its admission
        # refusal was MEASURED (2026-08-18, by another agent on this repo) to report:
        #
        #     foregrounded: exit 103    backgrounded: exit 0, with a 154-byte log
        #
        # So "103 means admission refusal", which this project had written down, is not sufficient:
        # under the background wrapper a refusal is indistinguishable from a pass on status alone.
        # This loop would have printed "All steps passed" having compiled and run nothing.
        #
        # Judged on POSITIVE EVIDENCE instead, in this order: a refusal is recognised first because
        # it can carry exit 0; then the step must prove it ran; only then does the status decide.
        verdict = classify_step_output(output, result.returncode)
        if verdict == "refusal":
            print(output)
            print(
                f"\nSTOPPED at {name}: rch ADMISSION REFUSAL, not a test result (exit "
                f"{result.returncode} — a refusal reports 0 when backgrounded). Nothing was "
                "verified. Retry when workers are admissible, or re-run with "
                "RCH_CARGO_WRAPPER_BYPASS=1 to build locally."
            )
            return 103

        if verdict == "no-evidence":
            print(output)
            print(
                f"\nSTOPPED at {name}: the step exited {result.returncode} but produced NO "
                "`test result:` line, so it did not run a test suite. Treat this as a broken "
                "invocation, never as a pass — that is the whole reason this check exists."
            )
            return result.returncode or 1

        for line in output.splitlines():
            if line.startswith("test result:") or " FAILED" in line:
                print(line)

        if result.returncode != 0:
            print(output)
            print(
                f"\nSTOPPED at {name} (exit {result.returncode}). Later steps are not run: a failure "
                "here makes their output untrustworthy. Check the expected-failure notes above "
                "before treating this as a regression."
            )
            return result.returncode
    print("\nAll steps passed.")
    return 0


def classify_step_output(output: str, returncode: int) -> str:
    """Judge one verification step on EVIDENCE first and exit status last.

    Returns one of `refusal`, `no-evidence`, `failed`, `passed`.

    Extracted from the runner so `--self-test` can pin the ORDERING: a refusal carrying exit 0 must
    not read as a pass, and an inline `if` cannot be tested without running cargo.
    """
    if "refusing local fallback" in output:
        return "refusal"
    if "test result:" not in output:
        return "no-evidence"
    if returncode != 0:
        return "failed"
    return "passed"


def self_test() -> int:
    failures = 0

    ok = all(argv[0] == "cargo" for _n, argv, _w in PLAN)
    print(("  ok   " if ok else "  FAIL ") + "every planned step is a cargo invocation")
    failures += 0 if ok else 1

    # The plan must cover every crate that received unbuilt code. A crate missing from here is a
    # crate nobody compiles after the freeze, which is the whole failure this script prevents.
    covered = {name for name, _a, _w in PLAN}
    for crate in [
        "fm-core",
        "fm-parser",
        "fm-layout",
        "fm-render-canvas",
        "fm-render-svg",
        "fm-render-term",
        "fm-wasm",
        "fm-cli",
    ]:
        ok = crate in covered
        print(("  ok   " if ok else "  FAIL ") + f"{crate} is in the plan")
        failures += 0 if ok else 1

    # The probe must agree with df, not with shutil. This is the check that would have caught my
    # own first version, which read the root-reserved figure and was six times too generous.
    probe = free_gib()
    df_out = subprocess.run(
        ["df", "-B1", "--output=avail", "/data"], capture_output=True, text=True, check=False
    )
    df_avail = None
    if df_out.returncode == 0:
        digits = [line.strip() for line in df_out.stdout.splitlines() if line.strip().isdigit()]
        if digits:
            df_avail = int(digits[-1]) / (1024**3)

    if df_avail is None:
        print(f"  SKIP df comparison unavailable; probe reports {probe:.1f} GiB")
    else:
        ok = abs(probe - df_avail) < 1.0
        print(
            ("  ok   " if ok else "  FAIL ")
            + f"probe {probe:.1f} GiB agrees with df {df_avail:.1f} GiB"
        )
        failures += 0 if ok else 1

    # The floor must be the stated one. A silently lowered floor turns this from a guard into a
    # rubber stamp, and the failure mode is a filled volume rather than a wrong number.
    ok = MIN_FREE_GIB == 42
    print(("  ok   " if ok else "  FAIL ") + f"the disk floor is the standing 42 GiB (is {MIN_FREE_GIB})")
    failures += 0 if ok else 1

    # The slot probe must not be `pgrep -x cargo`. That is the check that reported the slot FREE
    # while a sibling was mid-build behind a shell wrapper, and reported it TAKEN because of other
    # projects entirely.
    import inspect

    source = inspect.getsource(project_build_running)
    ok = '"-f"' in source and REPO in str(REPO) and "-x" not in source.split('"""')[-1]
    print(("  ok   " if ok else "  FAIL ") + "the slot probe matches full command lines, not the binary name")
    failures += 0 if ok else 1

    # A BACKGROUNDED rch refusal reports exit 0 with a ~154-byte log; the SAME refusal foregrounded
    # reports 103 (measured on this repo, 2026-08-18). So exit status alone cannot separate a pass
    # from a run that never happened, and this runner judges on evidence first.
    refusal = "[RCH] remote required; refusing local fallback (no admissible workers) — retryable"
    for label, got, want in (
        ("a refusal with exit 0 is not a pass", classify_step_output(refusal, 0), "refusal"),
        ("a refusal with exit 103 is a refusal", classify_step_output(refusal, 103), "refusal"),
        ("silence with exit 0 is not a pass", classify_step_output("", 0), "no-evidence"),
        (
            "a real suite that passed is a pass",
            classify_step_output("test result: ok. 12 passed; 0 failed", 0),
            "passed",
        ),
        (
            "a real suite that failed is a failure",
            classify_step_output("test result: FAILED. 1 passed; 1 failed", 101),
            "failed",
        ),
    ):
        ok = got == want
        print(("  ok   " if ok else "  FAIL ") + f"{label} (got {got})")
        failures += 0 if ok else 1

    print("self-test PASSED" if not failures else f"self-test FAILED ({failures})")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--self-test", action="store_true", help="check the plan's own invariants")
    parser.add_argument("--execute", action="store_true", help="actually run the steps, one at a time")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    return show(args.execute)


if __name__ == "__main__":
    raise SystemExit(main())
