#!/usr/bin/env python3
"""Reusable validation harness for showcase artifacts and host adapters."""

from __future__ import annotations

import argparse
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


class HtmlSmokeParser(HTMLParser):
    """Minimal parser wrapper so malformed HTML raises via feed/close usage."""


@dataclass
class CheckResult:
    name: str
    ok: bool
    detail: str

    def to_dict(self) -> dict[str, object]:
        return {"name": self.name, "ok": self.ok, "detail": self.detail}


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
    payload = json.loads(summary_path.read_text())
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
    if not isinstance(profiles, list) or not profiles:
        errors.append("summary profiles must be a non-empty list")
    if not isinstance(scenarios, list) or not scenarios:
        errors.append("summary scenarios must be a non-empty list")
    if not isinstance(results, list) or not results:
        errors.append("summary results must be a non-empty list")
    if not isinstance(determinism, list) or not determinism:
        errors.append("summary determinism must be a non-empty list")

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
            html_path = result.get("html_path")
            log_path = result.get("log_path")
            if isinstance(html_path, str) and not _resolve_artifact_path(repo_root, html_path).exists():
                errors.append(f"missing result html_path: {html_path}")
            if isinstance(log_path, str):
                resolved_log = _resolve_artifact_path(repo_root, log_path)
                if not resolved_log.exists():
                    errors.append(f"missing result log_path: {log_path}")
                else:
                    errors.extend(validate_log_payload(json.loads(resolved_log.read_text())))
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
                    manifest = json.loads(resolved_manifest.read_text())
                    expected_commands = len(payload.get("profiles", [])) * len(payload.get("scenarios", []))
                    if len(manifest.get("scenario_commands", [])) != expected_commands:
                        errors.append("replay manifest scenario command count does not match scenario/profile matrix")
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
    }


def shared_scenario_id(scenario_id: str) -> str:
    for prefix in PARITY_REQUIRED_HOST_KINDS:
        if scenario_id.startswith(f"{prefix}-"):
            return scenario_id[len(prefix) + 1 :]
    return scenario_id


def collect_latest_logs(root: Path) -> dict[tuple[str, str], dict[str, object]]:
    latest: dict[tuple[str, str], tuple[Path, dict[str, object]]] = {}
    for path in sorted(root.rglob("*__e2e__log.json")):
        payload = json.loads(path.read_text())
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


def validate_static_web(entry: Path, headers: Path, contract: Path, log_path: Path | None) -> dict[str, object]:
    entry_text = entry.read_text()
    headers_text = headers.read_text()
    contract_text = contract.read_text()

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
            "/pkg/*" in headers_text,
            "static host publishes immutable cache policy for root pkg assets",
        ),
        CheckResult(
            "evidence cache rule",
            "/evidence/*" in headers_text,
            "static host publishes review-friendly cache policy for evidence artifacts",
        ),
        CheckResult(
            "contract alignment",
            "root-level `/pkg/...` and `/evidence/...`" in contract_text,
            "entrypoint contract matches file-style /web asset semantics",
        ),
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = json.loads(log_path.read_text())
        failures.extend(validate_log_payload(payload))

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
    entry_text = entry.read_text()
    headers_text = headers.read_text()
    contract_text = contract.read_text()

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
            "/web_react" in headers_text,
            "react route publishes explicit cache behavior for the root route",
        ),
        CheckResult(
            "headers subtree rule",
            "/web_react/*" in headers_text,
            "react route publishes explicit cache behavior for deep links",
        ),
        CheckResult(
            "contract alignment",
            "bd-2u0.5.8.3.2" in contract_text
            and "the `/web_react` route against this component/service boundary" in contract_text,
            "react route aligns with the checked-in embedding contract",
        ),
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = json.loads(log_path.read_text())
        failures.extend(validate_log_payload(payload))

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


def validate_showcase_accessibility(entry: Path, log_path: Path | None) -> dict[str, object]:
    entry_text = entry.read_text()

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
    ]

    failures = [check.detail for check in checks if not check.ok]
    if log_path is not None:
        payload = json.loads(log_path.read_text())
        failures.extend(validate_log_payload(payload))

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
    entry_text = entry.read_text()

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
        payload = json.loads(log_path.read_text())
        failures.extend(validate_log_payload(payload))

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
    payload = json.loads(Path(args.log).read_text())
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
