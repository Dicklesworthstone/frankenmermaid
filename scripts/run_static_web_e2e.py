#!/usr/bin/env python3
"""Headless Chromium smoke/E2E runner for hosted showcase surfaces."""

from __future__ import annotations

import argparse
import functools
import hashlib
import html
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import threading
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Iterable
from urllib.parse import urlencode

sys.path.insert(0, str(Path(__file__).resolve().parent))
import showcase_harness


@dataclass(frozen=True)
class Scenario:
    scenario_id: str
    query: dict[str, str]
    required_substrings: tuple[str, ...]
    pass_reason: str


@dataclass(frozen=True)
class RunProfile:
    profile_id: str
    chromium_flags: tuple[str, ...]
    expected_substrings: tuple[str, ...] = ()


SCENARIOS = (
    Scenario(
        scenario_id="static-web-compare-export",
        query={
            "sample": "flowchart-1-incident-response-escalation",
            "spotlight": "flowchart-1-incident-response-escalation",
            "compare": "flowchart-3-change-approval-pipeline,sequence-1-checkout-risk-review",
            "lab": "cycles",
        },
        required_substrings=(
            "Comparing",
            "Built 4 reproducible artifact record",
            "Config and state",
            "layoutCycleLab()",
        ),
        pass_reason="Static /web host restored compare state, rendered the artifact lab, and populated the layout lab.",
    ),
    Scenario(
        scenario_id="static-web-diagnostics-recovery",
        query={
            "sample": "flowchart-1-incident-response-escalation",
            "source": "flowchart LR\nA-->",
            "lab": "overview",
        },
        required_substrings=(
            "Current revision is degraded and no prior healthy snapshot has been committed yet.",
            "highest severity",
            "parse failed",
            "No safe fallback preview is cached yet.",
        ),
        pass_reason="Static /web host surfaced diagnostics and degraded fallback messaging for a malformed source override.",
    ),
    Scenario(
        scenario_id="static-web-determinism-check",
        query={
            "sample": "flowchart-1-incident-response-escalation",
            "det": "5",
        },
        required_substrings=(
            "Determinism check passed:",
            "determinismCheck()",
        ),
        pass_reason="Static /web host reran the in-app determinism checker and surfaced normalized hash evidence for the current revision.",
    ),
    Scenario(
        scenario_id="static-web-presenter-tour",
        query={
            "tour": "main-demo",
            "tour_step": "1",
        },
        required_substrings=(
            "Presenter mode",
            "Step 2 of 5: Shared-engine spotlight",
            "Move to the flagship flowchart",
        ),
        pass_reason="Static /web host restored the guided presenter tour at a concrete step without manual UI setup.",
    ),
)

PROFILES = (
    RunProfile(profile_id="desktop-default", chromium_flags=()),
    RunProfile(
        profile_id="desktop-reduced-motion",
        chromium_flags=("--force-prefers-reduced-motion",),
    ),
    RunProfile(
        profile_id="mobile-narrow",
        chromium_flags=("--window-size=390,844",),
        expected_substrings=("Section navigation",),
    ),
)


def select_scenarios(selected_ids: list[str] | None) -> tuple[Scenario, ...]:
    if not selected_ids:
        return SCENARIOS
    selected = tuple(scenario for scenario in SCENARIOS if scenario.scenario_id in set(selected_ids))
    missing = sorted(set(selected_ids) - {scenario.scenario_id for scenario in selected})
    if missing:
        raise ValueError(f"unknown scenario ids: {', '.join(missing)}")
    return selected


def select_profiles(selected_ids: list[str] | None) -> tuple[RunProfile, ...]:
    if not selected_ids:
        return PROFILES
    selected = tuple(profile for profile in PROFILES if profile.profile_id in set(selected_ids))
    missing = sorted(set(selected_ids) - {profile.profile_id for profile in selected})
    if missing:
        raise ValueError(f"unknown profile ids: {', '.join(missing)}")
    return selected


class SilentHandler(SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return


def sha256_text(value: str) -> str:
    return f"sha256:{hashlib.sha256(value.encode('utf-8')).hexdigest()}"


def timestamp_utc() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H-%M-%SZ")


def build_url(base_url: str, route_prefix: str, query: dict[str, str]) -> str:
    encoded = urlencode(query)
    return f"{base_url}{route_prefix}?{encoded}" if encoded else f"{base_url}{route_prefix}"


def count_diagnostic_items(dom: str) -> int:
    return dom.count('class="diagnostic-item"')


def derive_degradation_tier(dom: str) -> str:
    if "Current revision is degraded" in dom or "No safe fallback preview is cached yet." in dom:
        return "fallback"
    if "artifact missing" in dom or "Runtime unavailable on GitHub right now" in dom:
        return "unavailable"
    if "warning" in dom:
        return "partial"
    return "healthy"


def derive_runtime_mode(dom: str) -> str:
    if "live runtime" in dom:
        return "live"
    if "artifact missing" in dom:
        return "artifact-missing"
    return "mock-forbidden"


def extract_determinism_report(dom: str) -> dict[str, object] | None:
    match = re.search(r'<pre id="determinism-json"[^>]*>(.*?)</pre>', dom, re.DOTALL)
    if not match:
        return None
    payload = html.unescape(match.group(1)).strip()
    if not payload or payload.startswith("Determinism evidence JSON will appear here"):
        return None
    return json.loads(payload)


def ensure_contains(dom: str, required_substrings: Iterable[str]) -> list[str]:
    return [item for item in required_substrings if item not in dom]


def dump_dom(chromium_path: str, url: str, timeout_seconds: int, profile: RunProfile) -> str:
    with tempfile.TemporaryDirectory() as temp_root:
        runtime_dir = Path(temp_root) / "runtime"
        profile_dir = Path(temp_root) / "profile"
        runtime_dir.mkdir(mode=0o700)
        profile_dir.mkdir(mode=0o700)
        cmd = [
            chromium_path,
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            f"--user-data-dir={profile_dir}",
            "--run-all-compositor-stages-before-draw",
            f"--virtual-time-budget={timeout_seconds * 1000}",
            "--dump-dom",
            url,
        ]
        cmd[5:5] = list(profile.chromium_flags)
        env = os.environ.copy()
        env["XDG_RUNTIME_DIR"] = str(runtime_dir)
        result = subprocess.run(cmd, capture_output=True, text=True, check=False, env=env)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "chromium --dump-dom failed")
    return result.stdout


def start_http_server(root: Path) -> tuple[ThreadingHTTPServer, str]:
    handler = functools.partial(SilentHandler, directory=str(root))
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    server = ThreadingHTTPServer(("127.0.0.1", port), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, f"http://127.0.0.1:{port}"


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def build_log(
    *,
    bead_id: str,
    scenario: Scenario,
    profile: RunProfile,
    run_index: int,
    url: str,
    dom: str,
    revision: str,
    script_hash: str,
    output_hash: str,
    surface: str,
    host_kind: str,
    determinism_report: dict[str, object] | None = None,
) -> dict[str, object]:
    first_run = (determinism_report or {}).get("runs", [{}])[0] if determinism_report else {}
    summary = determinism_report.get("summary") if determinism_report else scenario.pass_reason
    return {
        "schema_version": 1,
        "bead_id": bead_id,
        "scenario_id": scenario.scenario_id,
        "input_hash": sha256_text(url),
        "surface": surface,
        "renderer": "franken-svg",
        "theme": "corporate",
        "config_hash": script_hash,
        "parse_ms": int(first_run.get("parseMs", 0) or 0),
        "layout_ms": int(first_run.get("layoutMs", 0) or 0),
        "render_ms": int(first_run.get("renderMs", 0) or 0),
        "diagnostic_count": int(first_run.get("diagnosticCount", count_diagnostic_items(dom)) or 0),
        "degradation_tier": str(first_run.get("degradationTier", derive_degradation_tier(dom))),
        "output_artifact_hash": str(first_run.get("outputArtifactHash", output_hash)),
        "pass_fail_reason": str(summary),
        "run_kind": "e2e",
        "trace_id": f"{bead_id}-{scenario.scenario_id}-{profile.profile_id}-run{run_index}",
        "revision": revision,
        "host_kind": host_kind,
        "fallback_active": "Current revision is degraded" in dom,
        "runtime_mode": derive_runtime_mode(dom),
        "profile": profile.profile_id,
        "run_index": run_index,
        "determinism_status": "stable" if determinism_report and determinism_report.get("stable") else "unreported",
    }


def normalized_log_signature(log: dict[str, object]) -> dict[str, object]:
    return {
        "scenario_id": log["scenario_id"],
        "surface": log["surface"],
        "renderer": log["renderer"],
        "theme": log["theme"],
        "diagnostic_count": log["diagnostic_count"],
        "degradation_tier": log["degradation_tier"],
        "runtime_mode": log["runtime_mode"],
        "fallback_active": log["fallback_active"],
        "profile": log.get("profile"),
        "determinism_status": log.get("determinism_status"),
    }


def build_replay_command(
    *,
    bead_id: str,
    repo_root: str,
    output_root: str,
    chromium: str,
    timeout_seconds: int,
    route_prefix: str,
    surface: str,
    host_kind: str,
    scenario_prefix: str,
    repeat: int,
    revision: str | None = None,
    scenario_id: str | None = None,
    profile_id: str | None = None,
) -> list[str]:
    command = [
        "python3",
        "scripts/run_static_web_e2e.py",
        "--bead-id",
        bead_id,
        "--repo-root",
        repo_root,
        "--output-root",
        output_root,
        "--chromium",
        chromium,
        "--timeout-seconds",
        str(timeout_seconds),
        "--repeat",
        str(repeat),
        "--route-prefix",
        route_prefix,
        "--surface",
        surface,
        "--host-kind",
        host_kind,
        "--scenario-prefix",
        scenario_prefix,
    ]
    if revision:
        command.extend(["--revision", revision])
    if scenario_id:
        command.extend(["--scenario-id", scenario_id])
    if profile_id:
        command.extend(["--profile-id", profile_id])
    return command


def write_replay_bundle(
    *,
    bundle_dir: Path,
    bead_id: str,
    repo_root: Path,
    output_root: Path,
    chromium: str,
    timeout_seconds: int,
    route_prefix: str,
    surface: str,
    host_kind: str,
    scenario_prefix: str,
    revision: str,
    repeat: int,
    scenarios: tuple[Scenario, ...],
    profiles: tuple[RunProfile, ...],
    summary_path: Path,
    trace_index: list[dict[str, object]] | None = None,
) -> dict[str, str]:
    bundle_dir.mkdir(parents=True, exist_ok=True)
    suite_command = build_replay_command(
        bead_id=bead_id,
        repo_root=str(repo_root),
        output_root=str(output_root),
        chromium=chromium,
        timeout_seconds=timeout_seconds,
        route_prefix=route_prefix,
        surface=surface,
        host_kind=host_kind,
        scenario_prefix=scenario_prefix,
        repeat=repeat,
        revision=revision,
    )
    scenario_commands = []
    for scenario in scenarios:
        for profile in profiles:
            matching_traces = []
            if trace_index:
                matching_traces = [
                    item["trace_id"]
                    for item in trace_index
                    if item.get("scenario_id") == scenario.scenario_id.replace("static-web", scenario_prefix, 1)
                    and item.get("profile") == profile.profile_id
                ]
            scenario_commands.append(
                {
                    "scenario_id": scenario.scenario_id.replace("static-web", scenario_prefix, 1),
                    "profile": profile.profile_id,
                    "trace_ids": matching_traces,
                    "command": build_replay_command(
                        bead_id=bead_id,
                        repo_root=str(repo_root),
                        output_root=str(output_root),
                        chromium=chromium,
                        timeout_seconds=timeout_seconds,
                        route_prefix=route_prefix,
                        surface=surface,
                        host_kind=host_kind,
                        scenario_prefix=scenario_prefix,
                        repeat=1,
                        revision=revision,
                        scenario_id=scenario.scenario_id,
                        profile_id=profile.profile_id,
                    ),
                }
            )
    manifest = {
        "schema_version": 1,
        "bead_id": bead_id,
        "surface": surface,
        "host_kind": host_kind,
        "route_prefix": route_prefix,
        "repeat": repeat,
        "revision": revision,
        "summary_path": str(summary_path),
        "suite_command": suite_command,
        "trace_index": trace_index or [],
        "scenario_commands": scenario_commands,
    }
    manifest_path = bundle_dir / "replay_manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")

    script_lines = [
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        "",
        "repo_root=$(cd \"$(dirname \"$0\")/../../../../..\" && pwd)",
        "cd \"$repo_root\"",
        "",
        "# Rerun the full React release-grade suite",
        subprocess.list2cmdline(suite_command),
        "",
        "# Replay a single case by uncommenting one command below",
    ]
    for item in scenario_commands:
        script_lines.append(f"# {item['scenario_id']} / {item['profile']}")
        script_lines.append(f"# {subprocess.list2cmdline(item['command'])}")
    script_path = bundle_dir / "replay_suite.sh"
    script_path.write_text("\n".join(script_lines) + "\n")
    script_path.chmod(0o755)

    return {"manifest_path": str(manifest_path), "script_path": str(script_path)}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run hosted showcase E2E flows")
    parser.add_argument("--bead-id", default="bd-2u0.5.8.2.3")
    parser.add_argument("--repo-root", default=".", help="Repository root to serve")
    parser.add_argument("--output-root", default="evidence/runs/web/bd-2u0.5.8.2.3")
    parser.add_argument(
        "--chromium",
        default=os.environ.get("CHROMIUM_BIN", "/snap/chromium/3390/usr/lib/chromium-browser/chrome"),
    )
    parser.add_argument("--timeout-seconds", type=int, default=8)
    parser.add_argument("--revision", default=None)
    parser.add_argument("--repeat", type=int, default=5)
    parser.add_argument("--route-prefix", default="/web", help="Hosted route prefix to open")
    parser.add_argument(
        "--surface",
        default="web",
        choices=sorted(showcase_harness.ALLOWED_SURFACES - {"standalone", "cli", "wasm", "terminal"}),
    )
    parser.add_argument(
        "--host-kind",
        default="static-web",
        choices=sorted(showcase_harness.ALLOWED_HOST_KINDS - {"standalone", "cli", "test-harness"}),
    )
    parser.add_argument(
        "--scenario-prefix",
        default="static-web",
        help="Prefix used when translating shared scenario names to a host-specific surface id",
    )
    parser.add_argument(
        "--scenario-id",
        action="append",
        help="Optional base scenario id filter (repeat to select multiple)",
    )
    parser.add_argument(
        "--profile-id",
        action="append",
        help="Optional profile id filter (repeat to select multiple)",
    )
    parser.add_argument(
        "--replay-bundle-dir",
        help="Optional directory where replay manifest and shell helpers should be emitted",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()
    output_root = (repo_root / args.output_root).resolve()
    scenarios = select_scenarios(args.scenario_id)
    profiles = select_profiles(args.profile_id)
    revision = args.revision or subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=repo_root, capture_output=True, text=True, check=True
    ).stdout.strip()
    script_hash = showcase_harness.sha256_file(Path(__file__).resolve())

    server, base_url = start_http_server(repo_root)
    summary: dict[str, object] = {
        "ok": True,
        "base_url": base_url,
        "route_prefix": args.route_prefix,
        "surface": args.surface,
        "host_kind": args.host_kind,
        "repeat": args.repeat,
        "profiles": [profile.profile_id for profile in profiles],
        "scenarios": [scenario.scenario_id.replace("static-web", args.scenario_prefix, 1) for scenario in scenarios],
        "results": [],
        "determinism": [],
        "differential": [],
        "trace_index": [],
    }
    determinism_groups: dict[tuple[str, str], list[dict[str, object]]] = defaultdict(list)

    try:
        for scenario in scenarios:
            for profile in profiles:
                for run_index in range(1, args.repeat + 1):
                    url = build_url(base_url, args.route_prefix, scenario.query)
                    dom = dump_dom(args.chromium, url, args.timeout_seconds, profile)
                    missing = ensure_contains(dom, (*scenario.required_substrings, *profile.expected_substrings))
                    if missing:
                        raise RuntimeError(
                            f"{scenario.scenario_id}/{profile.profile_id}/run{run_index} missing expected DOM content: {missing}"
                        )

                    timestamp = timestamp_utc()
                    scenario_id = scenario.scenario_id.replace("static-web", args.scenario_prefix, 1)
                    scenario_dir = output_root / scenario_id / profile.profile_id
                    html_path = scenario_dir / f"{timestamp}__e2e__html.html"
                    write_text(html_path, dom)
                    output_hash = sha256_text(dom)
                    determinism_report = extract_determinism_report(dom)
                    log = build_log(
                        bead_id=args.bead_id,
                        scenario=Scenario(
                            scenario_id=scenario_id,
                            query=scenario.query,
                            required_substrings=scenario.required_substrings,
                            pass_reason=scenario.pass_reason.replace("Static /web host", f"{args.route_prefix} host"),
                        ),
                        profile=profile,
                        run_index=run_index,
                        url=url,
                        dom=dom,
                        revision=revision,
                        script_hash=script_hash,
                        output_hash=output_hash,
                        surface=args.surface,
                        host_kind=args.host_kind,
                        determinism_report=determinism_report,
                    )
                    errors = showcase_harness.validate_log_payload(log)
                    if errors:
                        raise RuntimeError(f"{scenario.scenario_id} generated invalid log: {errors}")
                    log_path = scenario_dir / f"{timestamp}__e2e__log.json"
                    write_text(log_path, json.dumps(log, indent=2) + "\n")
                    result_record = {
                        "scenario_id": scenario_id,
                        "profile": profile.profile_id,
                        "run_index": run_index,
                        "url": url,
                        "html_path": str(html_path.relative_to(repo_root)),
                        "log_path": str(log_path.relative_to(repo_root)),
                        "diagnostic_count": log["diagnostic_count"],
                        "degradation_tier": log["degradation_tier"],
                        "runtime_mode": log["runtime_mode"],
                        "output_artifact_hash": log["output_artifact_hash"],
                        "determinism_status": log.get("determinism_status"),
                        "trace_id": log["trace_id"],
                    }
                    summary["trace_index"].append(
                        {
                            "scenario_id": scenario_id,
                            "profile": profile.profile_id,
                            "run_index": run_index,
                            "trace_id": log["trace_id"],
                            "log_path": result_record["log_path"],
                        }
                    )
                    differential = showcase_harness.extract_differential_report(dom)
                    if scenario.scenario_id == "static-web-compare-export":
                        result_record["differential"] = differential
                        summary["differential"].append(
                            {
                                "scenario_id": scenario_id,
                                "profile": profile.profile_id,
                                "run_index": run_index,
                                **differential,
                            }
                        )
                    summary["results"].append(result_record)
                    determinism_groups[(scenario_id, profile.profile_id)].append(
                        {
                            "output_artifact_hash": log["output_artifact_hash"],
                            "normalized_log": normalized_log_signature(log),
                            "log_path": result_record["log_path"],
                        }
                    )

        for (scenario_id, profile_id), runs in determinism_groups.items():
            hashes = [run["output_artifact_hash"] for run in runs]
            signatures = [json.dumps(run["normalized_log"], sort_keys=True) for run in runs]
            summary["determinism"].append(
                {
                    "scenario_id": scenario_id,
                    "profile": profile_id,
                    "runs": len(runs),
                    "stable_output_hash": len(set(hashes)) == 1,
                    "stable_normalized_log": len(set(signatures)) == 1,
                    "output_hashes": hashes,
                }
            )
    finally:
        server.shutdown()
        server.server_close()

    summary_path = output_root / f"{timestamp_utc()}__determinism__summary.json"
    write_text(summary_path, json.dumps(summary, indent=2) + "\n")
    if args.replay_bundle_dir:
        summary["replay_bundle"] = write_replay_bundle(
            bundle_dir=(repo_root / args.replay_bundle_dir).resolve(),
            bead_id=args.bead_id,
            repo_root=repo_root,
            output_root=output_root,
            chromium=args.chromium,
            timeout_seconds=args.timeout_seconds,
            route_prefix=args.route_prefix,
            surface=args.surface,
            host_kind=args.host_kind,
            scenario_prefix=args.scenario_prefix,
            revision=revision,
            repeat=args.repeat,
            scenarios=scenarios,
            profiles=profiles,
            summary_path=summary_path,
            trace_index=summary["trace_index"],
        )
        write_text(summary_path, json.dumps(summary, indent=2) + "\n")

    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
