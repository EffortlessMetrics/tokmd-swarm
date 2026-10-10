"""Execute CI's checked-in clock producers and receipt consumer (bash/python3/jq)."""

import datetime
import json
import os
import re
import shutil
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


WORKFLOW = Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml"
MARKERS = (
    "job_started", "core_started", "core_completed",
    "ub_review_started", "ub_review_completed",
)
PHASES = ("core", "ub_review")


def step_script(workflow, name):
    """Read one literal run block without interpreting or rewriting its shell."""
    blocks = re.findall(
        rf"^      - name: {re.escape(name)}\n(.*?)(?=^      - |\Z)",
        workflow, re.MULTILINE | re.DOTALL,
    )
    if len(blocks) != 1 or "        run: |\n" not in blocks[0]:
        raise AssertionError(f"expected one literal run block for {name!r}")
    return textwrap.dedent(blocks[0].split("        run: |\n", 1)[1])


def clock_script(workflow, name):
    body = step_script(workflow, name)
    functions = re.findall(r"^mark_clock\(\) \{\n.*?^\}", body, re.MULTILINE | re.DOTALL)
    calls = re.findall(r"^mark_clock target/ci-core/\w+$", body, re.MULTILINE)
    if len(functions) != 1 or not calls:
        raise AssertionError(f"expected one clock definition and calls in {name!r}")
    return functions[0], calls


class ClockBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for tool in ("bash", "python3", "jq"):
            if shutil.which(tool) is None:
                raise AssertionError(f"required CI boundary tool is missing: {tool}")
        cls.workflow = WORKFLOW.read_text(encoding="utf-8")
        cls.consumer = step_script(cls.workflow, "Write UB Review observability receipt")
        cls.snapshot = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.snapshot.cleanup)
        root = Path(cls.snapshot.name)
        (root / "target/ci-core").mkdir(parents=True)
        init = step_script(cls.workflow, "Initialize CI observability receipt")
        core, core_calls = clock_script(cls.workflow, "Fast precontext and launch core gate")
        review = step_script(cls.workflow, "Record UB Review start")
        complete = step_script(cls.workflow, "UB Review advisory status")
        if len(core_calls) != 2:
            raise AssertionError("expected core start and completion calls")
        # Use all four actual definitions and their actual marker destinations.
        # Only the unrelated Cargo proof commands are omitted. Bounded waits
        # make integer-second elapsed values discriminate against invented zero.
        cls.run_shell(
            root,
            "\n".join((init, core, core_calls[0], "sleep 1.1", core_calls[1], review, "sleep 1.1", complete)),
        )

    @staticmethod
    def run_shell(root, script, outcome="success"):
        env = dict(os.environ)
        env.update(
            UB_OUTCOME=outcome, RUNNER_KIND="github", HEAD_SHA="a" * 40,
            GITHUB_WORKFLOW="CI", GITHUB_RUN_ID="635", GITHUB_RUN_ATTEMPT="1",
            GITHUB_JOB="tokmd-rust-result", GITHUB_STEP_SUMMARY=str(root / "summary.md"),
        )
        result = subprocess.run(
            ["bash", "-e", "-o", "pipefail", "-c", script], cwd=root, env=env,
            capture_output=True, text=True, timeout=15,
        )
        if result.returncode != 0:
            raise AssertionError(f"CI shell exited {result.returncode}: {result.stderr}")

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.markers = self.root / "target/ci-core"
        shutil.copytree(Path(self.snapshot.name) / "target/ci-core", self.markers)
        for name in ("core_exit", "test_exit", "xtask_test_exit", "proof_policy_exit"):
            (self.markers / name).write_text("0\n", encoding="utf-8")

    def receipt(self, outcome="success"):
        self.run_shell(self.root, self.consumer, outcome)
        return json.loads((self.markers / "ub-review-receipt.json").read_text(encoding="utf-8"))

    def controlled_clocks(self):
        # Negative consumer controls do not depend on the producer being fixed.
        for index, name in enumerate(MARKERS, start=1):
            (self.markers / f"{name}_mono_ns").write_text(f"{index * 2_000_000_000}\n", encoding="utf-8")
            (self.markers / f"{name}_utc").write_text(f"2026-10-10T00:00:{index * 2:02d}Z\n", encoding="utf-8")

    def test_every_actual_producer_writes_numeric_and_utc_newline_markers(self):
        for name in MARKERS:
            with self.subTest(marker=name):
                mono = (self.markers / f"{name}_mono_ns").read_bytes()
                self.assertRegex(mono, rb"\A[0-9]+\n\Z")
                utc = (self.markers / f"{name}_utc").read_bytes()
                self.assertRegex(utc, rb"\A[^\n\\]+Z\n\Z")
                timestamp = datetime.datetime.fromisoformat(utc.decode().rstrip("\n"))
                self.assertEqual(timestamp.utcoffset(), datetime.timedelta(0))

    def test_actual_producers_are_consumed_as_ordered_measured_timestamps(self):
        receipt = self.receipt()
        self.assertEqual(receipt["schema_version"], "tokmd.ci.ub-review.v1")
        self.assertEqual(receipt["timing_integrity"], "valid")
        self.assertEqual(receipt["terminal_reason"], "completed")
        self.assertEqual(receipt["timeout_classification"], "not_timeout")
        self.assertEqual(receipt["timeout_minutes"], 90)
        job_started = receipt["observability_started_mono_ns"]
        self.assertIsInstance(job_started, int)
        core = receipt["phases"]["core"]
        review = receipt["phases"]["ub_review"]
        self.assertLessEqual(job_started, core["started_mono_ns"])
        self.assertLessEqual(core["completed_mono_ns"], review["started_mono_ns"])
        self.assertLessEqual(core["completed_mono_ns"], review["completed_mono_ns"])
        for phase in PHASES:
            row = receipt["phases"][phase]
            self.assertIsInstance(row["started_mono_ns"], int)
            self.assertIsInstance(row["completed_mono_ns"], int)
            self.assertLess(row["started_mono_ns"], row["completed_mono_ns"])
            self.assertGreaterEqual(row["elapsed_seconds"], 1)
            self.assertEqual(row["elapsed_seconds"], (row["completed_mono_ns"] - row["started_mono_ns"]) // 1_000_000_000)
            for key in ("started_utc", "completed_utc"):
                self.assertTrue(row[key].endswith("Z"))
                self.assertNotIn("\\", row[key])
                self.assertNotIn("\n", row[key])
            self.assertLess(
                datetime.datetime.fromisoformat(row["started_utc"]),
                datetime.datetime.fromisoformat(row["completed_utc"]),
            )

    def test_missing_or_malformed_phase_marker_is_incomplete(self):
        for phase in PHASES:
            for edge in ("started", "completed"):
                for invalid in (None, "", "123\\n", "not-a-clock\n"):
                    with self.subTest(phase=phase, edge=edge, invalid=invalid):
                        self.controlled_clocks()
                        path = self.markers / f"{phase}_{edge}_mono_ns"
                        if invalid is None:
                            path.unlink()
                        else:
                            path.write_text(invalid, encoding="utf-8")
                        receipt = self.receipt()
                        self.assertEqual(receipt["timing_integrity"], "incomplete_or_reversed")
                        self.assertIsNone(receipt["phases"][phase][f"{edge}_mono_ns"])
                        self.assertEqual(receipt["phases"][phase]["elapsed_seconds"], 0)

    def test_reversed_phase_markers_are_incomplete(self):
        for phase in PHASES:
            with self.subTest(phase=phase):
                self.controlled_clocks()
                (self.markers / f"{phase}_completed_mono_ns").write_text("1\n", encoding="utf-8")
                receipt = self.receipt()
                self.assertEqual(receipt["timing_integrity"], "incomplete_or_reversed")
                self.assertEqual(receipt["phases"][phase]["elapsed_seconds"], 0)

    def test_skipped_review_has_no_measurement_and_is_distinct_from_measured_zero(self):
        self.controlled_clocks()
        for name in ("ub_review_started", "ub_review_completed"):
            for suffix in ("mono_ns", "utc"):
                (self.markers / f"{name}_{suffix}").unlink()
        self.run_shell(self.root, step_script(self.workflow, "UB Review advisory status"), "skipped")
        skipped = self.receipt("skipped")
        self.assertEqual(skipped["timing_integrity"], "valid")
        self.assertEqual(skipped["terminal_reason"], "ub_review_skipped")
        row = skipped["phases"]["ub_review"]
        for key in ("started_mono_ns", "completed_mono_ns", "started_utc", "completed_utc"):
            self.assertIsNone(row[key])
        self.assertEqual(row["outcome"], "skipped")
        self.assertEqual(row["elapsed_seconds"], 0)
        self.controlled_clocks()
        start = self.markers / "ub_review_started_mono_ns"
        (self.markers / "ub_review_completed_mono_ns").write_bytes(start.read_bytes())
        measured = self.receipt()
        self.assertEqual(measured["timing_integrity"], "valid")
        self.assertEqual(measured["terminal_reason"], "completed")
        row = measured["phases"]["ub_review"]
        self.assertEqual(row["started_mono_ns"], row["completed_mono_ns"])
        self.assertIsInstance(row["started_mono_ns"], int)
        self.assertEqual(row["elapsed_seconds"], 0)
        self.assertEqual(row["outcome"], "success")

    def test_advisory_outcome_and_core_timeout_keep_existing_classifications(self):
        for outcome, core_exit, terminal, timeout in (
            ("failure", "0", "ub_review_advisory_failed", "not_timeout"),
            ("failure", "1", "core_proof_failed", "not_timeout"),
            ("success", "124", "core_proof_timeout", "bounded_command_timeout"),
        ):
            with self.subTest(outcome=outcome, core_exit=core_exit):
                self.controlled_clocks()
                (self.markers / "core_exit").write_text(f"{core_exit}\n", encoding="utf-8")
                receipt = self.receipt(outcome)
                self.assertEqual(receipt["terminal_reason"], terminal)
                self.assertEqual(receipt["timeout_classification"], timeout)
                self.assertEqual(receipt["phases"]["ub_review"]["outcome"], outcome)


if __name__ == "__main__":
    unittest.main()
