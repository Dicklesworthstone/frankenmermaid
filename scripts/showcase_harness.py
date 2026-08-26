#!/usr/bin/env python3
"""Reusable validation harness for showcase artifacts and host adapters."""

from __future__ import annotations

import argparse
import html
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path


REQUIRED_LOG_FIELDS = {
    "schema_version",
    "bead_id",
    "scenario_id",
    "input_hash",
    "surface",
    "renderer",
    "theme",
    "config_hash",
    "parse_ms",
    "layout_ms",
    "render_ms",
    "diagnostic_count",
    "degradation_tier",
    "output_artifact_hash",
    "pass_fail_reason",
    "run_kind",
    "trace_id",
    "revision",
    "host_kind",
    "fallback_active",
    "runtime_mode",
}

ALLOWED_SURFACES = {"standalone", "web", "web_react", "cli", "wasm", "terminal"}
ALLOWED_RENDERERS = {"franken-svg", "mermaid-baseline", "canvas", "term", "cli"}
ALLOWED_RUN_KINDS = {"unit", "integration", "e2e", "determinism", "evidence"}
ALLOWED_HOST_KINDS = {"standalone", "static-web", "react-web", "cli", "test-harness"}
ALLOWED_DEGRADATION_TIERS = {"healthy", "partial", "fallback", "unavailable"}
ALLOWED_RUNTIME_MODES = {"live", "artifact-missing", "fallback-only", "mock-forbidden"}
PARITY_REQUIRED_HOST_KINDS = ("static-web", "react-web")
PARITY_STRICT_FIELDS = (
    "renderer",
    "theme",
    "diagnostic_count",
    "degradation_tier",
    "runtime_mode",
    "fallback_active",
    "determinism_status",
)
PARITY_ACCEPTABLE_DELTA_FIELDS = (
    "surface",
    "host_kind",
    "output_artifact_hash",
    "pass_fail_reason",
    "parse_ms",
    "layout_ms",
    "render_ms",
    "input_hash",
    "config_hash",
    "revision",
    "trace_id",
)

REVALIDATING_CACHE_CONTROL = "public, max-age=0, must-revalidate"
EVIDENCE_CACHE_CONTROL = "public, max-age=3600, must-revalidate"
IMMUTABLE_CACHE_CONTROL = "public, max-age=31536000, immutable"


class HtmlSmokeParser(HTMLParser):
    """Minimal parser wrapper so malformed HTML raises via feed/close usage."""


@dataclass
class CheckResult:
    name: str
    ok: bool
    detail: str

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "ok": self.ok, "detail": self.detail}


def read_json_file(path: Path, *, label: str | None = None) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        prefix = f"{label} " if label else ""
        raise RuntimeError(f"{prefix}JSON parse failed for {path}") from exc


def parse_headers_manifest(text: str) -> dict[str, dict[str, str]]:
    rules: dict[str, dict[str, str]] = {}
    current_rule: str | None = None

    for raw_line in text.splitlines():
        if not raw_line.strip():
            continue

        if raw_line.startswith((" ", "\t")):
            if current_rule is None:
                raise RuntimeError("header line appeared before any route rule")
            name, separator, value = raw_line.strip().partition(":")
            if separator != ":":
                raise RuntimeError(f"invalid header line: {raw_line.strip()}")
            rules[current_rule][name.strip().lower()] = value.strip()
            continue

        current_rule = raw_line.strip()
        rules[current_rule] = {}

    return rules


def has_cache_rule(
    rules: dict[str, dict[str, str]],
    route: str,
    expected_cache_control: str,
) -> bool:
    return rules.get(route, {}).get("cache-control") == expected_cache_control


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def validate_log_payload(payload: dict[str, object]) -> list[str]:
    errors: list[str] = []
    missing = sorted(REQUIRED_LOG_FIELDS - payload.keys())
    if missing:
        errors.append(f"missing required fields: {', '.join(missing)}")

    if payload.get("surface") not in ALLOWED_SURFACES:
        errors.append(f"invalid surface: {payload.get('surface')}")
    if payload.get("renderer") not in ALLOWED_RENDERERS:
        errors.append(f"invalid renderer: {payload.get('renderer')}")
    if payload.get("run_kind") not in ALLOWED_RUN_KINDS:
        errors.append(f"invalid run_kind: {payload.get('run_kind')}")
    if payload.get("host_kind") not in ALLOWED_HOST_KINDS:
        errors.append(f"invalid host_kind: {payload.get('host_kind')}")
    if payload.get("degradation_tier") not in ALLOWED_DEGRADATION_TIERS:
        errors.append(f"invalid degradation_tier: {payload.get('degradation_tier')}")
    if payload.get("runtime_mode") not in ALLOWED_RUNTIME_MODES:
        errors.append(f"invalid runtime_mode: {payload.get('runtime_mode')}")

    for field in ("input_hash", "config_hash", "output_artifact_hash"):
        value = payload.get(field)
        if not isinstance(value, str) or not value.startswith("sha256:"):
            errors.append(f"{field} must be a sha256-prefixed string")

    if not isinstance(payload.get("fallback_active"), bool):
        errors.append("fallback_active must be boolean")

    for field in ("parse_ms", "layout_ms", "render_ms", "diagnostic_count", "schema_version"):
        value = payload.get(field)
        if not isinstance(value, int) or value < 0:
            errors.append(f"{field} must be a non-negative integer")

    return errors


def _resolve_artifact_path(repo_root: Path, candidate: str) -> Path:
    path = Path(candidate)
    return path if path.is_absolute() else repo_root / path


def validate_e2e_summary(summary_path: Path, repo_root: Path, require_replay_bundle: bool = False) -> dict[str, object]:
    payload = read_json_file(summary_path, label="summary")
    if not isinstance(payload, dict):
        raise RuntimeError("summary JSON must be an object")
    errors: list[str] = []

    required_top_level = {
        "ok",
        "route_prefix",
        "surface",
        "host_kind",
        "repeat",
        "profiles",
        "scenarios",
        "results",
        "determinism",
    }
    missing = sorted(required_top_level - payload.keys())
    if missing:
        errors.append(f"missing summary fields: {', '.join(missing)}")

    if payload.get("surface") not in ALLOWED_SURFACES:
        errors.append(f"invalid summary surface: {payload.get('surface')}")
    if payload.get("host_kind") not in ALLOWED_HOST_KINDS:
        errors.append(f"invalid summary host_kind: {payload.get('host_kind')}")
    if not isinstance(payload.get("repeat"), int) or int(payload["repeat"]) < 1:
        errors.append("summary repeat must be a positive integer")

    profiles = payload.get("profiles")
    scenarios = payload.get("scenarios")
    results = payload.get("results")
    determinism = payload.get("determinism")
    differential = payload.get("differential")
    trace_index = payload.get("trace_index")
    if not isinstance(profiles, list) or not profiles:
        errors.append("summary profiles must be a non-empty list")
    if not isinstance(scenarios, list) or not scenarios:
        errors.append("summary scenarios must be a non-empty list")
    if not isinstance(results, list) or not results:
        errors.append("summary results must be a non-empty list")
    if not isinstance(determinism, list) or not determinism:
        errors.append("summary determinism must be a non-empty list")
    if differential is not None and not isinstance(differential, list):
        errors.append("summary differential must be a list when present")
    if trace_index is not None and not isinstance(trace_index, list):
        errors.append("summary trace_index must be a list when present")

    validated_results = 0
    if isinstance(results, list):
        for result in results:
            for field in (
                "scenario_id",
                "profile",
                "run_index",
                "html_path",
                "log_path",
                "diagnostic_count",
                "degradation_tier",
                "runtime_mode",
                "output_artifact_hash",
            ):
                if field not in result:
                    errors.append(f"result missing field: {field}")
            if trace_index is not None and "trace_id" not in result:
                errors.append("result missing field: trace_id")
            differential_report = result.get("differential")
            if differential_report is not None:
                for field in (
                    "telemetry_present",
                    "comparison_ready",
                    "franken_svg_present",
                    "mermaid_svg_present",
                    "health",
                    "mermaid_timing_ms",
                    "franken_svg_timing_ms",
                    "canvas_timing_ms",
                    "degradation_reasons",
                    "mermaid_baseline_degraded",
                    "franken_svg_degraded",
                    "runtime_artifact_missing",
                ):
                    if field not in differential_report:
                        errors.append(f"result differential missing field: {field}")
            html_path = result.get("html_path")
            log_path = result.get("log_path")
            if isinstance(html_path, str) and not _resolve_artifact_path(repo_root, html_path).exists():
                errors.append(f"missing result html_path: {html_path}")
            if isinstance(log_path, str):
                resolved_log = _resolve_artifact_path(repo_root, log_path)
                if not resolved_log.exists():
                    errors.append(f"missing result log_path: {log_path}")
                else:
                    log_payload = read_json_file(resolved_log, label="log")
                    if isinstance(log_payload, dict):
                        errors.extend(validate_log_payload(log_payload))
                    else:
                        errors.append(f"log payload must be a JSON object: {log_path}")
            validated_results += 1

    validated_determinism = 0
    if isinstance(determinism, list):
        for item in determinism:
            for field in ("scenario_id", "profile", "runs", "stable_output_hash", "stable_normalized_log", "output_hashes"):
                if field not in item:
                    errors.append(f"determinism entry missing field: {field}")
            if isinstance(item.get("output_hashes"), list) and isinstance(item.get("runs"), int):
                if len(item["output_hashes"]) != item["runs"]:
                    errors.append(
                        f"determinism output_hashes length mismatch for {item.get('scenario_id')}/{item.get('profile')}"
                    )
            validated_determinism += 1

    if isinstance(differential, list):
        for item in differential:
            for field in (
                "scenario_id",
                "profile",
                "run_index",
                "telemetry_present",
                "comparison_ready",
                "franken_svg_present",
                "mermaid_svg_present",
                "health",
                "mermaid_timing_ms",
                "franken_svg_timing_ms",
                "canvas_timing_ms",
                "degradation_reasons",
                "mermaid_baseline_degraded",
                "franken_svg_degraded",
                "runtime_artifact_missing",
            ):
                if field not in item:
                    errors.append(f"differential entry missing field: {field}")

    if isinstance(trace_index, list):
        for item in trace_index:
            for field in ("scenario_id", "profile", "run_index", "trace_id", "log_path"):
                if field not in item:
                    errors.append(f"trace_index entry missing field: {field}")

    replay_info = payload.get("replay_bundle")
    if require_replay_bundle:
        if not isinstance(replay_info, dict):
            errors.append("summary is missing replay_bundle metadata")
        else:
            for field in ("manifest_path", "script_path"):
                if field not in replay_info:
                    errors.append(f"replay_bundle missing field: {field}")
            manifest_path = replay_info.get("manifest_path")
            script_path = replay_info.get("script_path")
            if isinstance(manifest_path, str):
                resolved_manifest = _resolve_artifact_path(repo_root, manifest_path)
                if not resolved_manifest.exists():
                    errors.append(f"missing replay manifest: {manifest_path}")
                else:
                    manifest = read_json_file(resolved_manifest, label="replay manifest")
                    if isinstance(manifest, dict):
                        expected_commands = len(payload.get("profiles", [])) * len(payload.get("scenarios", []))
                        if len(manifest.get("scenario_commands", [])) != expected_commands:
                            errors.append("replay manifest scenario command count does not match scenario/profile matrix")
                        if trace_index is not None and manifest.get("trace_index") != trace_index:
                            errors.append("replay manifest trace_index does not match summary trace_index")
                    else:
                        errors.append("replay manifest must be a JSON object")
            if isinstance(script_path, str) and not _resolve_artifact_path(repo_root, script_path).exists():
                errors.append(f"missing replay shell helper: {script_path}")

    if errors:
        raise RuntimeError("; ".join(errors))

    return {
        "summary_path": str(summary_path),
        "surface": payload["surface"],
        "host_kind": payload["host_kind"],
        "result_count": validated_results,
        "determinism_count": validated_determinism,
        "profiles": payload["profiles"],
        "scenarios": payload["scenarios"],
        "has_replay_bundle": isinstance(replay_info, dict),
        "differential_count": len(differential) if isinstance(differential, list) else 0,
        "trace_count": len(trace_index) if isinstance(trace_index, list) else 0,
    }


def shared_scenario_id(scenario_id: str) -> str:
    for prefix in PARITY_REQUIRED_HOST_KINDS:
        if scenario_id.startswith(f"{prefix}-"):
            return scenario_id[len(prefix) + 1 :]
    return scenario_id


def collect_latest_logs(root: Path) -> dict[tuple[str, str], dict[str, object]]:
    latest: dict[tuple[str, str], tuple[Path, dict[str, object]]] = {}
    for path in sorted(root.rglob("*__e2e__log.json")):
        payload = read_json_file(path, label="log")
        if not isinstance(payload, dict):
            raise RuntimeError(f"log payload must be a JSON object: {path}")
        errors = validate_log_payload(payload)
        if errors:
            raise RuntimeError(f"{path} is not a valid showcase log: {errors}")
        scenario = shared_scenario_id(str(payload["scenario_id"]))
        profile = str(payload.get("profile", "default"))
        key = (scenario, profile)
        if key not in latest or path.name > latest[key][0].name:
            latest[key] = (path, payload)
    return {key: {"path": str(path), "payload": payload} for key, (path, payload) in latest.items()}


def compare_host_parity(
    *,
    static_root: Path,
    react_root: Path,
    allowed_metric_delta_ms: int = 250,
) -> dict[str, object]:
    static_logs = collect_latest_logs(static_root)
    react_logs = collect_latest_logs(react_root)

    static_keys = set(static_logs)
    react_keys = set(react_logs)
    missing_from_react = sorted(f"{scenario}/{profile}" for scenario, profile in (static_keys - react_keys))
    missing_from_static = sorted(f"{scenario}/{profile}" for scenario, profile in (react_keys - static_keys))

    pairs: list[dict[str, object]] = []
    parity_failures: list[str] = []

    for key in sorted(static_keys & react_keys):
        scenario, profile = key
        static_entry = static_logs[key]
        react_entry = react_logs[key]
        static_payload = static_entry["payload"]
        react_payload = react_entry["payload"]

        strict_mismatches: list[dict[str, object]] = []
        for field in PARITY_STRICT_FIELDS:
            static_value = static_payload.get(field)
            react_value = react_payload.get(field)
            if static_value != react_value:
                strict_mismatches.append(
                    {
                        "field": field,
                        "static": static_value,
                        "react": react_value,
                    }
                )

        acceptable_deltas: list[dict[str, object]] = []
        for field in PARITY_ACCEPTABLE_DELTA_FIELDS:
            static_value = static_payload.get(field)
            react_value = react_payload.get(field)
            if static_value != react_value:
                delta_record: dict[str, object] = {
                    "field": field,
                    "static": static_value,
                    "react": react_value,
                }
                if field in {"parse_ms", "layout_ms", "render_ms"}:
                    delta_record["difference_ms"] = abs(int(static_value) - int(react_value))
                    delta_record["within_tolerance"] = delta_record["difference_ms"] <= allowed_metric_delta_ms
                acceptable_deltas.append(delta_record)

        pair_ok = not strict_mismatches and all(
            delta.get("within_tolerance", True) for delta in acceptable_deltas
        )
        if not pair_ok:
            parity_failures.append(f"{scenario}/{profile}")

        pairs.append(
            {
                "scenario_id": scenario,
                "profile": profile,
                "ok": pair_ok,
                "static_log": static_entry["path"],
                "react_log": react_entry["path"],
                "strict_mismatches": strict_mismatches,
                "acceptable_deltas": acceptable_deltas,
            }
        )

    ok = not missing_from_react and not missing_from_static and not parity_failures
    return {
        "ok": ok,
        "static_root": str(static_root),
        "react_root": str(react_root),
        "allowed_metric_delta_ms": allowed_metric_delta_ms,
        "required_host_kinds": list(PARITY_REQUIRED_HOST_KINDS),
        "strict_fields": list(PARITY_STRICT_FIELDS),
        "acceptable_delta_fields": list(PARITY_ACCEPTABLE_DELTA_FIELDS),
        "missing_from_react": missing_from_react,
        "missing_from_static": missing_from_static,
        "pair_count": len(pairs),
        "failing_pairs": parity_failures,
        "pairs": pairs,
    }


def extract_module_script(html: str) -> str:
    match = re.search(r'<script type="module">(.*)</script>\s*</body>', html, re.S)
    if not match:
        raise ValueError("module script not found in HTML document")
    return match.group(1)


def extract_json_pre(dom: str, element_id: str) -> dict[str, object] | None:
    pattern = rf'<pre[^>]*id="{re.escape(element_id)}"[^>]*>(.*?)</pre>'
    match = re.search(pattern, dom, re.DOTALL)
    if not match:
        return None
    payload = html.unescape(match.group(1)).strip()
    if not payload or payload.endswith("will appear after the next committed render.") or payload.endswith(
        "will appear here after the checker runs."
    ):
        return None
    try:
        return json.loads(payload)
    except json.JSONDecodeError:
        return None


def extract_element_inner_html(dom: str, element_id: str) -> str | None:
    pattern = rf'<(?P<tag>[a-zA-Z0-9]+)[^>]*id="{re.escape(element_id)}"[^>]*>(?P<body>.*?)</(?P=tag)>'
    match = re.search(pattern, dom, re.DOTALL)
    if not match:
        return None
    return match.group("body")


def extract_differential_report(dom: str) -> dict[str, object]:
    telemetry = extract_json_pre(dom, "telemetry-json")
    franken_stage = extract_element_inner_html(dom, "fm-svg") or ""
    mermaid_stage = extract_element_inner_html(dom, "mermaid-svg") or ""
    degradation_reasons = list(telemetry.get("degradationReasons", [])) if isinstance(telemetry, dict) else []
    timings = dict(telemetry.get("timings", {})) if isinstance(telemetry, dict) else {}
    return {
        "telemetry_present": telemetry is not None,
        "comparison_ready": "<svg" in franken_stage.lower() and "<svg" in mermaid_stage.lower() and telemetry is not None,
        "franken_svg_present": "<svg" in franken_stage.lower(),
        "mermaid_svg_present": "<svg" in mermaid_stage.lower(),
        "health": telemetry.get("health", "unreported") if isinstance(telemetry, dict) else "unreported",
        "mermaid_timing_ms": int(timings.get("mermaid", 0) or 0) if isinstance(timings, dict) else 0,
        "franken_svg_timing_ms": int(timings.get("svg", 0) or 0) if isinstance(timings, dict) else 0,
        "canvas_timing_ms": int(timings.get("canvas", 0) or 0) if isinstance(timings, dict) else 0,
        "degradation_reasons": degradation_reasons,
        "mermaid_baseline_degraded": "mermaid baseline degraded" in degradation_reasons,
        "franken_svg_degraded": "franken svg render failed" in degradation_reasons,
        "runtime_artifact_missing": "runtime artifact missing" in degradation_reasons,
    }


def run_node_check(script: str) -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".mjs", delete=False) as handle:
        handle.write(script)
        temp_path = handle.name
    try:
        result = subprocess.run(
            ["node", "--check", temp_path],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            raise RuntimeError(result.stderr.strip() or "node --check failed")
    finally:
        import os
        try:
            os.unlink(temp_path)
        except OSError:
            pass


def run_json_command(command: list[str], cwd: Path) -> dict[str, object]:
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
        cwd=cwd,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip() or f"command failed: {' '.join(command)}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"command did not emit valid JSON: {' '.join(command)}") from exc


def validate_static_web(entry: Path, headers: Path, contract: Path, log_path: Path | None) -> dict[str, object]:
    entry_text = entry.read_text(encoding="utf-8")
    headers_text = headers.read_text(encoding="utf-8")
    contract_text = contract.read_text(encoding="utf-8")
    header_rules = parse_headers_manifest(headers_text)

    parser = HtmlSmokeParser()
    parser.feed(entry_text)
    parser.close()

    run_node_check(extract_module_script(entry_text))

    checks = [
        CheckResult(
            "source fetch",
            "../frankenmermaid_demo_showcase.html" in entry_text,
            "web host bootstraps the standalone showcase artifact",
        ),
        CheckResult(
            "document write bootstrap",
            "document.write(finalHtml);" in entry_text,
            "bootstrap host replaces shell with standalone showcase document",
        ),
        CheckResult(
            "host marker injection",
            'data-host-kind="static-web"' in entry_text,
            "static host marks injected HTML for downstream adapter assertions",
        ),
        CheckResult(
            "pkg cache rule",
            has_cache_rule(header_rules, "/pkg/*", REVALIDATING_CACHE_CONTROL),
            "static host keeps stable /pkg/* runtime assets on a revalidating cache policy until revisioned asset paths exist",
        ),
        CheckResult(
            "evidence cache rule",
            has_cache_rule(header_rules, "/evidence/*", EVIDENCE_CACHE_CONTROL),
            "static host publishes review-friendly cache policy for evidence artifacts",
        ),
        CheckResult(
            "headers root rule",
            has_cache_rule(header_rules, "/web", REVALIDATING_CACHE_CONTROL),
            "static host publishes explicit revalidating cache behavior for the /web entry route",
        ),
        CheckResult(
            "headers subtree rule",
            has_cache_rule(header_rules, "/web/*", REVALIDATING_CACHE_CONTROL),
            "static host publishes explicit revalidating cache behavior for /web deep links",
        ),
        CheckResult(
            "contract alignment",
            "root-level `/pkg/...` and `/evidence/...`" in contract_text,
            "entrypoint contract matches file-style /web asset semantics",
        ),
        CheckResult(
            "contract cache safety",
            "Because those paths are not revisioned today, `/pkg/*` must remain `public, max-age=0, must-revalidate`."
            in contract_text
            and "`immutable` caching is forbidden for the non-revisioned `/pkg/*` surface" in contract_text,
            "entrypoint contract documents the stable-runtime cache safety constraint",
        ),
        CheckResult(
            "contract route strategy",
            "`/web/` and `/web_react/` should redirect to `/web` and `/web_react` with HTTP 301 while preserving the query string."
            in contract_text
            and "Cloudflare Redirect Rules or Bulk Redirects with query preservation enabled instead of Pages `_redirects`"
            in contract_text
            and "Query parameters on `/web` and `/web_react` are state-bearing and must stay in the cache key;"
            in contract_text
            and "`_routes.json` should exclude `/pkg/*`, `/evidence/*`, `/web`, `/web/*`, `/web_react`, and `/web_react/*`"
            in contract_text,
            "entrypoint contract documents the Cloudflare route, cache-key, and Functions-exclusion plan",
        ),
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = read_json_file(log_path, label="log")
        if isinstance(payload, dict):
            failures.extend(validate_log_payload(payload))
        else:
            failures.append("log payload must be a JSON object")

    if failures:
        raise RuntimeError("; ".join(failures))

    return {
        "surface": "web",
        "entry": str(entry),
        "headers": str(headers),
        "contract": str(contract),
        "entry_hash": sha256_file(entry),
        "headers_hash": sha256_file(headers),
        "contract_hash": sha256_file(contract),
        "log_path": str(log_path) if log_path else None,
        "checks": [check.to_dict() for check in checks],
    }


def validate_react_web(entry: Path, headers: Path, contract: Path, log_path: Path | None) -> dict[str, object]:
    entry_text = entry.read_text(encoding="utf-8")
    headers_text = headers.read_text(encoding="utf-8")
    contract_text = contract.read_text(encoding="utf-8")
    header_rules = parse_headers_manifest(headers_text)

    parser = HtmlSmokeParser()
    parser.feed(entry_text)
    parser.close()

    run_node_check(extract_module_script(entry_text))

    checks = [
        CheckResult(
            "source fetch",
            "../frankenmermaid_demo_showcase.html" in entry_text,
            "react host bootstraps the standalone showcase artifact",
        ),
        CheckResult(
            "react root shell",
            'id="showcase-react-root"' in entry_text and 'data-showcase-host="react-web"' in entry_text,
            "react route defines a stable host root and host marker",
        ),
        CheckResult(
            "host marker injection",
            'data-host-kind="react-web"' in entry_text,
            "react host rewrites injected HTML with the react-web host kind",
        ),
        CheckResult(
            "body marker injection",
            'data-react-route-root="showcase-react-root"' in entry_text,
            "react host stamps the injected body with a route root marker",
        ),
        CheckResult(
            "bootstrap function",
            "async function bootstrapReactHost()" in entry_text,
            "react route owns a distinct bootstrap entrypoint",
        ),
        CheckResult(
            "headers root rule",
            has_cache_rule(header_rules, "/web_react", REVALIDATING_CACHE_CONTROL),
            "react route publishes explicit revalidating cache behavior for the root route",
        ),
        CheckResult(
            "headers subtree rule",
            has_cache_rule(header_rules, "/web_react/*", REVALIDATING_CACHE_CONTROL),
            "react route publishes explicit revalidating cache behavior for deep links",
        ),
        CheckResult(
            "pkg cache rule",
            has_cache_rule(header_rules, "/pkg/*", REVALIDATING_CACHE_CONTROL),
            "react route keeps stable /pkg/* runtime assets on a revalidating cache policy until revisioned asset paths exist",
        ),
        CheckResult(
            "evidence cache rule",
            has_cache_rule(header_rules, "/evidence/*", EVIDENCE_CACHE_CONTROL),
            "react route publishes review-friendly cache policy for evidence artifacts",
        ),
        CheckResult(
            "contract alignment",
            "bd-2u0.5.8.3.2" in contract_text
            and "the `/web_react` route against this component/service boundary" in contract_text,
            "react route aligns with the checked-in embedding contract",
        ),
        CheckResult(
            "contract cache matrix",
            "`/web_react` shares the same Pages project cache matrix as `/web`." in contract_text
            and "Cloudflare Redirect Rules or Bulk Redirects with query preservation enabled instead of Pages `_redirects`"
            in contract_text
            and "Query-bearing entry routes must not use cache rules that ignore search parameters."
            in contract_text
            and "`/pkg/*` => `public, max-age=0, must-revalidate` while runtime asset names remain stable"
            in contract_text
            and "`_routes.json` should exclude the static showcase routes/assets unless a route is intentionally dynamic."
            in contract_text,
            "react embedding contract aligns with the Cloudflare route/cache strategy",
        ),
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = read_json_file(log_path, label="log")
        if isinstance(payload, dict):
            failures.extend(validate_log_payload(payload))
        else:
            failures.append("log payload must be a JSON object")

    if failures:
        raise RuntimeError("; ".join(failures))

    return {
        "surface": "web_react",
        "entry": str(entry),
        "headers": str(headers),
        "contract": str(contract),
        "entry_hash": sha256_file(entry),
        "headers_hash": sha256_file(headers),
        "contract_hash": sha256_file(contract),
        "log_path": str(log_path) if log_path else None,
        "checks": [check.to_dict() for check in checks],
    }


def validate_cloudflare_hosting_plan(
    *,
    static_headers: Path,
    react_headers: Path,
    static_contract: Path,
    react_contract: Path,
    strategy_doc: Path | None = None,
) -> dict[str, object]:
    static_rules = parse_headers_manifest(static_headers.read_text(encoding="utf-8"))
    react_rules = parse_headers_manifest(react_headers.read_text(encoding="utf-8"))
    static_contract_text = static_contract.read_text(encoding="utf-8")
    react_contract_text = react_contract.read_text(encoding="utf-8")
    strategy_text = strategy_doc.read_text(encoding="utf-8") if strategy_doc is not None else ""

    checks = [
        CheckResult(
            "shared pkg cache rule",
            has_cache_rule(static_rules, "/pkg/*", REVALIDATING_CACHE_CONTROL)
            and has_cache_rule(react_rules, "/pkg/*", REVALIDATING_CACHE_CONTROL),
            "both showcase hosts keep stable /pkg/* runtime assets revalidating until versioned paths exist",
        ),
        CheckResult(
            "shared evidence cache rule",
            has_cache_rule(static_rules, "/evidence/*", EVIDENCE_CACHE_CONTROL)
            and has_cache_rule(react_rules, "/evidence/*", EVIDENCE_CACHE_CONTROL),
            "both showcase hosts publish the same review-friendly cache policy for evidence artifacts",
        ),
        CheckResult(
            "static route rules",
            has_cache_rule(static_rules, "/web", REVALIDATING_CACHE_CONTROL)
            and has_cache_rule(static_rules, "/web/*", REVALIDATING_CACHE_CONTROL),
            "static host publishes explicit revalidating cache rules for /web and /web/*",
        ),
        CheckResult(
            "react route rules",
            has_cache_rule(react_rules, "/web_react", REVALIDATING_CACHE_CONTROL)
            and has_cache_rule(react_rules, "/web_react/*", REVALIDATING_CACHE_CONTROL),
            "react host publishes explicit revalidating cache rules for /web_react and /web_react/*",
        ),
        CheckResult(
            "static contract route matrix",
            "Current cache matrix:" in static_contract_text
            and "Future optimization after versioned assets exist:" in static_contract_text
            and "Cloudflare Redirect Rules or Bulk Redirects with query preservation enabled instead of Pages `_redirects`"
            in static_contract_text
            and "`_routes.json` should exclude `/pkg/*`, `/evidence/*`, `/web`, `/web/*`, `/web_react`, and `/web_react/*`"
            in static_contract_text,
            "static contract captures the current cache matrix, the future immutable-versioned path plan, and the Functions exclusion plan",
        ),
        CheckResult(
            "react contract alignment",
            "`/web_react` shares the same Pages project cache matrix as `/web`." in react_contract_text
            and "Cloudflare Redirect Rules or Bulk Redirects with query preservation enabled instead of Pages `_redirects`"
            in react_contract_text
            and "When deployment packaging emits revisioned runtime asset paths or hashed filenames" in react_contract_text,
            "react contract acknowledges the shared Pages route/cache matrix and the future versioned-asset handoff",
        ),
    ]

    if strategy_doc is not None:
        checks.append(
            CheckResult(
                "strategy trace",
                "bd-2u0.5.9.1" in strategy_text and "validate-hosting-plan" in strategy_text,
                "demo strategy points deployment work at the checked-in hosting-plan validator",
            )
        )

    failures = [check.detail for check in checks if not check.ok]
    if failures:
        raise RuntimeError("; ".join(failures))

    return {
        "surface": "cloudflare-hosting-plan",
        "static_headers": str(static_headers),
        "react_headers": str(react_headers),
        "static_contract": str(static_contract),
        "react_contract": str(react_contract),
        "strategy_doc": str(strategy_doc) if strategy_doc else None,
        "static_headers_hash": sha256_file(static_headers),
        "react_headers_hash": sha256_file(react_headers),
        "static_contract_hash": sha256_file(static_contract),
        "react_contract_hash": sha256_file(react_contract),
        "strategy_doc_hash": sha256_file(strategy_doc) if strategy_doc else None,
        "checks": [check.to_dict() for check in checks],
    }


def validate_cloudflare_deploy_ops(
    *,
    wrangler_config: Path,
    ops_script: Path,
    static_contract: Path,
    react_contract: Path,
    strategy_doc: Path | None = None,
) -> dict[str, object]:
    repo_root = ops_script.resolve().parent.parent
    wrangler_payload = read_json_file(wrangler_config, label="wrangler config")
    if not isinstance(wrangler_payload, dict):
        raise RuntimeError("wrangler config must be a JSON object")
    static_contract_text = static_contract.read_text(encoding="utf-8")
    react_contract_text = react_contract.read_text(encoding="utf-8")
    strategy_text = strategy_doc.read_text(encoding="utf-8") if strategy_doc is not None else ""

    with tempfile.TemporaryDirectory() as tempdir:
        temp_root = Path(tempdir)
        staged_bundle = run_json_command(
            [
                "python3",
                str(ops_script),
                "stage-bundle",
                "--repo-root",
                str(repo_root),
                "--output-dir",
                str(temp_root / "bundle"),
            ],
            cwd=repo_root,
        )
        preview_deploy = run_json_command(
            [
                "python3",
                str(ops_script),
                "preview-deploy",
                "--repo-root",
                str(repo_root),
                "--output-dir",
                str(temp_root / "preview"),
                "--project-name",
                "frankenmermaid",
                "--branch",
                "preview-smoke",
                "--commit-hash",
                "deadbeef",
                "--commit-message",
                "preview smoke",
                "--dry-run",
            ],
            cwd=repo_root,
        )
        production_deploy = run_json_command(
            [
                "python3",
                str(ops_script),
                "production-deploy",
                "--repo-root",
                str(repo_root),
                "--output-dir",
                str(temp_root / "production"),
                "--project-name",
                "frankenmermaid",
                "--commit-hash",
                "deadbeef",
                "--commit-message",
                "production smoke",
                "--dry-run",
            ],
            cwd=repo_root,
        )
        rollback_drill = run_json_command(
            [
                "python3",
                str(ops_script),
                "rollback-drill",
                "--account-id",
                "account-id",
                "--project-name",
                "frankenmermaid",
                "--deployment-id",
                "deployment-id",
                "--reason",
                "smoke drill",
                "--dry-run",
            ],
            cwd=repo_root,
        )

    checks = [
        CheckResult(
            "wrangler config name",
            isinstance(wrangler_payload.get("name"), str) and bool(wrangler_payload.get("name")),
            "wrangler config defines a Pages project name",
        ),
        CheckResult(
            "wrangler output dir",
            isinstance(wrangler_payload.get("pages_build_output_dir"), str)
            and wrangler_payload["pages_build_output_dir"].startswith("./dist/cloudflare-pages/"),
            "wrangler config points at an isolated dist/cloudflare-pages staging root",
        ),
        CheckResult(
            "wrangler env overrides",
            wrangler_payload.get("env", {}).get("preview", {}).get("vars", {}).get("FM_DEPLOY_ENV") == "preview"
            and wrangler_payload.get("env", {}).get("production", {}).get("vars", {}).get("FM_DEPLOY_ENV")
            == "production",
            "wrangler config distinguishes preview and production deployment metadata",
        ),
        CheckResult(
            "static redirect strategy",
            "Cloudflare Redirect Rules or Bulk Redirects with query preservation enabled instead of Pages `_redirects`"
            in static_contract_text,
            "static contract documents the Redirect Rules requirement for canonical query-preserving redirects",
        ),
        CheckResult(
            "react redirect strategy",
            "Cloudflare Redirect Rules or Bulk Redirects with query preservation enabled instead of Pages `_redirects`"
            in react_contract_text,
            "react contract documents the Redirect Rules requirement for canonical query-preserving redirects",
        ),
        CheckResult(
            "bundle staging",
            staged_bundle.get("action") == "stage-bundle"
            and staged_bundle.get("redirect_strategy", {}).get("kind") == "cloudflare-redirect-rules"
            and any(item.get("path") == "_headers" for item in staged_bundle.get("generated_files", [])),
            "ops script stages the Pages bundle, emits a merged root _headers file, and records Redirect Rules guidance",
        ),
        CheckResult(
            "preview deploy dry run",
            preview_deploy.get("action") == "preview-deploy"
            and "--branch" in preview_deploy.get("command", [])
            and "preview-smoke" in preview_deploy.get("command", []),
            "ops script prints a preview deployment command with explicit branch metadata",
        ),
        CheckResult(
            "production deploy dry run",
            production_deploy.get("action") == "production-deploy"
            and "--branch" not in production_deploy.get("command", []),
            "ops script prints a production deployment command without preview-branch override",
        ),
        CheckResult(
            "rollback drill",
            rollback_drill.get("supported_execution") == "dashboard"
            and rollback_drill.get("preflight_request", {}).get("method") == "GET",
            "ops script keeps rollback honesty by using a deployment-list preflight and dashboard rollback drill payload",
        ),
    ]
    if strategy_doc is not None:
        checks.append(
            CheckResult(
                "strategy trace",
                "cloudflare_pages_ops.py" in strategy_text and "validate-cloudflare-deploy-ops" in strategy_text,
                "demo strategy points deployment work at the checked-in runbook and validator",
            )
        )

    failures = [check.detail for check in checks if not check.ok]
    if failures:
        raise RuntimeError("; ".join(failures))

    return {
        "surface": "cloudflare-deploy-ops",
        "wrangler_config": str(wrangler_config),
        "ops_script": str(ops_script),
        "static_contract": str(static_contract),
        "react_contract": str(react_contract),
        "strategy_doc": str(strategy_doc) if strategy_doc else None,
        "wrangler_config_hash": sha256_file(wrangler_config),
        "ops_script_hash": sha256_file(ops_script),
        "static_contract_hash": sha256_file(static_contract),
        "react_contract_hash": sha256_file(react_contract),
        "strategy_doc_hash": sha256_file(strategy_doc) if strategy_doc else None,
        "checks": [check.to_dict() for check in checks],
    }


def validate_showcase_accessibility(entry: Path, log_path: Path | None) -> dict[str, object]:
    entry_text = entry.read_text(encoding="utf-8")

    parser = HtmlSmokeParser()
    parser.feed(entry_text)
    parser.close()

    checks = [
        CheckResult(
            "skip link",
            'class="skip-link"' in entry_text and 'href="#main-content"' in entry_text,
            "showcase exposes a skip link that jumps to the main landmark",
        ),
        CheckResult(
            "main landmark",
            '<main id="main-content"' in entry_text,
            "showcase exposes a stable main landmark target for keyboard navigation",
        ),
        CheckResult(
            "reduced motion css",
            "@media (prefers-reduced-motion: reduce)" in entry_text,
            "showcase defines a reduced-motion CSS branch",
        ),
        CheckResult(
            "contrast css",
            "@media (prefers-contrast: more)" in entry_text,
            "showcase defines a high-contrast CSS branch",
        ),
        CheckResult(
            "focus visible",
            ":where(a, button, input, select, textarea, summary, [tabindex]):focus-visible" in entry_text,
            "showcase defines a shared focus-visible treatment",
        ),
        CheckResult(
            "spotlight keyboard zoom",
            'id="spotlight-stage"' in entry_text and 'aria-label="Toggle zoom for the spotlight render preview"' in entry_text,
            "spotlight render surface is keyboard focusable and labeled for zoom behavior",
        ),
        CheckResult(
            "live summaries",
            'id="parse-summary"' in entry_text and 'aria-live="polite"' in entry_text,
            "showcase announces summary updates via polite live regions",
        ),
        # Graph Deck section (bd-f2njj / epic bd-z7g6k). DOM-presence level on purpose —
        # the default gate stays headless-browser-free; behavioral coverage lives in
        # scripts/verify_deck_runtime.mjs (byte-identity + real WASM render) and the
        # opt-in scripts/deck_runtime_e2e.py.
        CheckResult(
            "deck section",
            'id="deck"' in entry_text and 'id="deck-stage"' in entry_text,
            "showcase carries the Graph Deck section with its focusable stage",
        ),
        CheckResult(
            "deck runtime block",
            ">>> deck-runtime:start" in entry_text and ">>> deck-runtime:end" in entry_text,
            "showcase inlines the deck runtime between its sync markers",
        ),
        CheckResult(
            "deck chrome",
            'id="deck-num"' in entry_text and 'id="deck-dots"' in entry_text,
            "deck section exposes the scene counter and dots the runtime populates",
        ),
        CheckResult(
            "deck keyboard scoping",
            "event.stopPropagation()" in entry_text,
            "deck keyboard handler stays stage-scoped so spotlight arrows never double-fire",
        ),
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = read_json_file(log_path, label="log")
        if isinstance(payload, dict):
            failures.extend(validate_log_payload(payload))
        else:
            failures.append("log payload must be a JSON object")

    if failures:
        raise RuntimeError("; ".join(failures))

    return {
        "surface": "standalone",
        "entry": str(entry),
        "entry_hash": sha256_file(entry),
        "log_path": str(log_path) if log_path else None,
        "checks": [check.to_dict() for check in checks],
    }


def validate_showcase_compatibility(entry: Path, log_path: Path | None) -> dict[str, object]:
    entry_text = entry.read_text(encoding="utf-8")

    parser = HtmlSmokeParser()
    parser.feed(entry_text)
    parser.close()

    checks = [
        CheckResult(
            "uuid fallback",
            "function makeUniqueId(prefix)" in entry_text and "crypto.randomUUID" in entry_text,
            "showcase falls back when randomUUID is unavailable",
        ),
        CheckResult(
            "clipboard fallback",
            "function writeClipboardText(value)" in entry_text and "document.execCommand(\"copy\")" in entry_text,
            "showcase falls back when navigator.clipboard is unavailable",
        ),
        CheckResult(
            "intersection observer fallback",
            "typeof IntersectionObserver === \"function\"" in entry_text,
            "showcase degrades gracefully when IntersectionObserver is unavailable",
        ),
        CheckResult(
            "backdrop support fallback",
            "@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px)))" in entry_text,
            "showcase defines a CSS fallback when backdrop-filter is unsupported",
        ),
        CheckResult(
            "reduced motion runtime gate",
            "if (!prefersReducedMotion()) {" in entry_text and "motionBehavior()" in entry_text,
            "showcase gates scripted motion and smooth scrolling on reduced-motion preference",
        ),
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = read_json_file(log_path, label="log")
        if isinstance(payload, dict):
            failures.extend(validate_log_payload(payload))
        else:
            failures.append("log payload must be a JSON object")

    if failures:
        raise RuntimeError("; ".join(failures))

    return {
        "surface": "standalone",
        "entry": str(entry),
        "entry_hash": sha256_file(entry),
        "log_path": str(log_path) if log_path else None,
        "checks": [check.to_dict() for check in checks],
    }


def cmd_validate_log(args: argparse.Namespace) -> int:
    try:
        payload = read_json_file(Path(args.log), label="log")
    except RuntimeError as exc:
        print(json.dumps({"ok": False, "errors": [str(exc)]}, indent=2))
        return 1
    if not isinstance(payload, dict):
        print(json.dumps({"ok": False, "errors": ["log payload must be a JSON object"]}, indent=2))
        return 1
    errors = validate_log_payload(payload)
    if errors:
        print(json.dumps({"ok": False, "errors": errors}, indent=2))
        return 1
    print(json.dumps({"ok": True, "log": args.log}, indent=2))
    return 0


def cmd_validate_static_web(args: argparse.Namespace) -> int:
    result = validate_static_web(
        entry=Path(args.entry),
        headers=Path(args.headers),
        contract=Path(args.contract),
        log_path=Path(args.log) if args.log else None,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_validate_react_web(args: argparse.Namespace) -> int:
    result = validate_react_web(
        entry=Path(args.entry),
        headers=Path(args.headers),
        contract=Path(args.contract),
        log_path=Path(args.log) if args.log else None,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_validate_cloudflare_hosting_plan(args: argparse.Namespace) -> int:
    result = validate_cloudflare_hosting_plan(
        static_headers=Path(args.static_headers),
        react_headers=Path(args.react_headers),
        static_contract=Path(args.static_contract),
        react_contract=Path(args.react_contract),
        strategy_doc=Path(args.strategy_doc) if args.strategy_doc else None,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_validate_cloudflare_deploy_ops(args: argparse.Namespace) -> int:
    result = validate_cloudflare_deploy_ops(
        wrangler_config=Path(args.wrangler_config),
        ops_script=Path(args.ops_script),
        static_contract=Path(args.static_contract),
        react_contract=Path(args.react_contract),
        strategy_doc=Path(args.strategy_doc) if args.strategy_doc else None,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_validate_showcase_accessibility(args: argparse.Namespace) -> int:
    result = validate_showcase_accessibility(
        entry=Path(args.entry),
        log_path=Path(args.log) if args.log else None,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_validate_showcase_compatibility(args: argparse.Namespace) -> int:
    result = validate_showcase_compatibility(
        entry=Path(args.entry),
        log_path=Path(args.log) if args.log else None,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_validate_e2e_summary(args: argparse.Namespace) -> int:
    result = validate_e2e_summary(
        summary_path=Path(args.summary),
        repo_root=Path(args.repo_root),
        require_replay_bundle=args.require_replay_bundle,
    )
    print(json.dumps({"ok": True, "result": result}, indent=2))
    return 0


def cmd_compare_host_parity(args: argparse.Namespace) -> int:
    result = compare_host_parity(
        static_root=Path(args.static_root),
        react_root=Path(args.react_root),
        allowed_metric_delta_ms=args.allowed_metric_delta_ms,
    )
    if args.report_out:
        report_path = Path(args.report_out)
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["ok"] else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Reusable showcase validation harness")
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate_log = subparsers.add_parser("validate-log", help="Validate a structured showcase log")
    validate_log.add_argument("log", help="Path to a JSON evidence log")
    validate_log.set_defaults(func=cmd_validate_log)

    validate_static = subparsers.add_parser(
        "validate-static-web",
        help="Validate the static /web bootstrap surface against the shared contract",
    )
    validate_static.add_argument("--entry", required=True, help="Path to web entry HTML")
    validate_static.add_argument("--headers", required=True, help="Path to static host _headers file")
    validate_static.add_argument("--contract", required=True, help="Path to static entry contract")
    validate_static.add_argument("--log", help="Optional path to structured evidence log to validate too")
    validate_static.set_defaults(func=cmd_validate_static_web)

    validate_react = subparsers.add_parser(
        "validate-react-web",
        help="Validate the /web_react bootstrap surface against the React embedding contract",
    )
    validate_react.add_argument("--entry", required=True, help="Path to web_react entry HTML")
    validate_react.add_argument("--headers", required=True, help="Path to web_react _headers file")
    validate_react.add_argument("--contract", required=True, help="Path to React embedding contract")
    validate_react.add_argument("--log", help="Optional path to structured evidence log to validate too")
    validate_react.set_defaults(func=cmd_validate_react_web)

    validate_hosting_plan = subparsers.add_parser(
        "validate-hosting-plan",
        help="Validate the cross-surface Cloudflare route/cache/asset strategy for /web and /web_react",
    )
    validate_hosting_plan.add_argument("--static-headers", required=True, help="Path to web/_headers")
    validate_hosting_plan.add_argument("--react-headers", required=True, help="Path to web_react/_headers")
    validate_hosting_plan.add_argument(
        "--static-contract",
        required=True,
        help="Path to the static entry contract",
    )
    validate_hosting_plan.add_argument(
        "--react-contract",
        required=True,
        help="Path to the React embedding contract",
    )
    validate_hosting_plan.add_argument(
        "--strategy-doc",
        help="Optional path to the demo strategy document for traceability checks",
    )
    validate_hosting_plan.set_defaults(func=cmd_validate_cloudflare_hosting_plan)

    validate_deploy_ops = subparsers.add_parser(
        "validate-cloudflare-deploy-ops",
        help="Validate the checked-in Pages/Wrangler deployment automation for /web and /web_react",
    )
    validate_deploy_ops.add_argument("--wrangler-config", required=True, help="Path to wrangler.jsonc")
    validate_deploy_ops.add_argument("--ops-script", required=True, help="Path to cloudflare_pages_ops.py")
    validate_deploy_ops.add_argument(
        "--static-contract",
        required=True,
        help="Path to the static entry contract",
    )
    validate_deploy_ops.add_argument(
        "--react-contract",
        required=True,
        help="Path to the React embedding contract",
    )
    validate_deploy_ops.add_argument(
        "--strategy-doc",
        help="Optional path to the demo strategy document for traceability checks",
    )
    validate_deploy_ops.set_defaults(func=cmd_validate_cloudflare_deploy_ops)

    validate_a11y = subparsers.add_parser(
        "validate-showcase-accessibility",
        help="Validate standalone showcase accessibility guardrails",
    )
    validate_a11y.add_argument("--entry", required=True, help="Path to standalone showcase HTML")
    validate_a11y.add_argument("--log", help="Optional structured evidence log to validate too")
    validate_a11y.set_defaults(func=cmd_validate_showcase_accessibility)

    validate_compat = subparsers.add_parser(
        "validate-showcase-compatibility",
        help="Validate standalone showcase compatibility and fallback guardrails",
    )
    validate_compat.add_argument("--entry", required=True, help="Path to standalone showcase HTML")
    validate_compat.add_argument("--log", help="Optional structured evidence log to validate too")
    validate_compat.set_defaults(func=cmd_validate_showcase_compatibility)

    parity = subparsers.add_parser(
        "compare-host-parity",
        help="Compare normalized /web and /web_react E2E evidence and emit a parity report",
    )
    parity.add_argument("--static-root", required=True, help="Directory containing /web E2E logs")
    parity.add_argument("--react-root", required=True, help="Directory containing /web_react E2E logs")
    parity.add_argument(
        "--allowed-metric-delta-ms",
        type=int,
        default=250,
        help="Maximum tolerated parse/layout/render timing delta before parity fails",
    )
    parity.add_argument("--report-out", help="Optional path to write the JSON parity report")
    parity.set_defaults(func=cmd_compare_host_parity)

    validate_summary = subparsers.add_parser(
        "validate-e2e-summary",
        help="Validate a hosted showcase E2E summary and optional replay bundle completeness",
    )
    validate_summary.add_argument("--summary", required=True, help="Path to a __determinism__summary.json file")
    validate_summary.add_argument(
        "--repo-root",
        default=".",
        help="Repository root used to resolve relative artifact paths",
    )
    validate_summary.add_argument(
        "--require-replay-bundle",
        action="store_true",
        help="Fail if replay manifest/script metadata is missing or incomplete",
    )
    validate_summary.set_defaults(func=cmd_validate_e2e_summary)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
