"""Exercise admission and refusal through checked-in production shell/source.

The candidate admits explicit native PR requests and refuses dispatch proof
without PR context. Passing this suite does not establish native CI qualification.
"""

import importlib.util
from collections import Counter
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
NODE = os.environ.get('TOKMD_CI_NODE', 'node')
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
        self.refs = {"refs/pull/734/head": HEAD, "refs/heads/codex/tokmd-queue-canary-base": BASE}

    def evaluate(self):
        return Q.admit(self.event, self.context, lambda pr: self.refs)

    def test_merge_group_admits_only_exact_native_composed_source(self):
        ref = "refs/heads/gh-readonly-queue/codex/tokmd-queue-canary-base/pr-734-" + HEAD
        group = dict(head_sha=HEAD, base_sha=BASE, head_ref=ref, base_ref="refs/heads/codex/tokmd-queue-canary-base")
        event = dict(action="checks_requested", repository=dict(full_name=Q.REPOSITORY), merge_group=group)
        context = dict(self.context, event_name="merge_group", ref=ref, sha=HEAD, workflow_sha=HEAD)
        def unexpected_read(_):
            self.fail("queue admission does not read PR refs")
        self.assertEqual(Q.admit(event, context, unexpected_read),
                         dict(mode="merge-group", head=HEAD, base=BASE, pr=""))
        for key, value in (("head_sha", "c" * 40), ("base_sha", "bad"),
                           ("head_ref", "refs/heads/codex/tokmd-queue-canary-base"), ("base_ref", "refs/heads/other")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                Q.admit(dict(event, merge_group=dict(group, **{key: value})), context, unexpected_read)
        for key, value in (("sha", "c" * 40), ("workflow_sha", "c" * 40),
                           ("ref", "refs/heads/codex/tokmd-queue-canary-base"), ("repository", "other/repo")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                Q.admit(event, dict(context, **{key: value}), unexpected_read)
        with self.assertRaises(ValueError):
            Q.admit(dict(event, action="destroyed"), context, unexpected_read)

    def test_merge_group_main_writes_admission_with_shared_budget(self):
        ref = "refs/heads/gh-readonly-queue/codex/tokmd-queue-canary-base/pr-734-" + HEAD
        event = dict(action="checks_requested", repository=dict(full_name=Q.REPOSITORY),
                     merge_group=dict(head_sha=HEAD, base_sha=BASE, head_ref=ref, base_ref="refs/heads/codex/tokmd-queue-canary-base"))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event_path, output = root / "event", root / "output"
            event_path.write_text(json.dumps(event))
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event_path), GITHUB_OUTPUT=str(output),
                       GITHUB_EVENT_NAME="merge_group", GITHUB_REF=ref, GITHUB_SHA=HEAD,
                       GITHUB_WORKFLOW_SHA=HEAD, GITHUB_REPOSITORY=Q.REPOSITORY,
                       QUALIFICATION_DEADLINE=str(int(time.time()) + 600))
            with patch.dict(os.environ, env, clear=True):
                self.assertEqual(Q.main(), 0)
            self.assertIn("mode=merge-group\n", output.read_text())
            self.assertIn("admitted=true\n", output.read_text())

    def test_exact_expected_green_request_validates_its_native_source_and_generation(self):
        self.assertEqual(self.evaluate(), dict(mode="qualification", head=HEAD, base=BASE, pr="734"))

    def test_no_request_draft_ready_and_label_events_are_refused_before_source_reads(self):
        def forbidden(pr):
            self.fail("unrequested event must not read Git source or reach proof")
        for action in ("opened", "synchronize", "ready_for_review", "converted_to_draft", "labeled"):
            with self.subTest(action=action), self.assertRaisesRegex(ValueError, "explicit qualification"):
                Q.admit(dict(action=action, pull_request=dict(draft=False)), dict(self.context, event_name="pull_request"), forbidden)

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
        request = dict(self.event["inputs"], merge_sha=merge, deadline_epoch=str(int(time.time()) + 600))
        body = "<!-- tokmd-qualification:v1\n" + json.dumps(request) + "\n-->"
        event = dict(action="labeled", label=dict(name="ci:qualify"), number=734,
                     repository=dict(full_name=Q.REPOSITORY),
                     pull_request=dict(state="open", body=body, draft=False, head=dict(sha=HEAD),
                                       base=dict(sha=BASE, ref="codex/tokmd-queue-canary-base", repo=dict(full_name=Q.REPOSITORY))))
        context = dict(self.context, event_name="pull_request", ref="refs/pull/734/merge", sha=merge, workflow_sha=merge)
        refs = dict(self.refs, **{"refs/pull/734/merge": merge})
        return event, context, refs

    def run_native_entrypoint(self, root, event, context, refs, deadline=None):
        event_path, output, calls = (root / name for name in ('event', 'output', 'git-calls'))
        for path in (output, calls):
            path.unlink(missing_ok=True)
        event_path.write_text(json.dumps(event))
        git = root / 'git'
        git.write_text(f'#!{sys.executable}\nimport json, os, pathlib, sys\n'
                       'pathlib.Path(os.environ["GIT_CALLS"]).write_text(json.dumps(sys.argv[1:]))\n'
                       'print(os.environ["SOURCE_REFS"])\n')
        git.chmod(0o755)
        env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'],
                   GITHUB_EVENT_PATH=str(event_path), GITHUB_OUTPUT=str(output), GITHUB_ENV=str(root / 'environment'),
                   GITHUB_EVENT_NAME=context['event_name'], GITHUB_REF=context['ref'], GITHUB_SHA=context['sha'],
                   GITHUB_WORKFLOW_SHA=context['workflow_sha'], GITHUB_REPOSITORY=context['repository'],
                   QUALIFICATION_DEADLINE=deadline or str(int(time.time()) + 600), GIT_CALLS=str(calls),
                   SOURCE_REFS='\n'.join(f'{sha}\t{ref}' for ref, sha in refs.items()))
        env.pop('CI_BUDGET_MONO_NS', None)
        run = subprocess.run([sys.executable, str(ROOT / 'scripts/ci_qualification.py')], cwd=root,
                             env=env, capture_output=True, text=True, timeout=3)
        return run, output.read_text() if output.exists() else '', json.loads(calls.read_text()) if calls.exists() else None

    def test_shared_repositories_bind_native_pr_source_reads_to_base_repository(self):
        with tempfile.TemporaryDirectory() as directory:
            for repository in ('EffortlessMetrics/tokmd-swarm', 'EffortlessMetrics/tokmd'):
                event, context, refs = self.native_request()
                context['repository'] = repository
                event['repository']['full_name'] = repository
                event['pull_request']['base']['repo']['full_name'] = repository
                event['pull_request']['head']['repo'] = dict(full_name='contributor/fork', fork=True)
                with self.subTest(repository=repository):
                    run, output, calls = self.run_native_entrypoint(Path(directory), event, context, refs)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    self.assertIn('admitted=true\n', output)
                    self.assertEqual(calls, ['ls-remote', f'https://github.com/{repository}.git',
                                            'refs/pull/734/head', 'refs/pull/734/merge', 'refs/heads/codex/tokmd-queue-canary-base'])

    def test_native_repository_mismatches_refuse_before_git_on_pr_and_main_push(self):
        with tempfile.TemporaryDirectory() as directory:
            for kind in ('pull_request', 'push'):
                for mismatch in ('native', 'event', 'base', 'missing-event', 'missing-base'):
                    if kind == 'push' and mismatch in ('base', 'missing-base'):
                        continue
                    event, context, refs = self.native_request()
                    if kind == 'push':
                        context.update(event_name='push', ref='refs/heads/codex/tokmd-queue-canary-base')
                    if mismatch == 'native':
                        context['repository'] = 'contributor/fork'
                        event['repository']['full_name'] = 'contributor/fork'
                        event['pull_request']['base']['repo']['full_name'] = 'contributor/fork'
                    elif mismatch == 'event':
                        event['repository']['full_name'] = 'EffortlessMetrics/tokmd'
                    elif mismatch == 'base':
                        event['pull_request']['base']['repo']['full_name'] = 'EffortlessMetrics/tokmd'
                    elif mismatch == 'missing-event':
                        event.pop('repository')
                    else:
                        event['pull_request']['base'].pop('repo')
                    with self.subTest(kind=kind, mismatch=mismatch):
                        run, output, calls = self.run_native_entrypoint(Path(directory), event, context, refs)
                        self.assertEqual(run.returncode, 1, run.stderr)
                        self.assertEqual(output, '')
                        self.assertIsNone(calls, 'invalid native repository reached Git source reads')

    def test_main_push_uses_production_twenty_minute_budget_in_both_shared_repositories(self):
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            budget = root / 'budget'
            before = int(time.time())
            run = subprocess.run(['bash', '-e', '-c', step_script(source, 'Start shared work budget')],
                                 env=dict(os.environ, GITHUB_OUTPUT=str(budget)), capture_output=True, text=True, timeout=3)
            self.assertEqual(run.returncode, 0, run.stderr)
            deadline = budget.read_text().strip().split('=', 1)[1]
            self.assertGreaterEqual(int(deadline), before + 1200)
            self.assertLessEqual(int(deadline), int(time.time()) + 1200)
            for repository in ('EffortlessMetrics/tokmd-swarm', 'EffortlessMetrics/tokmd'):
                context = dict(self.context, event_name='push', ref='refs/heads/codex/tokmd-queue-canary-base', repository=repository)
                for valid in (True, False):
                    with self.subTest(repository=repository, valid=valid):
                        candidate = deadline if valid else str(int(time.time()) + 360 * 60)
                        run, output, calls = self.run_native_entrypoint(root, dict(repository=dict(full_name=repository)),
                                                                       context, {}, candidate)
                        self.assertEqual(run.returncode, 0 if valid else 1, run.stderr)
                        self.assertIsNone(calls, 'main pushes must not query PR refs')
                        if valid:
                            self.assertIn('mode=main-push\n', output)
                            self.assertIn(f'deadline_epoch={deadline}\n', output)
                        else:
                            self.assertEqual(output, '')

    def test_native_explicit_request_admits_real_pr_composition(self):
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
        cases = [dict(event, pull_request=dict(event["pull_request"], draft=True)),
                 dict(event, pull_request=dict(event["pull_request"], body="")),
                 dict(event, pull_request=dict(event["pull_request"], state="closed")), dict(event, number=735)]
        for candidate in cases:
            with self.subTest(candidate=candidate), self.assertRaises(ValueError):
                Q.admit(candidate, context, lambda pr: refs)
        with self.assertRaisesRegex(ValueError, "merge composition is stale"):
            Q.admit(event, context, lambda pr: dict(refs, **{"refs/pull/734/merge": "d" * 40}))


    def test_request_expiry_is_shared_and_rejects_invalid_deadlines_before_source_reads(self):
        event, context, refs = self.native_request()
        request = Q.request_inputs(event, context)[0]
        for deadline in ("", "1", str(int(time.time()) + 1201)):
            candidate = dict(request, deadline_epoch=deadline)
            body = "<!-- tokmd-qualification:v1\n" + json.dumps(candidate) + "\n-->"
            changed = dict(event, pull_request=dict(event["pull_request"], body=body))
            def forbidden(pr):
                self.fail("invalid deadline must refuse before source reads")
            with self.subTest(deadline=deadline), self.assertRaisesRegex(ValueError, "request deadline"):
                Q.admit(changed, context, forbidden)

    def test_main_push_entrypoint_preserves_post_integration_admission_and_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            event, output = root / "event", root / "output"
            event.write_text(json.dumps(dict(repository=dict(full_name=Q.REPOSITORY))))
            deadline = str(int(time.time()) + 600)
            env = dict(os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_OUTPUT=str(output),
                       GITHUB_EVENT_NAME="push", GITHUB_REF="refs/heads/codex/tokmd-queue-canary-base", GITHUB_SHA=HEAD,
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

    def test_triggers_use_explicit_label_and_merge_group(self):
        triggers = self.source.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        self.assertIn("  pull_request:\n    types: [labeled]", triggers)
        self.assertIn("  merge_group:\n    types: [checks_requested]", triggers)
        self.assertNotIn("ready_for_review", triggers)
        self.assertNotIn("synchronize", triggers)
        self.assertIn("github.event.label.name == 'ci:qualify' && !github.event.pull_request.draft", self.source)
        self.assertIn("github.event.pull_request.draft) && ' (qualification not requested)'", self.source)

    def test_non_intent_and_draft_events_never_start_the_gate(self):
        for job in ("admission", "tokmd-rust-result"):
            body = self.source.split(f"  {job}:\n", 1)[1]
            expression = re.search(r"(?m)^    if: \${{ (.+) }}$", body).group(1)
            program = "const [exp,event,label,draft]=process.argv.slice(1);const github={event_name:event,event:{label:{name:label},pull_request:{draft:draft==='true'}}};const always=()=>true;process.stdout.write(String(eval(exp)));"
            for event, label, draft, expected in (("pull_request", "ci:qualify", "true", "false"),
                    ("pull_request", "ordinary", "false", "false"),
                    ("pull_request", "ci:qualify", "false", "true"), ("merge_group", "", "false", "true")):
                with self.subTest(job=job, event=event, label=label, draft=draft):
                    run = subprocess.run([NODE, "-e", program, expression, event, label, draft], capture_output=True, text=True)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    self.assertEqual(run.stdout, expected)

    def test_advisory_review_uses_artifact_only_with_read_only_token(self):
        self.assertIn("          posting: artifact-only", self.source)

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

    def test_merge_group_route_uses_hosted_runner_even_with_idle_self_hosted_capacity(self):
        body = step_script(self.source, "Decide gate runner (self-hosted primary, gh-hosted overflow)")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            script = body.replace("${{ github.event_name }}", "merge_group").replace(
                "${{ github.event.pull_request.head.repo.fork }}", "false")
            run = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script],
                                 env=dict(os.environ, GITHUB_OUTPUT=str(output), GH_TOKEN="test-only"),
                                 capture_output=True, text=True, timeout=3)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertIn("runner_kind=github\n", output.read_text())
        self.assertIn("github.event_name != 'merge_group' && secrets.EM_RUNNER_READ_TOKEN", self.source)

    def test_all_old_jobs_depend_on_real_admission_and_required_name_is_preserved(self):
        jobs = re.split(r"^  ([a-z][a-z0-9-]*):\n", self.source.split("jobs:\n", 1)[1], flags=re.MULTILINE)
        seen = 0
        for name, body in zip(jobs[1::2], jobs[2::2]):
            if name in ("admission", "queue-gate"):
                continue
            seen += 1
            self.assertRegex(body, r"(?m)^    needs:.*admission|^      - admission$")
            if name == "tokmd-rust-result":
                self.assertIn('    name: Tokmd Rust Result${{', body)
                self.assertIn("    if: ${{ always() &&", body)
            else:
                self.assertIn("needs.admission.outputs.admitted == 'true'", body)
        self.assertEqual(seen, 19)

    def test_expensive_jobs_wait_for_the_blocking_baseline(self):
        jobs = re.split(r"^  ([a-z][a-z0-9-]*):\n", self.source.split("jobs:\n", 1)[1], flags=re.MULTILINE)
        for name, body in zip(jobs[1::2], jobs[2::2]):
            if name in ("admission", "route", "deny", "detect", "queue-gate"):
                continue
            with self.subTest(job=name):
                self.assertIn("deny", re.search(r"^    needs:(.*?)(?=^    [a-z]|\Z)", body, re.MULTILINE | re.DOTALL).group(1))
                if name != "tokmd-rust-result":
                    self.assertIn("needs.deny.result == 'success'", body)

    def test_actual_check_name_preserves_qualification_and_does_not_overwrite_it_on_observation(self):
        body = self.source.split("  tokmd-rust-result:\n", 1)[1].split("  detect:\n", 1)[0]
        expression = re.search(r"^    name: (.+)$", body, re.MULTILINE).group(1)
        program = r"""const [template,event,action,label]=process.argv.slice(1);
const github={event_name:event,event:{action,label:{name:label},pull_request:{draft:false}}};
process.stdout.write(template.replace(/\$\{\{ (.*?) \}\}/g,(_,expression)=>String(eval(expression))));"""
        for event, action, label, expected in (("pull_request","opened","","Tokmd Rust Result (qualification not requested)"),
            ("pull_request","ready_for_review","","Tokmd Rust Result (qualification not requested)"),
            ("pull_request","labeled","ordinary","Tokmd Rust Result (qualification not requested)"),
            ("pull_request","labeled","ci:qualify","Tokmd Rust Result"),
            ("workflow_dispatch","","","Tokmd Rust Result"),("push","","","Tokmd Rust Result")):
            run = subprocess.run([NODE,"-e",program,expression,event,action,label],capture_output=True,text=True,timeout=3)
            self.assertEqual(run.returncode,0,run.stderr)
            self.assertEqual(run.stdout,expected)

    def test_required_runner_uses_the_route_or_a_refusal_capable_fallback(self):
        body = self.source.split('  tokmd-rust-result:\n', 1)[1].split('  detect:\n', 1)[0]
        expression = re.search(r'^    runs-on: \$\{\{ (.+) \}\}$', body, re.MULTILINE).group(1)
        program = """const [expression,runner]=process.argv.slice(1);
const needs={route:{outputs:{runner}}}; const fromJSON=JSON.parse;
process.stdout.write(JSON.stringify(eval(expression)));"""
        for runner, expected in (('', 'ubuntu-latest'), ('"ubuntu-latest"', 'ubuntu-latest'),
                ('["self-hosted","em-ci","trusted-pr","rust-small"]',
                 ['self-hosted', 'em-ci', 'trusted-pr', 'rust-small'])):
            with self.subTest(runner=runner):
                run = subprocess.run([NODE, '-e', program, expression, runner], capture_output=True, text=True, timeout=3)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(json.loads(run.stdout), expected)

    def test_actual_required_refusal_step_fails_without_successful_admission(self):
        body = step_script(self.source, "Refuse unrequested or failed admission")
        for admitted, result, expected in (("", "failure", 1), ("true", "failure", 1), ("", "success", 1), ("true", "success", 0)):
            with self.subTest(admitted=admitted, result=result):
                run = subprocess.run(["bash", "-e", "-c", body], env=dict(os.environ, ADMITTED=admitted, ADMISSION_RESULT=result, BASELINE_RESULT="success"),
                                     capture_output=True, text=True, timeout=3)
                self.assertEqual(run.returncode, expected)
                if expected:
                    self.assertIn(f'::error::admission={result} admitted={admitted} baseline=success', run.stderr)
                else:
                    self.assertEqual(run.stderr, '')

    def test_control_plane_python_selectors_cover_each_module_once(self):
        policy = (ROOT / 'ci/proof.toml').read_text()
        scope = next(block for block in policy.split('[[scope]]') if '\nname = "proof_control_plane"\n' in block)
        proof = scope.split('\nproof = [\n', 1)[1].split('\n]', 1)[0]
        selected = []
        for command in re.findall(r'^  "(python3 -m unittest .+)",$', proof, re.MULTILINE):
            if command.startswith('python3 -m unittest discover -s scripts -p '):
                pattern = command.split(' -p ', 1)[1]
                selected.extend(path.name for path in (ROOT / 'scripts').glob(pattern))
            else:
                path = ROOT / command.removeprefix('python3 -m unittest ')
                self.assertTrue(path.is_file(), command)
                selected.append(path.name)
        expected = {path.name: 1 for path in (ROOT / 'scripts').glob('ci_*_test.py')}
        self.assertEqual(dict(Counter(selected)), expected, 'control proof duplicates or omits a test module')

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
            result = subprocess.run([NODE, "-e", program, expression, json.dumps(context)], capture_output=True, text=True, timeout=3)
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
const steps = {review_budget: {outcome: 'success'}};
process.stdout.write(String(eval(expression)));"""
        for name in ("Record UB Review start", "UB Review (advisory)"):
            block = re.findall(rf"^      - name: {re.escape(name)}\n(.*?)(?=^      - |^  [a-z][a-z0-9-]*:|\Z)", self.source, re.MULTILINE | re.DOTALL)[0]
            condition = re.search(r"^        if: \$\{\{ (.+) \}\}$", block, re.MULTILINE).group(1)
            for prior, fork, expected in (("red-baseline", "false", "false"), ("stale-source", "false", "false"),
                                          ("success", "false", "true"), ("success", "true", "false")):
                with self.subTest(step=name, prior=prior, fork=fork):
                    run = subprocess.run([NODE, "-e", program, condition, prior, fork], capture_output=True, text=True, timeout=3)
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
                run = subprocess.run([NODE, "-e", program, output, condition, before, after, baseline], capture_output=True, text=True, timeout=3)
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
            }
            (root / "scripts").mkdir()
            shutil.copyfile(ROOT / "scripts/ci_budget.py", root / "scripts/ci_budget.py")
            for name, script in scripts.items():
                path = bindir / name
                path.write_text(script)
                path.chmod(0o755)
            env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"], RUNNER_KIND="github",
                       GITHUB_EVENT_NAME="workflow_dispatch", CALLS=str(root / "calls"), QUALIFICATION_DEADLINE="104", CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 125_000_000_000))
            result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", body], cwd=root, env=env, capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "calls").read_text().splitlines(), expected)
            receipts = [json.loads(path.read_text()) for path in (root / "target/ci-core").glob("*_exit.json")]
            self.assertEqual(len(receipts), 4)
            self.assertTrue(all(r["terminal_reason"] == "completed" for r in receipts))
            self.assertEqual({r["deadline_mono_ns"] for r in receipts}, {env["CI_BUDGET_MONO_NS"]})

    def test_actual_core_commands_share_one_deadline_and_expired_budget_launches_nothing(self):
        body = step_script(self.source, "Fast precontext and launch core gate")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bindir = root / "bin"
            bindir.mkdir()
            (root / "scripts").mkdir()
            shutil.copyfile(ROOT / "scripts/ci_budget.py", root / "scripts/ci_budget.py")
            cargo = bindir / "cargo"
            cargo.write_text('#!/bin/bash\necho "$*" >> "$CALLS"\nexit 0\n')
            cargo.chmod(0o755)
            env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"], RUNNER_KIND="github",
                       GITHUB_EVENT_NAME="workflow_dispatch", CALLS=str(root / "calls"), QUALIFICATION_DEADLINE="1", CI_BUDGET_MONO_NS="1")
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
