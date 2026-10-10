import json
import os
import subprocess
import tempfile
import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from release_consumer_smoke_aggregate import JOB_SURFACES, REQUIRED_SURFACES, aggregate_entries


def receipts(statuses):
    return [{"kind": surface, "status": status} for surface, status in statuses.items()]


def all_passed():
    return {surface: "passed" for surface in REQUIRED_SURFACES}


def all_jobs_succeeded():
    return {job: {"result": "success"} for job in JOB_SURFACES}


class AggregateContractTests(unittest.TestCase):
    def test_all_required_surfaces_pass(self):
        result = aggregate_entries(receipts(all_passed()), all_jobs_succeeded(), "stable")
        self.assertEqual(result["overall"], "passed")

    def test_explicit_failure_blocks(self):
        statuses = all_passed()
        statuses["wasm"] = "failed"
        result = aggregate_entries(receipts(statuses), all_jobs_succeeded(), "stable")
        self.assertEqual(result["overall"], "failed")

    def test_absent_receipt_blocks(self):
        statuses = all_passed()
        del statuses["nix"]
        result = aggregate_entries(receipts(statuses), all_jobs_succeeded(), "stable")
        self.assertEqual(result["entries"]["nix"]["status"], "failed")
        self.assertEqual(result["overall"], "failed")

    def test_unavailable_required_surface_blocks(self):
        statuses = all_passed()
        statuses["nix"] = "unavailable"
        result = aggregate_entries(receipts(statuses), all_jobs_succeeded(), "stable")
        self.assertEqual(result["overall"], "failed")

    def test_rc_policy_can_explicitly_exclude_not_supported_surface(self):
        statuses = all_passed()
        statuses["nix"] = "not_supported"
        result = aggregate_entries(receipts(statuses), all_jobs_succeeded(), "rc", {"nix"})
        self.assertEqual(result["overall"], "passed")

    def test_stable_cannot_exclude_not_supported_surface(self):
        statuses = all_passed()
        statuses["nix"] = "not_supported"
        result = aggregate_entries(receipts(statuses), all_jobs_succeeded(), "stable", {"nix"})
        self.assertEqual(result["overall"], "failed")


class AggregateFailClosedTests(unittest.TestCase):
    def test_failed_receipt_cannot_be_overwritten_by_a_pass(self):
        failed = {"kind": "wasm", "status": "failed"}
        passed = {"kind": "wasm", "status": "passed"}
        for duplicate_pair in ([failed, passed], [passed, failed]):
            with self.subTest(order=duplicate_pair):
                items = [item for item in receipts(all_passed()) if item["kind"] != "wasm"]
                result = aggregate_entries(items + duplicate_pair, all_jobs_succeeded(), "stable")
                self.assertEqual(result["overall"], "failed")
                self.assertEqual(result["entries"]["wasm"]["status"], "failed")
                self.assertIn("multiple receipts", result["entries"]["wasm"]["reason"])

    def test_duplicate_success_receipts_are_ambiguous(self):
        items = receipts(all_passed()) + [{"kind": "wasm", "status": "passed"}]
        self.assertEqual(aggregate_entries(items, all_jobs_succeeded(), "stable")["overall"], "failed")

    def test_third_duplicate_cannot_restore_success(self):
        items = receipts(all_passed()) + [{"kind": "wasm", "status": "passed"}] * 2
        self.assertEqual(aggregate_entries(items, all_jobs_succeeded(), "stable")["overall"], "failed")

    def test_binary_and_artifact_aliases_cannot_hide_duplicate_surfaces(self):
        for duplicate in (
            {"kind": "binary", "surface": "binary-linux-amd64", "status": "passed"},
            {"kind": "asset", "artifact": "wasm", "status": "passed"},
        ):
            with self.subTest(duplicate=duplicate):
                result = aggregate_entries(receipts(all_passed()) + [duplicate], all_jobs_succeeded(), "stable")
                self.assertEqual(result["overall"], "failed")

    def test_each_non_success_job_denies_its_passed_receipts(self):
        for job, surfaces in JOB_SURFACES.items():
            for status in ("failure", "cancelled", "skipped", "timed_out", "neutral", "pending", None):
                with self.subTest(job=job, status=status):
                    jobs = all_jobs_succeeded()
                    jobs[job]["result"] = status
                    result = aggregate_entries(receipts(all_passed()), jobs, "stable")
                    self.assertEqual(result["overall"], "failed")
                    for surface in surfaces:
                        self.assertEqual(result["entries"][surface]["status"], "failed")

    def test_absent_job_result_denies_existing_success_receipt(self):
        for job in JOB_SURFACES:
            with self.subTest(job=job):
                jobs = all_jobs_succeeded()
                del jobs[job]
                self.assertEqual(aggregate_entries(receipts(all_passed()), jobs, "stable")["overall"], "failed")

    def test_empty_job_results_cannot_authorize_release(self):
        self.assertEqual(aggregate_entries(receipts(all_passed()), {}, "stable")["overall"], "failed")

    def test_malformed_job_result_shapes_fail_closed(self):
        for bad in (None, [], "success", 1, True):
            with self.subTest(value=bad):
                jobs = all_jobs_succeeded()
                jobs["wasm-smoke"] = bad
                self.assertEqual(aggregate_entries(receipts(all_passed()), jobs, "stable")["overall"], "failed")
                self.assertEqual(aggregate_entries(receipts(all_passed()), bad, "stable")["overall"], "failed")

    def test_non_object_receipts_produce_failed_aggregate(self):
        for invalid in (None, [], "passed", 1, True):
            with self.subTest(value=invalid):
                result = aggregate_entries(receipts(all_passed()) + [invalid], all_jobs_succeeded(), "stable")
                self.assertEqual(result["overall"], "failed")

    def test_non_string_or_missing_surface_keys_fail_closed(self):
        for key in ([], {}, 1, True, None, ""):
            with self.subTest(key=key):
                invalid = {"kind": key, "status": "passed"}
                result = aggregate_entries(receipts(all_passed()) + [invalid], all_jobs_succeeded(), "stable")
                self.assertEqual(result["overall"], "failed")

    def test_successful_jobs_cannot_replace_missing_receipts(self):
        result = aggregate_entries([], all_jobs_succeeded(), "stable")
        self.assertEqual(result["overall"], "failed")
        self.assertTrue(all(result["entries"][surface]["status"] == "failed" for surface in REQUIRED_SURFACES))

    def test_input_receipts_are_not_mutated_by_job_failure(self):
        items = receipts(all_passed())
        original = [dict(item) for item in items]
        aggregate_entries(items, {}, "stable")
        self.assertEqual(items, original)

    def test_rc_exclusion_cannot_waive_failed_producer(self):
        statuses = all_passed()
        statuses["nix"] = "not_supported"
        jobs = all_jobs_succeeded()
        jobs["nix-smoke"]["result"] = "cancelled"
        result = aggregate_entries(receipts(statuses), jobs, "rc", {"nix"})
        self.assertEqual(result["overall"], "failed")

    def test_rc_exclusion_cannot_waive_duplicate_evidence(self):
        items = receipts(all_passed()) + [{"kind": "nix", "status": "not_supported"}]
        result = aggregate_entries(items, all_jobs_succeeded(), "rc", {"nix"})
        self.assertEqual(result["overall"], "failed")


class AggregateEntrypointTests(unittest.TestCase):
    def run_aggregate(self, items, jobs):
        script = Path(__file__).with_name("release_consumer_smoke_aggregate.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "receipts").mkdir()
            for index, item in enumerate(items):
                (root / "receipts" / f"{index:02d}.json").write_text(
                    json.dumps(item), encoding="utf-8"
                )
            env = dict(os.environ)
            env.update(
                NEEDS_RESULTS=json.dumps(jobs),
                RELEASE_KIND="stable",
                RELEASE_REPOSITORY="fixture/tokmd",
                TAG="v1.15.1",
                EXPECTED_VERSION="1.15.1",
                ALLOWED_NOT_SUPPORTED="",
            )
            process = subprocess.run(
                [sys.executable, str(script.resolve())], cwd=root, env=env,
                capture_output=True, text=True, timeout=15,
            )
            # Receipt construction succeeds even when the observed result is
            # failed; the workflow separately enforces the receipt verdict.
            self.assertEqual(process.returncode, 0, process.stderr)
            result = json.loads(
                (root / "tokmd.release_consumer_smoke.v1.json").read_text(encoding="utf-8")
            )
            report = (root / "release-consumer-smoke.md").read_text(encoding="utf-8")
            self.assertIn(f"Overall: `{result['overall']}`", report)
            self.assertEqual(result["repository"], "fixture/tokmd")
            self.assertEqual(result["tag"], "v1.15.1")
            self.assertEqual(result["expected_version"], "1.15.1")
            return result

    def test_valid_files_generate_passing_receipt_and_report(self):
        result = self.run_aggregate(receipts(all_passed()), all_jobs_succeeded())
        self.assertEqual(result["overall"], "passed")
        self.assertEqual(len(result["entries"]), len(REQUIRED_SURFACES))

    def test_failed_then_passed_duplicate_files_remain_failed(self):
        items = receipts(all_passed())
        items.extend([{"kind": "wasm", "status": "failed"},
                      {"kind": "wasm", "status": "passed"}])
        self.assertEqual(self.run_aggregate(items, all_jobs_succeeded())["overall"], "failed")

    def test_passed_then_failed_duplicate_files_remain_failed(self):
        items = receipts(all_passed()) + [{"kind": "wasm", "status": "failed"}]
        self.assertEqual(self.run_aggregate(items, all_jobs_succeeded())["overall"], "failed")

    def test_failed_job_cannot_be_masked_by_passing_files(self):
        jobs = all_jobs_succeeded()
        jobs["binary-smoke"]["result"] = "failure"
        result = self.run_aggregate(receipts(all_passed()), jobs)
        self.assertEqual(result["overall"], "failed")
        for entry in result["entries"]:
            if entry["kind"] in JOB_SURFACES["binary-smoke"]:
                self.assertEqual(entry["status"], "failed")

    def test_non_object_json_retains_a_failed_receipt(self):
        result = self.run_aggregate(receipts(all_passed()) + [None], all_jobs_succeeded())
        self.assertEqual(result["overall"], "failed")


if __name__ == "__main__":
    unittest.main()
