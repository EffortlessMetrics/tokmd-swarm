import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from release_consumer_smoke_aggregate import REQUIRED_SURFACES, aggregate_entries


def receipts(statuses):
    return [{"kind": surface, "status": status} for surface, status in statuses.items()]


def all_passed():
    return {surface: "passed" for surface in REQUIRED_SURFACES}


class AggregateContractTests(unittest.TestCase):
    def test_all_required_surfaces_pass(self):
        result = aggregate_entries(receipts(all_passed()), {}, "stable")
        self.assertEqual(result["overall"], "passed")

    def test_explicit_failure_blocks(self):
        statuses = all_passed()
        statuses["wasm"] = "failed"
        result = aggregate_entries(receipts(statuses), {}, "stable")
        self.assertEqual(result["overall"], "failed")

    def test_absent_receipt_blocks(self):
        statuses = all_passed()
        del statuses["nix"]
        result = aggregate_entries(receipts(statuses), {}, "stable")
        self.assertEqual(result["entries"]["nix"]["status"], "failed")
        self.assertEqual(result["overall"], "failed")

    def test_unavailable_required_surface_blocks(self):
        statuses = all_passed()
        statuses["nix"] = "unavailable"
        result = aggregate_entries(receipts(statuses), {}, "stable")
        self.assertEqual(result["overall"], "failed")

    def test_rc_policy_can_explicitly_exclude_not_supported_surface(self):
        statuses = all_passed()
        statuses["nix"] = "not_supported"
        result = aggregate_entries(receipts(statuses), {}, "rc", {"nix"})
        self.assertEqual(result["overall"], "passed")

    def test_stable_cannot_exclude_not_supported_surface(self):
        statuses = all_passed()
        statuses["nix"] = "not_supported"
        result = aggregate_entries(receipts(statuses), {}, "stable", {"nix"})
        self.assertEqual(result["overall"], "failed")


# Execute the real workflow run blocks with isolated fake child executables.
# These prove orchestration, not Cargo publishing or live registry authenticity.
import json
import os
import re
import shutil
import subprocess
import tempfile
import textwrap


@unittest.skipUnless(os.name == "posix" and shutil.which("bash") and shutil.which("jq"),
                     "release publisher shell contract requires Bash and jq")
class PublicationWorkflowTests(unittest.TestCase):
    def setUp(self):
        workflow = Path(__file__).resolve().parents[1] / ".github/workflows/release.yml"
        source = workflow.read_text(encoding="utf-8")
        match = re.search(r"(?ms)^  publish-crates:\n.*?(?=^  [a-z][a-z-]*:\n|\Z)", source)
        self.assertIsNotNone(match, "actual release workflow lost publish-crates")
        self.job = match.group(0)
        self.temp = tempfile.TemporaryDirectory(prefix="tokmd-publish-contract-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "target/publishing").mkdir(parents=True)
        tools = self.root / "tools"
        tools.mkdir()
        mock = textwrap.dedent('''\
            #!/usr/bin/env python3
            import json, os, sys
            from pathlib import Path
            tool = Path(sys.argv[0]).name
            args = sys.argv[1:]
            with Path("calls.jsonl").open("a", encoding="utf-8") as out:
                out.write(json.dumps([tool, args]) + "\\n")
            if tool == "git":
                print(os.environ.get("MOCK_SOURCE", os.environ["GITHUB_SHA"]))
            elif tool == "gh":
                if args[0] == "api":
                    print(os.environ.get("MOCK_ARTIFACTS", '[{"artifacts":[]}]'))
                    sys.exit(int(os.environ.get("MOCK_API_EXIT", "0")))
                else:
                    dest = Path(args[args.index("--dir") + 1])
                    dest.mkdir(parents=True)
                    (dest / "publish-receipt.json").write_text('{"restored":true}')
            elif "metadata" in args:
                print(json.dumps({"packages":[{"name":"tokmd", "version":os.environ.get("MOCK_VERSION", "1.15.1")}]}))
            elif "--registry-inventory" in args:
                path = Path(args[args.index("--registry-inventory") + 1])
                path.write_text('{"fixture":"registry"}')
                sys.exit(int(os.environ.get("MOCK_INVENTORY_EXIT", "0")))
            elif "--receipt" in args:
                path = Path(args[args.index("--receipt") + 1])
                path.write_text('{"fixture":"partial-or-complete"}')
                sys.exit(int(os.environ.get("MOCK_PUBLISH_EXIT", "0")))
            else:
                sys.exit("unexpected mock command")
        ''')
        for name in ("cargo", "git", "gh"):
            executable = tools / name
            executable.write_text(mock, encoding="utf-8")
            executable.chmod(0o755)
        self.env = {
            "PATH": str(tools) + os.pathsep + str(Path(sys.executable).parent) + os.pathsep + os.defpath,
            "GITHUB_SHA": "a" * 40,
            "GITHUB_REF_NAME": "v1.15.1",
            "GITHUB_REPOSITORY": "EffortlessMetrics/tokmd",
            "GITHUB_RUN_ID": "123",
            "GITHUB_RUN_ATTEMPT": "1",
        }

    def block(self, name):
        marker = "      - name: " + name + "\n"
        self.assertIn(marker, self.job)
        return self.job.split(marker, 1)[1].split("\n      - ", 1)[0]

    def run_step(self, name):
        block = self.block(name)
        self.assertIn("        run: |\n", block)
        script = textwrap.dedent(block.split("        run: |\n", 1)[1])
        return subprocess.run(["bash", "-c", script], cwd=self.root, env=self.env,
                              text=True, capture_output=True, timeout=10)

    def calls(self):
        path = self.root / "calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_real_job_is_guarded_bounded_and_does_not_use_legacy_publisher(self):
        self.assertIn("github.repository == 'EffortlessMetrics/tokmd'", self.job)
        self.assertIn("!contains(github.ref_name, '-')", self.job)
        self.assertNotIn("publish-release-crates.sh", self.job)
        self.assertIn("ref: ${{ github.sha }}", self.job)
        self.assertIn("group: tokmd-stable-registry-publication", self.job)
        self.assertIn("cancel-in-progress: false", self.job)
        self.assertIn("contents: read", self.job)
        self.assertIn("actions: read", self.job)
        self.assertEqual(self.job.count("CARGO_REGISTRY_TOKEN:"), 1)
        self.assertIn("CARGO_REGISTRY_TOKEN:", self.block("Publish crates with identity-bound receipts"))
        caps = [int(value) for value in re.findall(r"timeout-minutes: (\d+)", self.job)]
        self.assertLess(sum(caps[1:]), caps[0], "step ceilings leave no job finalization margin")

    def test_release_download_excludes_publisher_receipts_on_rerun(self):
        workflow = Path(__file__).resolve().parents[1] / ".github/workflows/release.yml"
        source = workflow.read_text(encoding="utf-8")
        job = source.split("  create-release:\n", 1)[1].split("  publish-crates:\n", 1)[0]
        download = job.split("      - name: Download Artifacts\n", 1)[1].split("      - name:", 1)[0]
        self.assertIn("pattern: tokmd-*", download)
        self.assertIn("path: artifacts", download)
        from fnmatch import fnmatchcase
        for artifact in ("release-publish-" + "a" * 40 + "-1", "consumer-smoke-final"):
            self.assertFalse(fnmatchcase(artifact, "tokmd-*"))
        for artifact in ("tokmd-linux-amd64", "tokmd-windows-amd64.exe", "tokmd-wasm-v1.15.1"):
            self.assertTrue(fnmatchcase(artifact, "tokmd-*"))

    def test_identity_uses_locked_metadata_and_writes_observation(self):
        result = self.run_step("Verify committed publication identity")
        self.assertEqual(result.returncode, 0, result.stderr)
        observation = json.loads((self.root / "target/publishing/workflow-identity.json").read_text())
        self.assertEqual(observation["source_commit"], self.env["GITHUB_SHA"])
        self.assertEqual(observation["version"], "1.15.1")
        self.assertIn(["cargo", ["metadata", "--locked", "--no-deps", "--format-version", "1"]], self.calls())

    def test_identity_rejects_rc_malformed_version_and_source_mismatch(self):
        for overrides in ({"GITHUB_REF_NAME":"v1.15.1-rc.1"},
                          {"GITHUB_REF_NAME":"v01.15.1"},
                          {"MOCK_VERSION":"1.15.0"}, {"MOCK_SOURCE":"b" * 40}):
            with self.subTest(overrides=overrides):
                self.env.update(overrides)
                result = self.run_step("Verify committed publication identity")
                self.assertNotEqual(result.returncode, 0)
                for key in overrides:
                    if key == "GITHUB_REF_NAME":
                        self.env[key] = "v1.15.1"
                    else:
                        del self.env[key]
        self.assertFalse(any("publish" in args for tool, args in self.calls()))

    def test_fresh_publish_delegates_exact_locked_receipt_and_bootstrap_args(self):
        result = self.run_step("Publish crates with identity-bound receipts")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [["cargo", ["--locked", "xtask", "publish", "--receipt",
            "target/publishing/publish-receipt.json", "--bootstrap", "tokmd-types,tokmd-envelope", "--yes"]]])

    def test_restored_receipt_selects_resume(self):
        (self.root / "target/publishing/publish-receipt.json").write_text('{"fixture":true}')
        result = self.run_step("Publish crates with identity-bound receipts")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls()[0][1][-1], "--resume")

    def test_publish_failure_preserves_partial_receipt_and_nonzero_exit(self):
        self.env["MOCK_PUBLISH_EXIT"] = "7"
        result = self.run_step("Publish crates with identity-bound receipts")
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertTrue((self.root / "target/publishing/publish-receipt.json").is_file())

    def test_inventory_failure_is_not_hidden_and_upload_is_always_selected(self):
        self.env["MOCK_INVENTORY_EXIT"] = "9"
        result = self.run_step("Observe complete registry inventory")
        self.assertEqual(result.returncode, 9, result.stderr)
        self.assertTrue((self.root / "target/publishing/registry-inventory.json").is_file())
        self.assertIn("always() && steps.identity.outcome == 'success'",
                      self.block("Observe complete registry inventory"))
        upload = self.block("Upload publication evidence even after failure")
        self.assertIn("if: always()", upload)
        self.assertIn("publish-receipt.json", upload)
        self.assertIn("registry-inventory.json", upload)
        self.assertIn("${{ github.sha }}-${{ github.run_attempt }}", upload)
        self.assertNotIn("continue-on-error", self.job)

    def test_first_attempt_does_not_download_old_state(self):
        result = self.run_step("Restore prior attempt publication receipt")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [])

    def test_retry_with_no_prior_artifact_uses_live_reinspection(self):
        self.env["GITHUB_RUN_ATTEMPT"] = "2"
        result = self.run_step("Restore prior attempt publication receipt")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([args[0] for tool, args in self.calls() if tool == "gh"], ["api"])
        self.assertFalse((self.root / "target/publishing/publish-receipt.json").exists())

    def test_retry_restores_only_latest_prior_attempt_of_same_sha(self):
        self.env["GITHUB_RUN_ATTEMPT"] = "3"
        prefix = "release-publish-" + self.env["GITHUB_SHA"] + "-"
        self.env["MOCK_ARTIFACTS"] = json.dumps([{"artifacts":[
            {"name": prefix + "1", "expired":False},
            {"name": prefix + "2", "expired":False},
            {"name": prefix + "3", "expired":False},
            {"name": "release-publish-" + "b" * 40 + "-2", "expired":False}]}])
        result = self.run_step("Restore prior attempt publication receipt")
        self.assertEqual(result.returncode, 0, result.stderr)
        downloads = [args for tool, args in self.calls() if tool == "gh" and args[0] == "run"]
        self.assertEqual(len(downloads), 1)
        self.assertEqual(downloads[0][downloads[0].index("--name") + 1], prefix + "2")
        self.assertEqual(json.loads((self.root / "target/publishing/publish-receipt.json").read_text()),
                         {"restored":True})

    def test_expired_prior_receipt_and_failed_artifact_query_fail_closed(self):
        self.env["GITHUB_RUN_ATTEMPT"] = "2"
        self.env["MOCK_ARTIFACTS"] = json.dumps([{"artifacts":[{
            "name":"release-publish-" + self.env["GITHUB_SHA"] + "-1", "expired":True}]}])
        result = self.run_step("Restore prior attempt publication receipt")
        self.assertNotEqual(result.returncode, 0)
        self.env["MOCK_API_EXIT"] = "11"
        result = self.run_step("Restore prior attempt publication receipt")
        self.assertEqual(result.returncode, 11)
        self.assertFalse(any(args[0] == "run" for tool, args in self.calls() if tool == "gh"))


if __name__ == "__main__":
    unittest.main()
