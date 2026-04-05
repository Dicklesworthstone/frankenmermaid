import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from scripts.run_static_web_e2e import (
    build_replay_command,
    build_url,
    count_diagnostic_items,
    derive_degradation_tier,
    derive_runtime_mode,
    extract_determinism_report,
    normalized_log_signature,
    select_profiles,
    select_scenarios,
    write_replay_bundle,
)


class StaticWebE2eHelperTests(unittest.TestCase):
    def test_build_url_encodes_query(self):
        url = build_url("http://127.0.0.1:8080", "/web", {"sample": "flowchart-1", "lab": "cycles"})
        self.assertEqual(url, "http://127.0.0.1:8080/web?sample=flowchart-1&lab=cycles")

    def test_count_diagnostic_items(self):
        dom = '<div class="diagnostic-item"></div><div class="diagnostic-item"></div>'
        self.assertEqual(count_diagnostic_items(dom), 2)

    def test_derive_degradation_tier_prefers_fallback(self):
        dom = "Current revision is degraded and no prior healthy snapshot has been committed yet."
        self.assertEqual(derive_degradation_tier(dom), "fallback")

    def test_derive_runtime_mode_detects_live(self):
        self.assertEqual(derive_runtime_mode("live runtime"), "live")

    def test_normalized_log_signature_discards_run_specific_fields(self):
        signature = normalized_log_signature(
            {
                "scenario_id": "static-web-compare-export",
                "surface": "web",
                "renderer": "franken-svg",
                "theme": "corporate",
                "diagnostic_count": 2,
                "degradation_tier": "fallback",
                "runtime_mode": "live",
                "fallback_active": True,
                "profile": "desktop-default",
                "determinism_status": "stable",
                "trace_id": "ignore-me",
                "output_artifact_hash": "sha256:abc",
            }
        )
        self.assertEqual(
            signature,
            {
                "scenario_id": "static-web-compare-export",
                "surface": "web",
                "renderer": "franken-svg",
                "theme": "corporate",
                "diagnostic_count": 2,
                "degradation_tier": "fallback",
                "runtime_mode": "live",
                "fallback_active": True,
                "profile": "desktop-default",
                "determinism_status": "stable",
            },
        )

    def test_extract_determinism_report_from_dom(self):
        dom = """
        <div>before</div>
        <pre id="determinism-json">{
          &quot;stable&quot;: true,
          &quot;summary&quot;: &quot;Determinism check passed&quot;,
          &quot;runs&quot;: [{&quot;runIndex&quot;: 1, &quot;outputArtifactHash&quot;: &quot;sha256:abc&quot;}]
        }</pre>
        """
        report = extract_determinism_report(dom)
        self.assertIsNotNone(report)
        self.assertTrue(report["stable"])
        self.assertEqual(report["runs"][0]["outputArtifactHash"], "sha256:abc")

    def test_select_scenarios_rejects_unknown_ids(self):
        with self.assertRaisesRegex(ValueError, "unknown scenario ids: nope"):
            select_scenarios(["nope"])

    def test_select_profiles_rejects_unknown_ids(self):
        with self.assertRaisesRegex(ValueError, "unknown profile ids: nope"):
            select_profiles(["nope"])

    def test_build_replay_command_supports_filtered_react_replay(self):
        command = build_replay_command(
            bead_id="bd-2u0.5.11.4",
            repo_root=".",
            output_root="evidence/runs/web/bd-2u0.5.11.4",
            chromium="/snap/bin/chromium",
            timeout_seconds=8,
            route_prefix="/web_react",
            surface="web_react",
            host_kind="react-web",
            scenario_prefix="react-web",
            repeat=1,
            revision="deadbeef",
            scenario_id="static-web-determinism-check",
            profile_id="mobile-narrow",
        )
        self.assertIn("--scenario-id", command)
        self.assertIn("static-web-determinism-check", command)
        self.assertIn("--profile-id", command)
        self.assertIn("mobile-narrow", command)
        self.assertIn("--route-prefix", command)
        self.assertIn("/web_react", command)

    def test_write_replay_bundle_emits_manifest_and_script(self):
        with TemporaryDirectory() as tempdir:
            root = Path(tempdir)
            bundle = write_replay_bundle(
                bundle_dir=root / "bundle",
                bead_id="bd-2u0.5.11.4",
                repo_root=root,
                output_root=root / "evidence",
                chromium="/snap/bin/chromium",
                timeout_seconds=8,
                route_prefix="/web_react",
                surface="web_react",
                host_kind="react-web",
                scenario_prefix="react-web",
                revision="deadbeef",
                repeat=5,
                scenarios=select_scenarios(["static-web-compare-export"]),
                profiles=select_profiles(["desktop-default"]),
                summary_path=root / "summary.json",
            )
            manifest = (root / "bundle" / "replay_manifest.json").read_text()
            script = (root / "bundle" / "replay_suite.sh").read_text()
            self.assertEqual(bundle["manifest_path"], str(root / "bundle" / "replay_manifest.json"))
            self.assertIn("\"surface\": \"web_react\"", manifest)
            self.assertIn("react-web-compare-export", manifest)
            self.assertIn("python3 scripts/run_static_web_e2e.py", script)
            self.assertIn("--profile-id desktop-default", script)


if __name__ == "__main__":
    unittest.main()
