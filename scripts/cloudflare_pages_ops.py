#!/usr/bin/env python3
"""Cloudflare Pages deployment helpers for the hosted showcase surfaces."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts.showcase_harness import (
    compare_host_parity,
    parse_headers_manifest,
    validate_cloudflare_hosting_plan,
    validate_e2e_summary,
)


DEFAULT_CANONICAL_HOST = "frankenmermaid.com"
DEFAULT_COMPATIBILITY_DATE = "2026-04-06"
DEFAULT_OUTPUT_DIR = Path("dist/cloudflare-pages/latest")
DEFAULT_SMOKE_STAGE_DIR = Path("dist/cloudflare-pages/deploy-smoke-stage")
DEFAULT_SMOKE_OUTPUT_DIR = Path("evidence/runs/web/bd-2u0.5.9.3/deploy-smoke")
DEFAULT_PREVIEW_BRANCH = "preview-web"
DEFAULT_PROJECT_NAME = "frankenmermaid"
PAGES_ROLLBACK_DOCS_URL = "https://developers.cloudflare.com/pages/configuration/rollbacks/"
PAGES_API_DOCS_URL = "https://developers.cloudflare.com/pages/configuration/api/"
PAGES_WRANGLER_DOCS_URL = "https://developers.cloudflare.com/pages/functions/wrangler-configuration/"
DEFAULT_SMOKE_SCENARIOS = (
    "static-web-compare-export",
    "static-web-diagnostics-recovery",
)
DEFAULT_SMOKE_PROFILES = ("desktop-default",)
HISTORICAL_EVIDENCE_PREFIXES = ("evidence/",)
FORBIDDEN_ACTION_PATTERNS = (
    ("gh-actions-command", r"\bgh\s+(?:run|workflow)\b"),
    ("github-action-reference", r"\bactions/[A-Za-z0-9_.-]+"),
    ("action-only-environment", r"\bGITHUB_(?:ENV|OUTPUT|STEP_SUMMARY)\b"),
)
HEADER_ROUTE_ORDER = (
    "/web",
    "/web/*",
    "/web_react",
    "/web_react/*",
    "/pkg/*",
    "/evidence/*",
)
REQUIRED_BUNDLE_FILES = (
    "frankenmermaid_demo_showcase.html",
    "web/index.html",
    "web_react/index.html",
    "pkg/frankenmermaid.d.ts",
    "pkg/frankenmermaid.js",
    "pkg/frankenmermaid_bg.wasm",
    "pkg/frankenmermaid_bg.wasm.d.ts",
    "pkg/package.json",
    "evidence/capability_scenario_matrix.json",
)


def sha256_file(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def tracked_repo_paths(repo_root: Path) -> list[Path]:
    """Return Git-tracked paths so policy decisions cannot be bypassed by ignores."""
    result = subprocess.run(
        ["git", "-C", str(repo_root), "ls-files", "-z"],
        check=False,
        capture_output=True,
    )
    if result.returncode != 0:
        raise RuntimeError("Git tracked-file inventory failed; release policy must fail closed")
    return [Path(item.decode("utf-8")) for item in result.stdout.split(b"\0") if item]


def explicitly_non_executable_historical_evidence(relative_path: Path, text: str) -> bool:
    """Allow historical evidence only when the file labels itself non-executable."""
    return (
        relative_path.as_posix().startswith(HISTORICAL_EVIDENCE_PREFIXES)
        and "non-executable historical evidence" in text[:4096].lower()
    )


def is_tracked_workflow_path(relative_path: Path) -> bool:
    parts = relative_path.parts
    return any(parts[index : index + 2] == (".github", "workflows") for index in range(len(parts) - 1)) and (
        relative_path.suffix in {".yml", ".yaml"}
    )


def scan_github_actions_policy(repo_root: Path) -> dict[str, object]:
    """Find tracked operational GitHub Actions dependencies before a local release."""
    violations: list[dict[str, object]] = []
    for relative_path in tracked_repo_paths(repo_root):
        rendered_path = relative_path.as_posix()
        if is_tracked_workflow_path(relative_path):
            violations.append(
                {
                    "kind": "tracked-workflow",
                    "path": rendered_path,
                    "line": None,
                    "detail": "tracked GitHub Actions workflow YAML is forbidden; owner approval is required before deletion",
                }
            )

        source_path = repo_root / relative_path
        if (
            rendered_path == "AGENTS.md"
            or not source_path.is_file()
            or source_path.suffix not in {".md", ".py", ".mjs", ".sh", ".yml", ".yaml"}
        ):
            continue
        try:
            source_text = source_path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if explicitly_non_executable_historical_evidence(relative_path, source_text):
            continue
        for line_number, line in enumerate(source_text.splitlines(), start=1):
            for kind, pattern in FORBIDDEN_ACTION_PATTERNS:
                if re.search(pattern, line):
                    violations.append(
                        {
                            "kind": kind,
                            "path": rendered_path,
                            "line": line_number,
                            "detail": f"forbidden GitHub Actions dependency: {line.strip()}",
                        }
                    )

    return {
        "action": "policy-scan",
        "repo_root": str(repo_root),
        "ok": not violations,
        "violations": violations,
        "required_remediation": (
            "Remove operational GitHub Actions dependencies and use /dsr plus Wrangler. "
            "Tracked workflow deletion requires the repository owner's explicit written approval."
        ),
    }


def build_bundle_file_map(repo_root: Path) -> dict[Path, Path]:
    mapping: dict[Path, Path] = {}
    for relative in REQUIRED_BUNDLE_FILES:
        rel_path = Path(relative)
        mapping[repo_root / rel_path] = rel_path
    return mapping


def merge_headers(static_headers: Path, react_headers: Path) -> dict[str, dict[str, str]]:
    merged: dict[str, dict[str, str]] = {}
    for manifest in (static_headers, react_headers):
        for route, headers in parse_headers_manifest(manifest.read_text()).items():
            merged.setdefault(route, {}).update(headers)
    return merged


def render_headers_manifest(rules: dict[str, dict[str, str]]) -> str:
    ordered_routes = list(HEADER_ROUTE_ORDER)
    ordered_routes.extend(sorted(route for route in rules if route not in HEADER_ROUTE_ORDER))
    blocks: list[str] = []
    for route in ordered_routes:
        if route not in rules:
            continue
        blocks.append(route)
        for header_name, value in sorted(rules[route].items()):
            rendered_name = "-".join(part.capitalize() for part in header_name.split("-"))
            blocks.append(f"  {rendered_name}: {value}")
        blocks.append("")
    return "\n".join(blocks).rstrip() + "\n"


def build_redirect_rules(canonical_host: str) -> list[dict[str, object]]:
    return [
        {
            "description": "Canonicalize /web/ to /web while preserving the query string.",
            "source_path": "/web/",
            "target_url": f"https://{canonical_host}/web",
            "status_code": 301,
            "preserve_query_string": True,
        },
        {
            "description": "Canonicalize /web_react/ to /web_react while preserving the query string.",
            "source_path": "/web_react/",
            "target_url": f"https://{canonical_host}/web_react",
            "status_code": 301,
            "preserve_query_string": True,
        },
    ]


def ensure_output_dir_safe(output_dir: Path, expected_relative_paths: set[Path]) -> None:
    if not output_dir.exists():
        return

    existing_files = {
        path.relative_to(output_dir)
        for path in output_dir.rglob("*")
        if path.is_file()
    }
    unexpected = sorted(str(path) for path in existing_files - expected_relative_paths)
    if unexpected:
        raise RuntimeError(
            "output directory contains unexpected files; choose a fresh staging directory instead: "
            + ", ".join(unexpected)
        )


def stage_bundle(
    *,
    repo_root: Path,
    output_dir: Path,
    canonical_host: str = DEFAULT_CANONICAL_HOST,
    static_headers: Path | None = None,
    react_headers: Path | None = None,
) -> dict[str, object]:
    static_headers_path = static_headers or repo_root / "web" / "_headers"
    react_headers_path = react_headers or repo_root / "web_react" / "_headers"

    hosting_plan = validate_cloudflare_hosting_plan(
        static_headers=static_headers_path,
        react_headers=react_headers_path,
        static_contract=repo_root / "evidence" / "contracts" / "showcase_static_entrypoint_contract.md",
        react_contract=repo_root / "evidence" / "contracts" / "showcase_react_embedding_contract.md",
        strategy_doc=repo_root / "evidence" / "demo_strategy.md",
    )

    bundle_map = build_bundle_file_map(repo_root)
    generated_headers_rel = Path("_headers")
    expected_paths = {dest for dest in bundle_map.values()}
    expected_paths.add(generated_headers_rel)
    ensure_output_dir_safe(output_dir, expected_paths)

    copied_files: list[dict[str, str]] = []
    for source_path, dest_rel in sorted(bundle_map.items(), key=lambda item: str(item[1])):
        if not source_path.exists():
            raise FileNotFoundError(f"required bundle source is missing: {source_path}")
        destination = output_dir / dest_rel
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source_path, destination)
        copied_files.append(
            {
                "source": str(source_path.relative_to(repo_root)),
                "destination": str(dest_rel),
                "sha256": sha256_file(destination),
            }
        )

    headers_manifest = render_headers_manifest(merge_headers(static_headers_path, react_headers_path))
    generated_headers = output_dir / generated_headers_rel
    generated_headers.parent.mkdir(parents=True, exist_ok=True)
    generated_headers.write_text(headers_manifest)

    return {
        "action": "stage-bundle",
        "bundle_root": str(output_dir),
        "canonical_host": canonical_host,
        "hosting_plan_gate": hosting_plan,
        "copied_files": copied_files,
        "generated_files": [
            {
                "path": str(generated_headers_rel),
                "sha256": sha256_file(generated_headers),
            }
        ],
        "redirect_strategy": {
            "kind": "cloudflare-redirect-rules",
            "rules": build_redirect_rules(canonical_host),
            "reason": (
                "Pages `_redirects` does not expose a preserve-query-string control, so canonical "
                "trailing-slash redirects for `/web/` and `/web_react/` are modeled as zone-level "
                "Redirect Rules or Bulk Redirects."
            ),
        },
    }


def build_pages_project_create_command(
    *,
    project_name: str,
    production_branch: str,
    compatibility_date: str,
) -> list[str]:
    return [
        "wrangler",
        "pages",
        "project",
        "create",
        project_name,
        "--production-branch",
        production_branch,
        "--compatibility-date",
        compatibility_date,
    ]


def build_pages_deploy_command(
    *,
    directory: Path,
    project_name: str,
    branch: str | None,
    commit_hash: str | None,
    commit_message: str | None,
    commit_dirty: bool,
) -> list[str]:
    command = [
        "wrangler",
        "pages",
        "deploy",
        str(directory),
        "--project-name",
        project_name,
    ]
    if branch:
        command.extend(["--branch", branch])
    if commit_hash:
        command.extend(["--commit-hash", commit_hash])
    if commit_message:
        command.extend(["--commit-message", commit_message])
    if commit_dirty:
        command.append("--commit-dirty")
    return command


def build_deployments_api_request(*, account_id: str, project_name: str) -> dict[str, object]:
    return {
        "method": "GET",
        "url": f"https://api.cloudflare.com/client/v4/accounts/{account_id}/pages/projects/{project_name}/deployments",
        "required_env": ["CLOUDFLARE_API_TOKEN"],
        "docs_url": PAGES_API_DOCS_URL,
    }


def build_rollback_drill(
    *,
    account_id: str,
    project_name: str,
    deployment_id: str,
    reason: str,
) -> dict[str, object]:
    return {
        "action": "rollback-drill",
        "supported_execution": "dashboard",
        "project_name": project_name,
        "account_id": account_id,
        "target_deployment_id": deployment_id,
        "reason": reason,
        "eligibility_rules": [
            "Only production deployments that completed successfully are valid rollback targets.",
            "Preview deployments are not valid rollback targets.",
            "Roll-forward is a fresh production deploy using the same staged bundle path and commit metadata flow.",
        ],
        "preflight_request": build_deployments_api_request(account_id=account_id, project_name=project_name),
        "docs": {
            "rollbacks": PAGES_ROLLBACK_DOCS_URL,
            "api": PAGES_API_DOCS_URL,
        },
    }


def run_subprocess(command: list[str], *, env: dict[str, str] | None = None) -> dict[str, object]:
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        env=env,
    )
    return {
        "command": command,
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def run_json_subprocess(
    command: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
) -> dict[str, object]:
    result = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        cwd=cwd,
        env=env,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip() or f"command failed: {' '.join(command)}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"command did not emit valid JSON: {' '.join(command)}") from exc


def emit_json(payload: dict[str, object]) -> int:
    print(json.dumps(payload, indent=2))
    return 0


def resolve_repo_path(repo_root: Path, candidate: str) -> Path:
    path = Path(candidate)
    return path if path.is_absolute() else repo_root / path


def build_route_integrity_report(stage_payload: dict[str, object]) -> dict[str, object]:
    copied_destinations = {
        str(item["destination"])
        for item in stage_payload.get("copied_files", [])
        if isinstance(item, dict) and "destination" in item
    }
    generated_paths = {
        str(item["path"])
        for item in stage_payload.get("generated_files", [])
        if isinstance(item, dict) and "path" in item
    }
    redirect_rules = list(stage_payload.get("redirect_strategy", {}).get("rules", []))
    expected_redirect_targets = {
        "/web/": "https://frankenmermaid.com/web",
        "/web_react/": "https://frankenmermaid.com/web_react",
    }

    def record(name: str, ok: bool, detail: str) -> dict[str, object]:
        return {"name": name, "ok": ok, "detail": detail}

    checks = [
        record(
            "static entry",
            "web/index.html" in copied_destinations,
            "staged bundle contains the /web entry document",
        ),
        record(
            "react entry",
            "web_react/index.html" in copied_destinations,
            "staged bundle contains the /web_react entry document",
        ),
        record(
            "shared showcase",
            "frankenmermaid_demo_showcase.html" in copied_destinations,
            "staged bundle contains the shared standalone showcase snapshot",
        ),
        record(
            "runtime assets",
            "pkg/frankenmermaid.js" in copied_destinations and "pkg/frankenmermaid_bg.wasm" in copied_destinations,
            "staged bundle contains the checked-in JS and WASM runtime artifacts",
        ),
        record(
            "capability artifact",
            "evidence/capability_scenario_matrix.json" in copied_destinations,
            "staged bundle contains the capability matrix artifact required by the hosted showcase",
        ),
        record(
            "merged headers",
            "_headers" in generated_paths,
            "staged bundle emits a root-level _headers manifest for the hosted routes",
        ),
        record(
            "redirect rules",
            len(redirect_rules) == 2
            and all(
                isinstance(rule, dict)
                and rule.get("source_path") in expected_redirect_targets
                and rule.get("target_url") == expected_redirect_targets[rule["source_path"]]
                and rule.get("preserve_query_string") is True
                for rule in redirect_rules
            ),
            "redirect rules preserve the query string for /web/ and /web_react/ canonicalization",
        ),
        record(
            "hosting plan precondition",
            isinstance(stage_payload.get("hosting_plan_gate"), dict)
            and stage_payload["hosting_plan_gate"].get("surface") == "cloudflare-hosting-plan",
            "staging still requires the checked-in hosting-plan validator to pass first",
        ),
    ]
    return {"ok": all(check["ok"] for check in checks), "checks": checks}


def _deploy_smoke_command(
    *,
    bead_id: str,
    repo_root: Path,
    serve_root: Path,
    output_root: Path,
    chromium: str,
    timeout_seconds: int,
    repeat: int,
    route_prefix: str,
    surface: str,
    host_kind: str,
    scenario_prefix: str,
    replay_bundle_dir: Path,
    scenario_ids: tuple[str, ...],
    profile_ids: tuple[str, ...],
) -> list[str]:
    command = [
        "python3",
        str(repo_root / "scripts" / "run_static_web_e2e.py"),
        "--bead-id",
        bead_id,
        "--repo-root",
        str(repo_root),
        "--serve-root",
        str(serve_root),
        "--output-root",
        str(output_root),
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
        "--replay-bundle-dir",
        str(replay_bundle_dir),
    ]
    for scenario_id in scenario_ids:
        command.extend(["--scenario-id", scenario_id])
    for profile_id in profile_ids:
        command.extend(["--profile-id", profile_id])
    return command


def run_deploy_smoke(
    *,
    repo_root: Path,
    stage_dir: Path,
    output_root: Path,
    chromium: str,
    timeout_seconds: int,
    repeat: int,
    scenario_ids: tuple[str, ...],
    profile_ids: tuple[str, ...],
    allowed_metric_delta_ms: int,
) -> dict[str, object]:
    stage_payload = stage_bundle(repo_root=repo_root, output_dir=stage_dir)
    route_integrity = build_route_integrity_report(stage_payload)

    static_root = output_root / "static"
    react_root = output_root / "react"
    static_replay_root = output_root / "static-replay"
    react_replay_root = output_root / "react-replay"

    static_summary = run_json_subprocess(
        _deploy_smoke_command(
            bead_id="bd-2u0.5.9.3-static",
            repo_root=repo_root,
            serve_root=stage_dir,
            output_root=static_root,
            chromium=chromium,
            timeout_seconds=timeout_seconds,
            repeat=repeat,
            route_prefix="/web",
            surface="web",
            host_kind="static-web",
            scenario_prefix="static-web",
            replay_bundle_dir=static_replay_root,
            scenario_ids=scenario_ids,
            profile_ids=profile_ids,
        ),
        cwd=repo_root,
    )
    react_summary = run_json_subprocess(
        _deploy_smoke_command(
            bead_id="bd-2u0.5.9.3-react",
            repo_root=repo_root,
            serve_root=stage_dir,
            output_root=react_root,
            chromium=chromium,
            timeout_seconds=timeout_seconds,
            repeat=repeat,
            route_prefix="/web_react",
            surface="web_react",
            host_kind="react-web",
            scenario_prefix="react-web",
            replay_bundle_dir=react_replay_root,
            scenario_ids=scenario_ids,
            profile_ids=profile_ids,
        ),
        cwd=repo_root,
    )

    static_summary_path = resolve_repo_path(repo_root, str(static_summary["summary_path"]))
    react_summary_path = resolve_repo_path(repo_root, str(react_summary["summary_path"]))
    static_validation = validate_e2e_summary(
        summary_path=static_summary_path,
        repo_root=repo_root,
        require_replay_bundle=True,
    )
    react_validation = validate_e2e_summary(
        summary_path=react_summary_path,
        repo_root=repo_root,
        require_replay_bundle=True,
    )

    parity_report = compare_host_parity(
        static_root=static_root,
        react_root=react_root,
        allowed_metric_delta_ms=allowed_metric_delta_ms,
    )
    parity_report_path = output_root / "deploy-smoke-parity.json"
    parity_report_path.parent.mkdir(parents=True, exist_ok=True)
    parity_report_path.write_text(json.dumps(parity_report, indent=2) + "\n")

    smoke_checks = [
        {
            "name": "route integrity",
            "ok": route_integrity["ok"],
            "detail": "staged bundle contains expected routes, assets, headers, and canonical redirect rules",
        },
        {
            "name": "static summary",
            "ok": static_validation["has_replay_bundle"],
            "detail": "static /web staged-bundle replay summary validated with replay metadata",
        },
        {
            "name": "react summary",
            "ok": react_validation["has_replay_bundle"],
            "detail": "react /web_react staged-bundle replay summary validated with replay metadata",
        },
        {
            "name": "parity",
            "ok": parity_report["ok"],
            "detail": "static and React staged-bundle smoke logs remain aligned within the parity tolerance",
        },
        {
            "name": "static normalized determinism",
            "ok": all(item["stable_normalized_log"] for item in static_summary["determinism"]),
            "detail": "static /web staged-bundle replay kept normalized smoke outcomes stable across repeats",
        },
        {
            "name": "react normalized determinism",
            "ok": all(item["stable_normalized_log"] for item in react_summary["determinism"]),
            "detail": "react /web_react staged-bundle replay kept normalized smoke outcomes stable across repeats",
        },
    ]

    return {
        "schema_version": 1,
        "bead_id": "bd-2u0.5.9.3",
        "scenario_id": "cloudflare-deploy-smoke",
        "ok": all(check["ok"] for check in smoke_checks),
        "stage_dir": str(stage_dir),
        "output_root": str(output_root),
        "chromium": chromium,
        "timeout_seconds": timeout_seconds,
        "repeat": repeat,
        "scenario_ids": list(scenario_ids),
        "profile_ids": list(profile_ids),
        "route_integrity": route_integrity,
        "static_summary_path": str(static_summary_path.relative_to(repo_root)),
        "react_summary_path": str(react_summary_path.relative_to(repo_root)),
        "static_validation": static_validation,
        "react_validation": react_validation,
        "parity_report_path": str(parity_report_path.relative_to(repo_root)),
        "parity": parity_report,
        "stage_bundle": stage_payload,
        "checks": smoke_checks,
    }


def cmd_stage_bundle(args: argparse.Namespace) -> int:
    payload = stage_bundle(
        repo_root=Path(args.repo_root),
        output_dir=Path(args.output_dir),
        canonical_host=args.canonical_host,
        static_headers=Path(args.static_headers),
        react_headers=Path(args.react_headers),
    )
    return emit_json(payload)


def _build_deploy_payload(args: argparse.Namespace, branch: str | None) -> dict[str, object]:
    staged_bundle = stage_bundle(
        repo_root=Path(args.repo_root),
        output_dir=Path(args.output_dir),
        canonical_host=args.canonical_host,
        static_headers=Path(args.static_headers),
        react_headers=Path(args.react_headers),
    )
    command = build_pages_deploy_command(
        directory=Path(args.output_dir),
        project_name=args.project_name,
        branch=branch,
        commit_hash=args.commit_hash,
        commit_message=args.commit_message,
        commit_dirty=args.commit_dirty,
    )
    return {
        "action": "preview-deploy" if branch else "production-deploy",
        "dry_run": args.dry_run,
        "docs_url": PAGES_WRANGLER_DOCS_URL,
        "staged_bundle": staged_bundle,
        "command": command,
    }


def cmd_preview_deploy(args: argparse.Namespace) -> int:
    payload = _build_deploy_payload(args, args.branch)
    if args.dry_run:
        return emit_json(payload)
    result = run_subprocess(payload["command"])
    payload["result"] = result
    return emit_json(payload)


def cmd_production_deploy(args: argparse.Namespace) -> int:
    payload = _build_deploy_payload(args, None)
    if args.dry_run:
        return emit_json(payload)
    result = run_subprocess(payload["command"])
    payload["result"] = result
    return emit_json(payload)


def cmd_create_project(args: argparse.Namespace) -> int:
    command = build_pages_project_create_command(
        project_name=args.project_name,
        production_branch=args.production_branch,
        compatibility_date=args.compatibility_date,
    )
    payload = {
        "action": "create-project",
        "dry_run": args.dry_run,
        "docs_url": PAGES_WRANGLER_DOCS_URL,
        "command": command,
    }
    if args.dry_run:
        return emit_json(payload)
    payload["result"] = run_subprocess(command)
    return emit_json(payload)


def cmd_list_deployments(args: argparse.Namespace) -> int:
    request = build_deployments_api_request(account_id=args.account_id, project_name=args.project_name)
    payload = {
        "action": "list-deployments",
        "dry_run": args.dry_run,
        "request": request,
    }
    if args.dry_run:
        return emit_json(payload)

    api_token = os.environ.get("CLOUDFLARE_API_TOKEN")
    if not api_token:
        raise RuntimeError("CLOUDFLARE_API_TOKEN is required for list-deployments without --dry-run")
    command = [
        "curl",
        request["url"],
        "--request",
        "GET",
        "--header",
        f"Authorization: Bearer {api_token}",
    ]
    payload["result"] = run_subprocess(command)
    return emit_json(payload)


def cmd_rollback_drill(args: argparse.Namespace) -> int:
    payload = build_rollback_drill(
        account_id=args.account_id,
        project_name=args.project_name,
        deployment_id=args.deployment_id,
        reason=args.reason,
    )
    payload["dry_run"] = args.dry_run
    if args.dry_run:
        return emit_json(payload)

    api_token = os.environ.get("CLOUDFLARE_API_TOKEN")
    if api_token:
        request = payload["preflight_request"]
        command = [
            "curl",
            request["url"],
            "--request",
            "GET",
            "--header",
            f"Authorization: Bearer {api_token}",
        ]
        payload["preflight_result"] = run_subprocess(command)
    return emit_json(payload)


def cmd_smoke_check(args: argparse.Namespace) -> int:
    repo_root = Path(args.repo_root).resolve()
    payload = run_deploy_smoke(
        repo_root=repo_root,
        stage_dir=(repo_root / args.stage_dir).resolve(),
        output_root=(repo_root / args.output_root).resolve(),
        chromium=args.chromium,
        timeout_seconds=args.timeout_seconds,
        repeat=args.repeat,
        scenario_ids=tuple(args.scenario_id or DEFAULT_SMOKE_SCENARIOS),
        profile_ids=tuple(args.profile_id or DEFAULT_SMOKE_PROFILES),
        allowed_metric_delta_ms=args.allowed_metric_delta_ms,
    )
    if args.report_out:
        report_path = (repo_root / args.report_out).resolve()
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps(payload, indent=2))
    return 0 if payload["ok"] else 1


def cmd_policy_scan(args: argparse.Namespace) -> int:
    payload = scan_github_actions_policy(Path(args.repo_root).resolve())
    print(json.dumps(payload, indent=2))
    return 0 if payload["ok"] else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Cloudflare Pages deployment helpers for the showcase")
    subparsers = parser.add_subparsers(dest="command", required=True)

    stage_bundle = subparsers.add_parser("stage-bundle", help="Stage the static Pages bundle for /web and /web_react")
    stage_bundle.add_argument("--repo-root", default=".", help="Repository root")
    stage_bundle.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Deploy bundle output directory")
    stage_bundle.add_argument("--canonical-host", default=DEFAULT_CANONICAL_HOST, help="Canonical host for redirect rules")
    stage_bundle.add_argument("--static-headers", default="web/_headers", help="Source static _headers file")
    stage_bundle.add_argument("--react-headers", default="web_react/_headers", help="Source React _headers file")
    stage_bundle.set_defaults(func=cmd_stage_bundle)

    create_project = subparsers.add_parser("create-project", help="Create the Pages project")
    create_project.add_argument("--project-name", default=DEFAULT_PROJECT_NAME, help="Pages project name")
    create_project.add_argument("--production-branch", default="main", help="Production branch")
    create_project.add_argument(
        "--compatibility-date",
        default=DEFAULT_COMPATIBILITY_DATE,
        help="Cloudflare compatibility date",
    )
    create_project.add_argument("--dry-run", action="store_true", help="Print the command without executing it")
    create_project.set_defaults(func=cmd_create_project)

    preview_deploy = subparsers.add_parser("preview-deploy", help="Stage and deploy a preview build")
    preview_deploy.add_argument("--repo-root", default=".", help="Repository root")
    preview_deploy.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Deploy bundle output directory")
    preview_deploy.add_argument("--project-name", default=DEFAULT_PROJECT_NAME, help="Pages project name")
    preview_deploy.add_argument("--branch", default=DEFAULT_PREVIEW_BRANCH, help="Preview branch name")
    preview_deploy.add_argument("--commit-hash", help="Attached commit hash")
    preview_deploy.add_argument("--commit-message", help="Attached commit message")
    preview_deploy.add_argument("--commit-dirty", action="store_true", help="Mark the deployment as dirty")
    preview_deploy.add_argument("--dry-run", action="store_true", help="Print the command without executing it")
    preview_deploy.add_argument("--canonical-host", default=DEFAULT_CANONICAL_HOST, help="Canonical host for redirect rules")
    preview_deploy.add_argument("--static-headers", default="web/_headers", help="Source static _headers file")
    preview_deploy.add_argument("--react-headers", default="web_react/_headers", help="Source React _headers file")
    preview_deploy.set_defaults(func=cmd_preview_deploy)

    production_deploy = subparsers.add_parser("production-deploy", help="Stage and deploy the production build")
    production_deploy.add_argument("--repo-root", default=".", help="Repository root")
    production_deploy.add_argument("--output-dir", default=str(DEFAULT_OUTPUT_DIR), help="Deploy bundle output directory")
    production_deploy.add_argument("--project-name", default=DEFAULT_PROJECT_NAME, help="Pages project name")
    production_deploy.add_argument("--commit-hash", help="Attached commit hash")
    production_deploy.add_argument("--commit-message", help="Attached commit message")
    production_deploy.add_argument("--commit-dirty", action="store_true", help="Mark the deployment as dirty")
    production_deploy.add_argument("--dry-run", action="store_true", help="Print the command without executing it")
    production_deploy.add_argument("--canonical-host", default=DEFAULT_CANONICAL_HOST, help="Canonical host for redirect rules")
    production_deploy.add_argument("--static-headers", default="web/_headers", help="Source static _headers file")
    production_deploy.add_argument("--react-headers", default="web_react/_headers", help="Source React _headers file")
    production_deploy.set_defaults(func=cmd_production_deploy)

    list_deployments = subparsers.add_parser("list-deployments", help="List Pages deployments through the REST API")
    list_deployments.add_argument("--account-id", required=True, help="Cloudflare account id")
    list_deployments.add_argument("--project-name", default=DEFAULT_PROJECT_NAME, help="Pages project name")
    list_deployments.add_argument("--dry-run", action="store_true", help="Print the request without executing it")
    list_deployments.set_defaults(func=cmd_list_deployments)

    rollback_drill = subparsers.add_parser(
        "rollback-drill",
        help="Print a rollback drill payload and optional deployment-list preflight",
    )
    rollback_drill.add_argument("--account-id", required=True, help="Cloudflare account id")
    rollback_drill.add_argument("--project-name", default=DEFAULT_PROJECT_NAME, help="Pages project name")
    rollback_drill.add_argument("--deployment-id", required=True, help="Production deployment id to target")
    rollback_drill.add_argument("--reason", required=True, help="Why the rollback drill is being prepared")
    rollback_drill.add_argument("--dry-run", action="store_true", help="Print the drill without calling the API")
    rollback_drill.set_defaults(func=cmd_rollback_drill)

    smoke_check = subparsers.add_parser(
        "smoke-check",
        help="Stage the Pages bundle and run route-integrity plus staged-bundle smoke checks for /web and /web_react",
    )
    smoke_check.add_argument("--repo-root", default=".", help="Repository root")
    smoke_check.add_argument("--stage-dir", default=str(DEFAULT_SMOKE_STAGE_DIR), help="Staging directory for the Pages bundle")
    smoke_check.add_argument(
        "--output-root",
        default=str(DEFAULT_SMOKE_OUTPUT_DIR),
        help="Evidence output directory for staged-bundle smoke results",
    )
    smoke_check.add_argument(
        "--chromium",
        default=os.environ.get("CHROMIUM_BIN", "/snap/bin/chromium"),
        help="Path to the Chromium binary for staged smoke replays",
    )
    smoke_check.add_argument("--timeout-seconds", type=int, default=8, help="Per-run virtual time budget")
    smoke_check.add_argument("--repeat", type=int, default=2, help="Repeat count for determinism-sensitive smoke scenarios")
    smoke_check.add_argument(
        "--scenario-id",
        action="append",
        help="Optional base scenario id filter (defaults to compare-export + diagnostics-recovery)",
    )
    smoke_check.add_argument(
        "--profile-id",
        action="append",
        help="Optional profile id filter (defaults to desktop-default)",
    )
    smoke_check.add_argument(
        "--allowed-metric-delta-ms",
        type=int,
        default=250,
        help="Maximum tolerated parse/layout/render delta for parity comparisons",
    )
    smoke_check.add_argument("--report-out", help="Optional path to persist the smoke summary JSON")
    smoke_check.set_defaults(func=cmd_smoke_check)

    policy_scan = subparsers.add_parser(
        "policy-scan",
        help="Fail closed on tracked GitHub Actions release dependencies",
    )
    policy_scan.add_argument("--repo-root", default=".", help="Repository root")
    policy_scan.set_defaults(func=cmd_policy_scan)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
