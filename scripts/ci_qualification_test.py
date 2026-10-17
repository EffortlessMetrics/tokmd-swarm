"""Exercise admission and refusal through checked-in production shell/source.

The candidate admits explicit native PR requests and refuses dispatch proof
without PR context. Passing this suite does not establish native CI qualification.
"""

import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("qualification", ROOT / "scripts/ci_qualification.py")
Q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(Q)
HEAD, BASE = "a" * 40, "b" * 40


def step_script(source, name):
    blocks = re.findall(rf"^      - name: {re.escape(name)}\n(.*?)(?=^      - |^  [a-z][a-z0-9-]*:|\Z)", source, re.MULTILINE | re.DOTALL)
    if len(blocks) != 1 or "        run: " not in blocks[0]:
        raise AssertionError(f"missing literal production step {name}")
    value = blocks[0].split("        run: ", 1)[1]
    return textwrap.dedent(value[2:]) if value.startswith("|\n") else value.splitlines()[0]


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.event = {"inputs": {"pull_request": "734", "head_sha": HEAD, "base_sha": BASE,
                                 "generation": f"{HEAD}:{BASE}", "expected_green": "true"}}
        self.context = dict(event_name="workflow_dispatch", ref="refs/heads/topic", sha=HEAD,
                            workflow_sha=HEAD, repository=Q.REPOSITORY)
        self.refs = {"refs/pull/734/head": HEAD, "refs/heads/main": BASE}

    def evaluate(self):
        return Q.admit(self.event, self.context, lambda pr: self.refs)

    def test_exact_expected_green_request_validates_its_native_source_and_generation(self):
        self.assertEqual(self.evaluate(), dict(mode="qualification", head=HEAD, base=BASE, pr="734"))

    def test_no_request_draft_ready_and_label_events_are_refused_before_source_reads(self):
        def forbidden(pr):
            self.fail("unrequested event must not read Git source or reach proof")
        for action in ("opened", "synchronize", "ready_for_review", "converted_to_draft", "labeled"):
            with self.subTest(action=action), self.assertRaisesRegex(ValueError, "explicit qualification"):
                Q.admit(dict(action=action, pull_request=dict(draft=True)), dict(self.context, event_name="pull_request"), forbidden)

    def test_malformed_missing_unreviewed_or_wrong_generation_request_is_refused(self):
        for key, value in (("pull_request", "../main"), ("head_sha", "A" * 40), ("base_sha", ""),
                           ("generation", f"{HEAD}:{'c' * 40}"), ("expected_green", "false")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                Q.admit({"inputs": dict(self.event["inputs"], **{key: value})}, self.context, lambda pr: self.refs)

    def test_native_run_workflow_or_repository_cannot_be_relabelled_as_the_request(self):
        for key in ("sha", "workflow_sha", "repository"):
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "native run"):
                Q.admit(self.event, dict(self.context, **{key: "c" * 40}), lambda pr: self.refs)

    def test_head_base_and_generation_movement_refuse_stale_request(self):
        for ref in self.refs:
            with self.subTest(ref=ref), self.assertRaisesRegex(ValueError, "stale"):
                Q.admit(self.event, self.context, lambda pr: dict(self.refs, **{ref: "c" * 40}))

    def test_source_read_failure_is_not_an_admission(self):
        def failure(pr):
            raise subprocess.TimeoutExpired("git ls-remote", 20)
        with self.assertRaises(subprocess.TimeoutExpired):
            Q.admit(self.event, self.context, failure)

    def test_valid_dispatch_reaches_truthful_production_migration_refusal(self):
        source = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn("python3 scripts/ci_qualification.py", step_script(source, "Validate explicit qualification request"))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event = root / "event.json"
            event.write_text(json.dumps(self.event))
            output = root / "output"
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_OUTPUT=str(output),
                       GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF=self.context["ref"], GITHUB_SHA=HEAD,
                       GITHUB_WORKFLOW_SHA=HEAD, GITHUB_REPOSITORY=Q.REPOSITORY)
            with patch.dict(os.environ, env), patch.object(Q, "live_refs", return_value=self.refs):
                self.assertEqual(Q.main(), 1)
            self.assertFalse(output.exists(), "refusal must not publish admitted=true")

    def native_request(self):
        merge = "c" * 40
        request = dict(self.event["inputs"], merge_sha=merge)
        body = "<!-- tokmd-qualification:v1\n" + json.dumps(request) + "\n-->"
        event = dict(action="labeled", label=dict(name="ci:qualify"), number=734,
                     pull_request=dict(state="open", body=body, draft=True, head=dict(sha=HEAD), base=dict(sha=BASE, ref="main")))
        context = dict(self.context, event_name="pull_request", ref="refs/pull/734/merge", sha=merge, workflow_sha=merge)
        refs = dict(self.refs, **{"refs/pull/734/merge": merge})
        return event, context, refs

    def test_native_explicit_request_admits_real_pr_composition_including_draft(self):
        event, context, refs = self.native_request()
        self.assertEqual(Q.admit(event, context, lambda pr: refs)["mode"], "pr-qualification")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event_path, output = root / "event", root / "output"
            event_path.write_text(json.dumps(event))
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event_path), GITHUB_OUTPUT=str(output),
                       GITHUB_EVENT_NAME=context["event_name"], GITHUB_REF=context["ref"], GITHUB_SHA=context["sha"],
                       GITHUB_WORKFLOW_SHA=context["workflow_sha"], GITHUB_REPOSITORY=Q.REPOSITORY,
                       QUALIFICATION_DEADLINE=str(int(time.time()) + 600))
            with patch.dict(os.environ, env), patch.object(Q, "live_refs", return_value=refs):
                self.assertEqual(Q.main(), 0)
            self.assertIn("admitted=true\n", output.read_text())

    def test_native_label_alone_closed_wrong_pr_or_stale_merge_cannot_admit(self):
        event, context, refs = self.native_request()
        cases = [dict(event, pull_request=dict(event["pull_request"], body="")),
                 dict(event, pull_request=dict(event["pull_request"], state="closed")), dict(event, number=735)]
        for candidate in cases:
            with self.subTest(candidate=candidate), self.assertRaises(ValueError):
                Q.admit(candidate, context, lambda pr: refs)
        with self.assertRaisesRegex(ValueError, "merge composition is stale"):
            Q.admit(event, context, lambda pr: dict(refs, **{"refs/pull/734/merge": "d" * 40}))


    def test_main_push_entrypoint_preserves_post_integration_admission_and_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event, output = root / "event", root / "output"
            event.write_text("{}")
            deadline = str(int(time.time()) + 600)
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_OUTPUT=str(output),
                       GITHUB_EVENT_NAME="push", GITHUB_REF="refs/heads/main", GITHUB_SHA=HEAD,
                       GITHUB_WORKFLOW_SHA=HEAD, GITHUB_REPOSITORY=Q.REPOSITORY, QUALIFICATION_DEADLINE=deadline)
            with patch.dict(os.environ, env), patch.object(Q, "live_refs", side_effect=AssertionError("unexpected source read")):
                self.assertEqual(Q.main(), 0)
            values = dict(line.split("=", 1) for line in output.read_text().splitlines())
            self.assertEqual(values["admitted"], "true")
            self.assertEqual(values["deadline_epoch"], deadline)
            self.assertEqual(values["mode"], "main-push")



class ProductionPathTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = (ROOT / ".github/workflows/ci.yml").read_text()

    def test_trigger_namespace_has_no_job_directives(self):
        triggers = self.source.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        self.assertNotRegex(triggers, r"(?m)^    (if|needs):")

    def test_actual_pr_trigger_admits_explicit_label_and_keeps_ready_as_collaboration(self):
        triggers = self.source.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        pr = triggers.split("  pull_request:\n", 1)[1].split("  workflow_dispatch:", 1)[0]
        types = re.search(r"^    types: \[(.+)\]$", pr, re.MULTILINE).group(1)
        actual = {value.strip() for value in types.split(",")}
        self.assertEqual(actual, {"opened", "reopened", "synchronize", "labeled"})
        self.assertNotIn("ready_for_review", actual)
        self.assertNotIn("converted_to_draft", actual)
        self.assertIn('branches: [ "main" ]', pr)

    def test_actual_route_keeps_forks_hosted_and_requires_same_idle_runner_capabilities(self):
        body = step_script(self.source, "Decide gate runner (self-hosted primary, gh-hosted overflow)")
        split = {"runners": [{"status": "online", "busy": False, "labels": [{"name": "trusted-pr"}]},
                             {"status": "online", "busy": False, "labels": [{"name": "rust-small"}]}]}
        capable = {"runners": [{"status": "online", "busy": False, "labels": [{"name": "trusted-pr"}, {"name": "rust-small"}]}]}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            gh = root / "gh"
            gh.write_text('#!/bin/bash\nprintf "%s\\n" "$RUNNERS_JSON"\n')
            gh.chmod(0o755)
            for fork, runners, token, expected in ((True, capable, "test-only", "github"),
                    (False, split, "test-only", "github"), (False, capable, "test-only", "self-hosted"),
                    (False, capable, "", "github")):
                with self.subTest(fork=fork, runners=runners, token=bool(token)):
                    output = root / "output"
                    output.unlink(missing_ok=True)
                    script = body.replace("${{ github.event_name }}", "pull_request").replace("${{ github.event.pull_request.head.repo.fork }}", str(fork).lower())
                    run = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], cwd=root,
                        env=dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"], GITHUB_OUTPUT=str(output),
                                 GH_TOKEN=token, RUNNERS_JSON=json.dumps(runners)), capture_output=True, text=True, timeout=3)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    self.assertIn("runner_kind=" + expected + "\n", output.read_text())

    def test_all_old_jobs_depend_on_real_admission_and_required_name_is_preserved(self):
        jobs = re.split(r"^  ([a-z][a-z0-9-]*):\n", self.source.split("jobs:\n", 1)[1], flags=re.MULTILINE)
        seen = 0
        for name, body in zip(jobs[1::2], jobs[2::2]):
            if name == "admission":
                continue
            seen += 1
            self.assertRegex(body, r"(?m)^    needs:.*admission|^      - admission$")
            if name == "tokmd-rust-result":
                self.assertIn("    name: Tokmd Rust Result\n", body)
                self.assertIn("    if: always()\n", body)
            else:
                self.assertIn("needs.admission.outputs.admitted == 'true'", body)
        self.assertEqual(seen, 19)

    def test_expensive_jobs_wait_for_the_blocking_baseline(self):
        jobs = re.split(r"^  ([a-z][a-z0-9-]*):\n", self.source.split("jobs:\n", 1)[1], flags=re.MULTILINE)
        for name, body in zip(jobs[1::2], jobs[2::2]):
            if name in ("admission", "route", "deny"):
                continue
            with self.subTest(job=name):
                self.assertIn("deny", re.search(r"^    needs:(.*?)(?=^    [a-z]|\Z)", body, re.MULTILINE | re.DOTALL).group(1))
                if name != "tokmd-rust-result":
                    self.assertIn("needs.deny.result == 'success'", body)

    def test_actual_required_refusal_step_fails_without_successful_admission(self):
        body = step_script(self.source, "Refuse unrequested or failed admission")
        for admitted, result, expected in (("", "failure", 1), ("true", "failure", 1), ("", "success", 1), ("true", "success", 0)):
            with self.subTest(admitted=admitted, result=result):
                run = subprocess.run(["bash", "-e", "-c", body], env=dict(os.environ, ADMITTED=admitted, ADMISSION_RESULT=result, BASELINE_RESULT="success"), timeout=3)
                self.assertEqual(run.returncode, expected)

    def test_actual_concurrency_separates_observation_and_exact_generation_without_cancellation(self):
        block = self.source.split("concurrency:\n", 1)[1].split("\nenv:\n", 1)[0]
        self.assertIn("github.event_name == 'workflow_dispatch'", block)
        self.assertIn("format('qualification-{0}-{1}', inputs.pull_request, inputs.generation)", block)
        self.assertIn("format('observation-{0}', github.event.pull_request.number || github.ref)", block)
        self.assertIn("  cancel-in-progress: false\n", block)
        self.assertIn("  queue: max\n", block)
        # Evaluate the actual checked-in group expression for this supported
        # expression subset. Service queue behavior remains a native canary gap.
        group = re.search(r"^  group: (.+)$", block, re.MULTILINE).group(1)
        expression = re.search(r"\$\{\{ (github.event_name.+) \}\}", group).group(1)
        program = """const [expression, context] = process.argv.slice(1);
const {github, inputs} = JSON.parse(context);
const format = (template, ...args) => template.replace(/\\{(\\d+)\\}/g, (_, i) => args[Number(i)]);
process.stdout.write(String(eval(expression)));"""
        def actual(event, head=HEAD, base=BASE, action="opened", label="ordinary"):
            context = dict(github=dict(event_name=event, event=dict(action=action, label=dict(name=label), pull_request=dict(number=734)), sha="merge-" + head + base, ref="refs/heads/topic"),
                           inputs=dict(pull_request="734", generation=f"{head}:{base}"))
            result = subprocess.run(["node", "-e", program, expression, json.dumps(context)], capture_output=True, text=True, timeout=3)
            self.assertEqual(result.returncode, 0, result.stderr)
            return result.stdout
        request = actual("workflow_dispatch")
        self.assertEqual(request, actual("workflow_dispatch"))
        self.assertNotEqual(request, actual("workflow_dispatch", head="c" * 40))
        self.assertNotEqual(request, actual("pull_request"))
        native = actual("pull_request", action="labeled", label="ci:qualify")
        self.assertEqual(native, actual("pull_request", action="labeled", label="ci:qualify"))
        self.assertNotEqual(native, actual("pull_request", action="labeled", label="ordinary"))
        self.assertNotEqual(native, actual("pull_request", head="c" * 40, action="labeled", label="ci:qualify"))

    def test_production_admission_checks_checkout_before_running_validator(self):
        body = step_script(self.source, "Validate explicit qualification request")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, script in (("git", '#!/bin/bash\necho "$CHECKOUT_SHA"\n'), ("python3", '#!/bin/bash\ntouch "$CALLED"\n')):
                path = root / name
                path.write_text(script)
                path.chmod(0o755)
            for checkout, expected in ((HEAD, 0), (BASE, 1)):
                marker = root / "called"
                marker.unlink(missing_ok=True)
                result = subprocess.run(["bash", "-e", "-c", body], env=dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                                        CHECKOUT_SHA=checkout, GITHUB_SHA=HEAD, CALLED=str(marker)), capture_output=True, text=True, timeout=3)
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertEqual(marker.exists(), expected == 0)

    def test_red_baseline_cannot_admit_the_required_proof(self):
        body = step_script(self.source, "Refuse unrequested or failed admission")
        result = subprocess.run(["bash", "-e", "-c", body], env=dict(os.environ, ADMITTED="true", ADMISSION_RESULT="success", BASELINE_RESULT="failure"), timeout=3)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("needs: [admission, route, deny]", self.source)

    def test_actual_advisory_conditions_refuse_red_baseline_and_stale_pre_setup_source(self):
        program = """const [expression, prior, fork] = process.argv.slice(1);
const success = () => prior === 'success'; const cancelled = () => false;
const needs = {admission: {outputs: {admitted: 'true'}}};
const github = {event_name: 'pull_request', event: {pull_request: {head: {repo: {fork: fork === 'true'}}}}};
process.stdout.write(String(eval(expression)));"""
        for name in ("Record UB Review start", "UB Review (advisory)"):
            block = re.findall(rf"^      - name: {re.escape(name)}\n(.*?)(?=^      - |^  [a-z][a-z0-9-]*:|\Z)", self.source, re.MULTILINE | re.DOTALL)[0]
            condition = re.search(r"^        if: \$\{\{ (.+) \}\}$", block, re.MULTILINE).group(1)
            for prior, fork, expected in (("red-baseline", "false", "false"), ("stale-source", "false", "false"),
                                          ("success", "false", "true"), ("success", "true", "false")):
                with self.subTest(step=name, prior=prior, fork=fork):
                    run = subprocess.run(["node", "-e", program, condition, prior, fork], capture_output=True, text=True, timeout=3)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    self.assertEqual(run.stdout, expected)

    def test_actuals_cargo_report_refuses_failed_pre_or_post_source_validation(self):
        rust = self.source.split("  tokmd-rust-result:\n", 1)[1].split("  detect:\n", 1)[0]
        output = re.search(r"^      source_validated: \$\{\{ (.+) \}\}$", rust, re.MULTILINE).group(1)
        actuals = self.source.split("  ci-actuals:\n", 1)[1]
        condition = re.search(r"^    if: (.+)$", actuals, re.MULTILINE).group(1)
        condition = condition.replace("needs.tokmd-rust-result", 'needs["tokmd-rust-result"]')
        program = """const [output, condition, before, after, baseline] = process.argv.slice(1);
const steps = {source_before: {outcome: before}, source_after: {outcome: after}};
const validated = String(eval(output));
const always = () => true;
const needs = {admission: {outputs: {admitted: 'true'}}, deny: {result: baseline},
  'tokmd-rust-result': {outputs: {source_validated: validated}}};
process.stdout.write(String(eval(condition)));"""
        for before, after, baseline, expected in (("failure", "skipped", "success", "false"),
                    ("success", "failure", "success", "false"), ("success", "success", "failure", "false"),
                    ("success", "success", "success", "true")):
            with self.subTest(before=before, after=after, baseline=baseline):
                run = subprocess.run(["node", "-e", program, output, condition, before, after, baseline], capture_output=True, text=True, timeout=3)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(run.stdout, expected)

    def test_actual_core_keeps_all_four_obligations_and_consumes_one_budget(self):
        body = step_script(self.source, "Fast precontext and launch core gate")
        expected = ["--locked xtask gate --check", "test --locked --all-features",
                    "test --locked -p xtask --all-features", "--locked xtask proof-policy --check"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bindir = root / "bin"
            bindir.mkdir()
            scripts = {
                "cargo": '#!/bin/bash\necho "$*" >> "$CALLS"\nexit 0\n',
                "timeout": '#!/bin/bash\necho "$1 $2" >> "$CALLS.timeout"\nshift 2\nexec "$@"\n',
                "date": '#!/bin/bash\nif [ "$1" = +%s ]; then n=$(cat "$CALLS.clock" 2>/dev/null || echo 100); echo "$n"; echo "$((n+1))" > "$CALLS.clock"; else exec /bin/date "$@"; fi\n',
            }
            for name, script in scripts.items():
                path = bindir / name
                path.write_text(script)
                path.chmod(0o755)
            env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"], RUNNER_KIND="github",
                       GITHUB_EVENT_NAME="workflow_dispatch", CALLS=str(root / "calls"), QUALIFICATION_DEADLINE="104")
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", body], cwd=root, env=env, capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "calls").read_text().splitlines(), expected)
            self.assertEqual((root / "calls.timeout").read_text().splitlines(), [f"--kill-after=30s {n}s" for n in (4, 3, 2, 1)])

    def test_actual_core_commands_share_one_deadline_and_expired_budget_launches_nothing(self):
        body = step_script(self.source, "Fast precontext and launch core gate")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bindir = root / "bin"
            bindir.mkdir()
            cargo = bindir / "cargo"
            cargo.write_text('#!/bin/bash\necho "$*" >> "$CALLS"\nexit 0\n')
            cargo.chmod(0o755)
            env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"], RUNNER_KIND="github",
                       GITHUB_EVENT_NAME="workflow_dispatch", CALLS=str(root / "calls"), QUALIFICATION_DEADLINE="1")
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", body], cwd=root, env=env, capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse((root / "calls").exists())
            for marker in ("core_exit", "test_exit", "xtask_test_exit", "proof_policy_exit"):
                self.assertEqual((root / "target/ci-core" / marker).read_text(), "124\n")
        self.assertNotIn('timeout --kill-after=30s 15m', body)
        self.assertEqual(body.count('run_bounded "cargo '), 4)
        self.assertNotRegex(step_script(self.source, "Assert core gate verdict"), r"(?m)^\s*cargo ")

    def test_actual_verdict_preserves_a_failed_obligation_without_retry(self):
        body = step_script(self.source, "Assert core gate verdict")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            markers = root / "target/ci-core"
            markers.mkdir(parents=True)
            expected = {"core_exit": "0", "test_exit": "2", "xtask_test_exit": "0", "proof_policy_exit": "0"}
            for name, value in expected.items():
                (markers / name).write_text(value + "\n")
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", body], cwd=root,
                                    env=dict(os.environ, RUNNER_KIND="self-hosted", GITHUB_STEP_SUMMARY=str(root / "summary")),
                                    capture_output=True, text=True, timeout=3)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Tests: FAIL (exit 2)", (root / "summary").read_text())
            for name, value in expected.items():
                self.assertEqual((markers / name).read_text(), value + "\n")


if __name__ == "__main__":
    unittest.main()
