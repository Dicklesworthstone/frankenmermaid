import importlib.util
import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


OPS_PATH = Path(__file__).resolve().parent.parent / "scripts" / "cloudflare_pages_ops.py"
OPS_SPEC = importlib.util.spec_from_file_location("cloudflare_pages_ops", OPS_PATH)
OPS = importlib.util.module_from_spec(OPS_SPEC)
assert OPS_SPEC.loader is not None
sys.modules[OPS_SPEC.name] = OPS
OPS_SPEC.loader.exec_module(OPS)


class CloudflarePagesOpsTests(unittest.TestCase):
    def test_render_headers_manifest_merges_static_and_react_rules(self):
        with TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            static_headers = root / "web_headers"
            react_headers = root / "react_headers"
            static_headers.write_text(
                "/pkg/*\n"
                "  Cache-Control: public, max-age=0, must-revalidate\n\n"
                "/web\n"
                "  Cache-Control: public, max-age=0, must-revalidate\n"
            )
            react_headers.write_text(
                "/pkg/*\n"
                "  Cache-Control: public, max-age=0, must-revalidate\n\n"
                "/web_react\n"
                "  Cache-Control: public, max-age=0, must-revalidate\n"
            )

            manifest = OPS.render_headers_manifest(OPS.merge_headers(static_headers, react_headers))
            self.assertIn("/web\n", manifest)
            self.assertIn("/web_react\n", manifest)
            self.assertIn("/pkg/*\n", manifest)

    def test_build_redirect_rules_preserves_query_string(self):
        rules = OPS.build_redirect_rules("frankenmermaid.com")
        self.assertEqual(len(rules), 2)
        self.assertTrue(all(rule["preserve_query_string"] for rule in rules))
        self.assertEqual(rules[0]["target_url"], "https://frankenmermaid.com/web")

    def test_stage_bundle_copies_required_files_and_generates_headers(self):
        repo_root = Path(__file__).resolve().parent.parent
        with TemporaryDirectory() as tempdir:
            output_dir = Path(tempdir) / "bundle"
            payload = OPS.stage_bundle(repo_root=repo_root, output_dir=output_dir)
            self.assertEqual(payload["action"], "stage-bundle")
            self.assertTrue((output_dir / "frankenmermaid_demo_showcase.html").exists())
            self.assertTrue((output_dir / "web" / "index.html").exists())
            self.assertTrue((output_dir / "_headers").exists())
            self.assertEqual(payload["redirect_strategy"]["kind"], "cloudflare-redirect-rules")

    def test_stage_bundle_rejects_unexpected_existing_files(self):
        repo_root = Path(__file__).resolve().parent.parent
        with TemporaryDirectory() as tempdir:
            output_dir = Path(tempdir) / "bundle"
            output_dir.mkdir(parents=True)
            (output_dir / "unexpected.txt").write_text("stale")
            with self.assertRaisesRegex(RuntimeError, "unexpected files"):
                OPS.stage_bundle(repo_root=repo_root, output_dir=output_dir)

    def test_build_route_integrity_report_accepts_valid_stage_bundle(self):
        repo_root = Path(__file__).resolve().parent.parent
        with TemporaryDirectory() as tempdir:
            output_dir = Path(tempdir) / "bundle"
            payload = OPS.stage_bundle(repo_root=repo_root, output_dir=output_dir)
            report = OPS.build_route_integrity_report(payload)
            self.assertTrue(report["ok"])
            self.assertEqual(len(report["checks"]), 8)

    def test_build_route_integrity_report_rejects_missing_redirect_rules(self):
        payload = {
            "copied_files": [
                {"destination": "web/index.html"},
                {"destination": "web_react/index.html"},
                {"destination": "frankenmermaid_demo_showcase.html"},
                {"destination": "pkg/frankenmermaid.js"},
                {"destination": "pkg/frankenmermaid_bg.wasm"},
                {"destination": "evidence/capability_scenario_matrix.json"},
            ],
            "generated_files": [{"path": "_headers"}],
            "redirect_strategy": {"rules": []},
            "hosting_plan_gate": {"surface": "cloudflare-hosting-plan"},
        }
        report = OPS.build_route_integrity_report(payload)
        self.assertFalse(report["ok"])
        failing = [check["name"] for check in report["checks"] if not check["ok"]]
        self.assertEqual(failing, ["redirect rules"])

    def test_build_pages_project_create_command_matches_wranger_surface(self):
        command = OPS.build_pages_project_create_command(
            project_name="frankenmermaid",
            production_branch="main",
            compatibility_date="2026-04-06",
        )
        self.assertEqual(
            command,
            [
                "wrangler",
                "pages",
                "project",
                "create",
                "frankenmermaid",
                "--production-branch",
                "main",
                "--compatibility-date",
                "2026-04-06",
            ],
        )

    def test_build_pages_deploy_command_supports_preview_metadata(self):
        command = OPS.build_pages_deploy_command(
            directory=Path("dist/cloudflare-pages/latest"),
            project_name="frankenmermaid",
            branch="preview-web",
            commit_hash="deadbeef",
            commit_message="preview smoke",
            commit_dirty=True,
        )
        self.assertIn("--branch", command)
        self.assertIn("preview-web", command)
        self.assertIn("--commit-hash", command)
        self.assertIn("--commit-dirty", command)

    def test_deploy_smoke_command_supports_staged_bundle_serving(self):
        command = OPS._deploy_smoke_command(
            bead_id="bd-2u0.5.9.3-static",
            repo_root=Path("/repo"),
            serve_root=Path("/repo/dist/cloudflare-pages/deploy-smoke-stage"),
            output_root=Path("/repo/evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static"),
            chromium="/snap/bin/chromium",
            timeout_seconds=8,
            repeat=2,
            route_prefix="/web",
            surface="web",
            host_kind="static-web",
            scenario_prefix="static-web",
            replay_bundle_dir=Path("/repo/evidence/runs/web/bd-2u0.5.9.3/deploy-smoke/static-replay"),
            scenario_ids=("static-web-compare-export",),
            profile_ids=("desktop-default",),
        )
        self.assertIn("--serve-root", command)
        self.assertIn("/repo/dist/cloudflare-pages/deploy-smoke-stage", command)
        self.assertIn("--replay-bundle-dir", command)

    def test_build_deployments_api_request_matches_cloudflare_docs(self):
        payload = OPS.build_deployments_api_request(account_id="acct", project_name="frankenmermaid")
        self.assertEqual(payload["method"], "GET")
        self.assertIn("/accounts/acct/pages/projects/frankenmermaid/deployments", payload["url"])
        self.assertEqual(payload["required_env"], ["CLOUDFLARE_API_TOKEN"])

    def test_build_rollback_drill_stays_honest_about_dashboard_execution(self):
        payload = OPS.build_rollback_drill(
            account_id="acct",
            project_name="frankenmermaid",
            deployment_id="deploy-123",
            reason="smoke",
        )
        self.assertEqual(payload["supported_execution"], "dashboard")
        self.assertEqual(payload["target_deployment_id"], "deploy-123")
        self.assertIn("Preview deployments are not valid rollback targets.", payload["eligibility_rules"])

    def test_policy_scan_rejects_tracked_workflows_and_action_references(self):
        repo_root = Path(__file__).resolve().parent.parent
        payload = OPS.scan_github_actions_policy(repo_root)
        self.assertFalse(payload["ok"])
        self.assertTrue(
            any(
                violation["kind"] == "tracked-workflow" and violation["path"] == ".github/workflows/ci.yml"
                for violation in payload["violations"]
            )
        )
        self.assertTrue(
            any(
                violation["kind"] == "github-action-reference" and violation["path"] == ".github/workflows/ci.yml"
                for violation in payload["violations"]
            )
        )
        self.assertTrue(
            any(
                violation["kind"] == "tracked-workflow"
                and violation["path"] == "extensions/vscode-frankenmermaid/.github/workflows/react-doctor.yml"
                for violation in payload["violations"]
            )
        )

    def test_stage_bundle_cli_emits_machine_readable_json(self):
        repo_root = Path(__file__).resolve().parent.parent
        with TemporaryDirectory() as tempdir:
            result = OPS.run_subprocess(
                [
                    "python3",
                    str(OPS_PATH),
                    "stage-bundle",
                    "--repo-root",
                    str(repo_root),
                    "--output-dir",
                    str(Path(tempdir) / "bundle"),
                ]
            )
            self.assertEqual(result["returncode"], 0)
            payload = json.loads(result["stdout"])
            self.assertEqual(payload["action"], "stage-bundle")
            self.assertTrue(payload["generated_files"][0]["path"].endswith("_headers"))


if __name__ == "__main__":
    unittest.main()
