#!/usr/bin/env python3
"""Ordered verification plan for code committed during the build freeze.

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
    for line in result.stdout.splitlines():
        if REPO in line and "pgrep" not in line:
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
        result = subprocess.run(argv, cwd=REPO, check=False)
        if result.returncode != 0:
            print(
                f"\nSTOPPED at {name} (exit {result.returncode}). Later steps are not run: a failure "
                "here makes their output untrustworthy. Check the expected-failure notes above "
                "before treating this as a regression."
            )
            return result.returncode
    print("\nAll steps passed.")
    return 0


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
