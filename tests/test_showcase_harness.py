import importlib.util
import json
import sys
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


HARNESS_PATH = Path(__file__).resolve().parent.parent / "scripts" / "showcase_harness.py"
SPEC = importlib.util.spec_from_file_location("showcase_harness", HARNESS_PATH)
HARNESS = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = HARNESS
SPEC.loader.exec_module(HARNESS)

RUNNER_PATH = Path(__file__).resolve().parent.parent / "scripts" / "run_static_web_e2e.py"
RUNNER_SPEC = importlib.util.spec_from_file_location("run_static_web_e2e", RUNNER_PATH)
RUNNER = importlib.util.module_from_spec(RUNNER_SPEC)
assert RUNNER_SPEC.loader is not None
sys.modules[RUNNER_SPEC.name] = RUNNER
RUNNER_SPEC.loader.exec_module(RUNNER)


class ShowcaseHarnessTests(unittest.TestCase):
    def test_validate_log_payload_accepts_current_static_web_log(self):
        log_path = (
            Path(__file__).resolve().parent.parent
            / "evidence"
            / "runs"
            / "web"
            / "bd-2u0.5.8.2.2"
            / "static-web-entrypoint"
            / "2026-03-29T01-43-37Z__evidence__log.json"
        )
        payload = json.loads(log_path.read_text())
        self.assertEqual(HARNESS.validate_log_payload(payload), [])

    def test_validate_log_payload_accepts_current_react_web_log(self):
        log_path = (
            Path(__file__).resolve().parent.parent
            / "evidence"
            / "runs"
            / "web"
            / "bd-2u0.5.8.3.2"
            / "react-web-entrypoint"
            / "2026-04-01T03-23-00Z__evidence__log.json"
        )
        payload = json.loads(log_path.read_text())
        self.assertEqual(HARNESS.validate_log_payload(payload), [])

    def test_validate_log_payload_rejects_missing_required_fields(self):
        payload = {
            "schema_version": 1,
            "surface": "web",
            "renderer": "franken-svg",
        }
        errors = HARNESS.validate_log_payload(payload)
        self.assertTrue(any("missing required fields" in error for error in errors))

    def test_extract_module_script_finds_web_bootstrap_script(self):
        html = (Path(__file__).resolve().parent.parent / "web" / "index.html").read_text()
        script = HARNESS.extract_module_script(html)
        self.assertIn("async function bootstrap()", script)

    def test_validate_static_web_smoke(self):
        root = Path(__file__).resolve().parent.parent
        result = HARNESS.validate_static_web(
            entry=root / "web" / "index.html",
            headers=root / "web" / "_headers",
            contract=root / "evidence" / "contracts" / "showcase_static_entrypoint_contract.md",
            log_path=root
            / "evidence"
            / "runs"
            / "web"
            / "bd-2u0.5.8.2.2"
            / "static-web-entrypoint"
            / "2026-03-29T01-43-37Z__evidence__log.json",
        )
        self.assertEqual(result["surface"], "web")
        self.assertTrue(result["entry_hash"].startswith("sha256:"))

    def test_validate_static_web_rejects_missing_fetch_bootstrap(self):
        with TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            entry = temp / "index.html"
            entry.write_text(
                "<!DOCTYPE html><html><body><script type=\"module\">document.write('x');</script></body></html>"
            )
            headers = temp / "_headers"
            headers.write_text("/pkg/*\n  Cache-Control: public, max-age=31536000, immutable\n")
            contract = temp / "contract.md"
            contract.write_text("root-level `/pkg/...` and `/evidence/...`")

            with self.assertRaises(RuntimeError):
                HARNESS.validate_static_web(entry=entry, headers=headers, contract=contract, log_path=None)

    def test_validate_react_web_smoke(self):
        root = Path(__file__).resolve().parent.parent
        result = HARNESS.validate_react_web(
            entry=root / "web_react" / "index.html",
            headers=root / "web_react" / "_headers",
            contract=root / "evidence" / "contracts" / "showcase_react_embedding_contract.md",
            log_path=None,
        )
        self.assertEqual(result["surface"], "web_react")
        self.assertTrue(result["entry_hash"].startswith("sha256:"))

    def test_validate_react_web_rejects_missing_host_markers(self):
        with TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            entry = temp / "index.html"
            entry.write_text(
                "<!DOCTYPE html><html><body><main id=\"showcase-react-root\"></main><script type=\"module\">async function bootstrapReactHost(){ const response = await fetch('../frankenmermaid_demo_showcase.html'); document.write(await response.text()); }</script></body></html>"
            )
            headers = temp / "_headers"
            headers.write_text("/web_react\n  Cache-Control: public, max-age=0, must-revalidate\n")
            contract = temp / "contract.md"
            contract.write_text("`/web_react` should implement the `/web_react` route against this component/service boundary.")

            with self.assertRaises(RuntimeError):
                HARNESS.validate_react_web(entry=entry, headers=headers, contract=contract, log_path=None)

    def test_validate_showcase_accessibility_smoke(self):
        root = Path(__file__).resolve().parent.parent
        result = HARNESS.validate_showcase_accessibility(
            entry=root / "frankenmermaid_demo_showcase.html",
            log_path=None,
        )
        self.assertEqual(result["surface"], "standalone")
        self.assertTrue(result["entry_hash"].startswith("sha256:"))

    def test_validate_showcase_accessibility_rejects_missing_skip_link(self):
        with TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            entry = temp / "showcase.html"
            entry.write_text("<!DOCTYPE html><html><body><main id=\"main-content\"></main></body></html>")

            with self.assertRaises(RuntimeError):
                HARNESS.validate_showcase_accessibility(entry=entry, log_path=None)

    def test_validate_showcase_compatibility_smoke(self):
        root = Path(__file__).resolve().parent.parent
        result = HARNESS.validate_showcase_compatibility(
            entry=root / "frankenmermaid_demo_showcase.html",
            log_path=None,
        )
        self.assertEqual(result["surface"], "standalone")
        self.assertTrue(result["entry_hash"].startswith("sha256:"))

    def test_validate_showcase_compatibility_rejects_missing_fallbacks(self):
        with TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            entry = temp / "showcase.html"
            entry.write_text("<!DOCTYPE html><html><body><main id=\"main-content\"></main></body></html>")

            with self.assertRaises(RuntimeError):
                HARNESS.validate_showcase_compatibility(entry=entry, log_path=None)

    def test_build_url_supports_web_react_prefix(self):
        url = RUNNER.build_url(
            "http://127.0.0.1:8123",
            "/web_react",
            {"sample": "flowchart-1-incident-response-escalation", "lab": "cycles"},
        )
        self.assertEqual(
            url,
            "http://127.0.0.1:8123/web_react?sample=flowchart-1-incident-response-escalation&lab=cycles",
        )

    def test_build_log_supports_web_react_surface_and_host_kind(self):
        scenario = RUNNER.Scenario(
            scenario_id="react-web-compare-export",
            query={},
            required_substrings=(),
            pass_reason="/web_react host restored compare state",
        )
        profile = RUNNER.RunProfile(profile_id="desktop-default", chromium_flags=())
        log = RUNNER.build_log(
            bead_id="bd-2u0.5.8.3.3",
            scenario=scenario,
            profile=profile,
            run_index=1,
            url="http://127.0.0.1:8123/web_react?sample=x",
            dom="<html><body>healthy runtime</body></html>",
            revision="deadbeef",
            script_hash="sha256:abc",
            output_hash="sha256:def",
            surface="web_react",
            host_kind="react-web",
            determinism_report=None,
        )
        self.assertEqual(log["surface"], "web_react")
        self.assertEqual(log["host_kind"], "react-web")
        self.assertEqual(log["scenario_id"], "react-web-compare-export")
        self.assertEqual(HARNESS.validate_log_payload(log), [])

    def test_compare_host_parity_accepts_expected_wrapper_differences(self):
        with TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            static_dir = root / "static"
            react_dir = root / "react"
            static_dir.mkdir()
            react_dir.mkdir()

            static_log = {
                "schema_version": 1,
                "bead_id": "bd-2u0.5.8.4",
                "scenario_id": "static-web-compare-export",
                "input_hash": "sha256:input-static",
                "surface": "web",
                "renderer": "franken-svg",
                "theme": "corporate",
                "config_hash": "sha256:config-static",
                "parse_ms": 19,
                "layout_ms": 34,
                "render_ms": 55,
                "diagnostic_count": 2,
                "degradation_tier": "fallback",
                "output_artifact_hash": "sha256:output-static",
                "pass_fail_reason": "Static /web host restored compare state",
                "run_kind": "e2e",
                "trace_id": "trace-static",
                "revision": "rev-static",
                "host_kind": "static-web",
                "fallback_active": True,
                "runtime_mode": "live",
                "profile": "desktop-default",
                "determinism_status": "unreported",
            }
            react_log = {
                **static_log,
                "scenario_id": "react-web-compare-export",
                "surface": "web_react",
                "input_hash": "sha256:input-react",
                "config_hash": "sha256:config-react",
                "parse_ms": 41,
                "layout_ms": 60,
                "render_ms": 78,
                "output_artifact_hash": "sha256:output-react",
                "pass_fail_reason": "/web_react host restored compare state",
                "trace_id": "trace-react",
                "revision": "rev-react",
                "host_kind": "react-web",
            }

            (static_dir / "2026-04-01T18-00-00Z__e2e__log.json").write_text(json.dumps(static_log))
            (react_dir / "2026-04-01T18-00-01Z__e2e__log.json").write_text(json.dumps(react_log))

            report = HARNESS.compare_host_parity(static_root=static_dir, react_root=react_dir)
            self.assertTrue(report["ok"])
            self.assertEqual(report["pair_count"], 1)
            self.assertEqual(report["pairs"][0]["scenario_id"], "compare-export")

    def test_compare_host_parity_rejects_strict_behavior_drift(self):
        with TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            static_dir = root / "static"
            react_dir = root / "react"
            static_dir.mkdir()
            react_dir.mkdir()

            static_log = {
                "schema_version": 1,
                "bead_id": "bd-2u0.5.8.4",
                "scenario_id": "static-web-determinism-check",
                "input_hash": "sha256:input-static",
                "surface": "web",
                "renderer": "franken-svg",
                "theme": "corporate",
                "config_hash": "sha256:config-static",
                "parse_ms": 10,
                "layout_ms": 11,
                "render_ms": 12,
                "diagnostic_count": 1,
                "degradation_tier": "healthy",
                "output_artifact_hash": "sha256:output-static",
                "pass_fail_reason": "Static /web host reran determinism",
                "run_kind": "e2e",
                "trace_id": "trace-static",
                "revision": "rev-static",
                "host_kind": "static-web",
                "fallback_active": False,
                "runtime_mode": "live",
                "profile": "desktop-default",
                "determinism_status": "stable",
            }
            react_log = {
                **static_log,
                "scenario_id": "react-web-determinism-check",
                "surface": "web_react",
                "host_kind": "react-web",
                "diagnostic_count": 3,
            }

            (static_dir / "2026-04-01T18-01-00Z__e2e__log.json").write_text(json.dumps(static_log))
            (react_dir / "2026-04-01T18-01-01Z__e2e__log.json").write_text(json.dumps(react_log))

            report = HARNESS.compare_host_parity(static_root=static_dir, react_root=react_dir)
            self.assertFalse(report["ok"])
            self.assertEqual(report["failing_pairs"], ["determinism-check/desktop-default"])

    def test_validate_e2e_summary_accepts_current_react_release_bundle(self):
        root = Path(__file__).resolve().parent.parent
        result = HARNESS.validate_e2e_summary(
            summary_path=root
            / "evidence"
            / "runs"
            / "web"
            / "bd-2u0.5.11.4"
            / "react"
            / "2026-04-01T17-54-52Z__determinism__summary.json",
            repo_root=root,
            require_replay_bundle=True,
        )
        self.assertEqual(result["surface"], "web_react")
        self.assertTrue(result["has_replay_bundle"])

    def test_validate_e2e_summary_rejects_missing_replay_bundle_when_required(self):
        with TemporaryDirectory() as tempdir:
            temp = Path(tempdir)
            html_path = temp / "run.html"
            log_path = temp / "run.json"
            html_path.write_text("<html></html>")
            log_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "bead_id": "bd-x",
                        "scenario_id": "react-web-determinism-check",
                        "input_hash": "sha256:a",
                        "surface": "web_react",
                        "renderer": "franken-svg",
                        "theme": "corporate",
                        "config_hash": "sha256:b",
                        "parse_ms": 1,
                        "layout_ms": 1,
                        "render_ms": 1,
                        "diagnostic_count": 1,
                        "degradation_tier": "healthy",
                        "output_artifact_hash": "sha256:c",
                        "pass_fail_reason": "ok",
                        "run_kind": "e2e",
                        "trace_id": "trace",
                        "revision": "rev",
                        "host_kind": "react-web",
                        "fallback_active": False,
                        "runtime_mode": "live",
                    }
                )
            )
            summary_path = temp / "summary.json"
            summary_path.write_text(
                json.dumps(
                    {
                        "ok": True,
                        "route_prefix": "/web_react",
                        "surface": "web_react",
                        "host_kind": "react-web",
                        "repeat": 1,
                        "profiles": ["desktop-default"],
                        "scenarios": ["react-web-determinism-check"],
                        "results": [
                            {
                                "scenario_id": "react-web-determinism-check",
                                "profile": "desktop-default",
                                "run_index": 1,
                                "html_path": str(html_path),
                                "log_path": str(log_path),
                                "diagnostic_count": 1,
                                "degradation_tier": "healthy",
                                "runtime_mode": "live",
                                "output_artifact_hash": "sha256:c",
                            }
                        ],
                        "determinism": [
                            {
                                "scenario_id": "react-web-determinism-check",
                                "profile": "desktop-default",
                                "runs": 1,
                                "stable_output_hash": True,
                                "stable_normalized_log": True,
                                "output_hashes": ["sha256:c"],
                            }
                        ],
                    }
                )
            )

            with self.assertRaises(RuntimeError):
                HARNESS.validate_e2e_summary(summary_path=summary_path, repo_root=temp, require_replay_bundle=True)


if __name__ == "__main__":
    unittest.main()
