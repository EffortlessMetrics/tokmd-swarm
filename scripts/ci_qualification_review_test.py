"""Production-path regressions for the independent continuation review."""

import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
from unittest.mock import patch

import ci_budget as budget

import ci_qualification_test as request_fixture

NODE = request_fixture.NODE
from ci_workflow_admission_test import WORKFLOWS, jobs

Q, ROOT, step_script = request_fixture.Q, request_fixture.ROOT, request_fixture.step_script


def block(source, name):
    match = re.search(r'^( +)- name: ' + re.escape(name) + r'\n(.*?)(?=^\1- |\Z)', source, re.MULTILINE | re.DOTALL)
    if not match:
        raise AssertionError(name)
    return match.group(2)


def script(body):
    value = body.split('run: ', 1)[1]
    return textwrap.dedent(value[2:]) if value.startswith('|\n') else value.splitlines()[0]


class ReviewRegressions(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_control_suite_uses_the_action_runtime_without_a_node_path_prerequisite(self):
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        body = block(source, 'Test qualification controls')
        pin = re.search(r'uses: (actions/github-script@[a-f0-9]{40})', body)
        self.assertIsNotNone(pin, 'mandatory control tests need an explicit action runtime')
        self.assertIn(pin.group(1), (ROOT / '.github/workflows/release.yml').read_text())
        program = textwrap.dedent(body.split('script: |\n', 1)[1])
        bindir = self.root / 'no-node-on-path'
        bindir.mkdir()
        record = self.root / 'runtime'
        python = bindir / 'python3'
        python.write_text('#!/bin/bash\nprintf "%s\\n" "$TOKMD_CI_NODE" "$@" > "$RUNTIME_RECORD"\nexit "$CHILD_STATUS"\n')
        python.chmod(0o755)
        node = os.path.realpath(request_fixture.NODE)
        if not os.path.isabs(node) or not os.path.exists(node):
            node = shutil.which(request_fixture.NODE)
        self.assertIsNotNone(node)
        for status in ('0', '2'):
            with self.subTest(status=status):
                env = dict(os.environ, PATH=str(bindir), RUNTIME_RECORD=str(record), CHILD_STATUS=status)
                run = subprocess.run([node, '-e', 'const core={setFailed:(message)=>{console.error(message);process.exitCode=1;}};\n' + program],
                    cwd=ROOT, env=env, capture_output=True, text=True, timeout=5)
                self.assertEqual(run.returncode, 0 if status == '0' else 1, run.stderr)
                self.assertEqual(record.read_text().splitlines(),
                    [node, '-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'ci_*_test.py'])

    def test_windows_matrix_shared_scripts_choose_bash(self):
        source = (ROOT / '.github/workflows/test-action.yml').read_text()
        body = dict(jobs(source))['test-action']
        self.assertIn('windows-latest', body)
        for name in ('Admit explicit PR qualification', 'Finalize explicit PR qualification budget'):
            self.assertIn('shell: bash', block(body, name), name)

    def test_non_linux_source_reads_use_the_portable_child_timeout(self):
        bindir = self.root / 'bin'
        bindir.mkdir()
        git = bindir / 'git'
        git.write_text('#!/bin/bash\nprintf "%s\\n" "' + 'a' * 40 + '\trefs/pull/734/head"\n')
        git.chmod(0o755)
        for platform in ('darwin', 'win32'):
            with self.subTest(platform=platform), patch.object(Q.sys, 'platform', platform), \
                    patch.dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ['PATH'],
                        CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 2_000_000_000)), \
                    patch.object(Q, 'run_command', side_effect=PermissionError('unsupported group observation')):
                self.assertEqual(Q.live_refs('734'), {'refs/pull/734/head': 'a' * 40})

    def test_phase_allocations_preserve_best_effort_actions_in_all_workflows(self):
        checked = 0
        for name in ('ci',) + WORKFLOWS:
            for job, body in jobs((ROOT / f'.github/workflows/{name}.yml').read_text()):
                indent = re.search(r'^( +)- (?:name|uses):', body, re.MULTILINE).group(1)
                steps = re.split(r'^' + indent + r'- ', body, flags=re.MULTILINE)[1:]
                for allocation, action in zip(steps, steps[1:]):
                    if not allocation.startswith('name: Allocate phase '):
                        continue
                    if not re.search(r'^' + indent + r'  continue-on-error: true$', action, re.MULTILINE):
                        continue
                    checked += 1
                    with self.subTest(workflow=name, job=job, phase=allocation.splitlines()[0]):
                        self.assertRegex(allocation, r'(?m)^' + indent + r'  continue-on-error: true$')
                        phase = re.search(r'^ +id: (phase_budget_[0-9]+)$', allocation, re.MULTILINE).group(1)
                        self.assertIn(f"steps.{phase}.outcome == 'success'", action)
        self.assertGreaterEqual(checked, 5)

    def test_advisory_review_leaves_two_minutes_for_selected_final_reporting(self):
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        env = dict(os.environ, CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 365_000_000_000),
            GITHUB_OUTPUT=str(self.root / 'review-output'))
        run = subprocess.run(['bash', '-e', '-c', script(block(source, 'Allocate advisory review budget'))],
            cwd=ROOT, env=env, capture_output=True, text=True, timeout=3)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual((self.root / 'review-output').read_text(), 'minutes=4\n')

    def test_actual_reporting_conditions_require_source_and_phase_admission_after_failure(self):
        source = (ROOT / '.github/workflows/ci.yml').read_text().split('  ci-actuals:\n', 1)[1]
        program = """const [expression,source,phase]=process.argv.slice(1);
const always=()=>true;
const steps=new Proxy({}, {get:(_,key)=>({outcome:key==='job_budget'?source:phase})});
process.stdout.write(String(eval(expression)));"""
        for name in ('Generate CI timings sidecar', 'Generate CI actuals receipt', 'Upload CI actuals receipt'):
            condition = re.search(r'^ +if: \$\{\{ (.+) \}\}$', block(source, name), re.MULTILINE).group(1)
            for admitted, phase, expected in (('success', 'success', 'true'),
                    ('failure', 'success', 'false'), ('success', 'failure', 'false'), ('', '', 'false')):
                with self.subTest(step=name, source=admitted, phase=phase):
                    run = subprocess.run([NODE, '-e', program, condition, admitted, phase],
                        capture_output=True, text=True, timeout=3)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    self.assertEqual(run.stdout, expected)

    def test_process_creation_does_not_renew_the_retained_endpoint(self):
        real_popen = subprocess.Popen
        def delayed(*args, **kwargs):
            time.sleep(.2)
            return real_popen(*args, **kwargs)
        with patch.dict(os.environ, CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 300_000_000)), \
                patch.object(budget.subprocess, 'Popen', side_effect=delayed):
            started = time.monotonic()
            status = budget.run_command([sys.executable, '-c', 'import time; time.sleep(2)'],
                self.root / 'spawn', grace=.04, reserve=0)
            self.assertEqual(status, 124)
            self.assertLess(time.monotonic() - started, .42, 'process creation renewed the admitted budget')

    def test_fresh_satellite_guard_then_native_environment_injection_admits_next_phase(self):
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        body = dict(jobs(source))['msrv']
        fixture = request_fixture.AdmissionTests()
        fixture.setUp()
        event, context, refs = fixture.native_request()
        (self.root / 'scripts').mkdir()
        for name in ('ci_qualification.py', 'ci_budget.py'):
            shutil.copyfile(ROOT / 'scripts' / name, self.root / 'scripts' / name)
        bindir = self.root / 'bin'
        bindir.mkdir()
        git = bindir / 'git'
        git.write_text('#!/bin/bash\nif [ "$1" = rev-parse ]; then echo "$CHECKOUT_SHA"; else printf "%s\\n" "$SOURCE_REFS"; fi\n')
        git.chmod(0o755)
        event_path, output, environment = (self.root / name for name in ('event', 'output', 'environment'))
        event_path.write_text(json.dumps(event))
        env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ['PATH'], GITHUB_EVENT_PATH=str(event_path),
            GITHUB_EVENT_NAME='pull_request', GITHUB_REPOSITORY=context['repository'], GITHUB_REF=context['ref'],
            GITHUB_SHA=context['sha'], GITHUB_WORKFLOW_SHA=context['workflow_sha'], GITHUB_OUTPUT=str(output),
            GITHUB_ENV=str(environment), CHECKOUT_SHA=context['sha'],
            QUALIFICATION_DEADLINE=Q.request_inputs(event, context)[0]['deadline_epoch'],
            SOURCE_REFS='\n'.join(f'{sha}\t{ref}' for ref, sha in refs.items()))
        env.pop('CI_BUDGET_MONO_NS', None)
        run = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', script(block(body, 'Revalidate shared qualification budget'))],
            cwd=self.root, env=env, capture_output=True, text=True, timeout=4)
        self.assertEqual(run.returncode, 0, run.stderr)
        injected = dict(line.split('=', 1) for line in environment.read_text().splitlines())
        self.assertIn('CI_BUDGET_MONO_NS', injected)
        env.update(injected)  # GitHub injects GITHUB_ENV only at this step boundary.
        run = subprocess.run(['bash', '-e', '-c', script(block(body, 'Allocate phase 2 budget'))],
            cwd=self.root, env=env, capture_output=True, text=True, timeout=3)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertIn('minutes=', output.read_text())

    def test_normal_or_failed_leader_exit_settles_descendant_and_stdout(self):
        for status in (0, 2):
            with self.subTest(status=status):
                pidfile = self.root / 'descendant'
                pidfile.unlink(missing_ok=True)
                code = '''import subprocess,sys
p=subprocess.Popen([sys.executable,"-c","import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(10)"])
open(sys.argv[1],"w").write(str(p.pid))
raise SystemExit(int(sys.argv[2]))
'''
                command = [sys.executable, str(ROOT / 'scripts/ci_budget.py'), 'run', '--marker', str(self.root / 'marker'),
                    '--grace-seconds', '0.08', '--reserve-seconds', '0', '--', sys.executable, '-c', code, str(pidfile), str(status)]
                try:
                    run = subprocess.run(command, env=dict(os.environ, CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 2_000_000_000)),
                        capture_output=True, text=True, timeout=3)
                    self.assertEqual(run.returncode, status)
                    descendant = int(pidfile.read_text())
                    path = Path(f'/proc/{descendant}/stat')
                    state = path.read_text().split()[2] if path.exists() else None
                    self.assertIn(state, (None, 'Z'), 'owned descendant still executing')
                    self.assertEqual(json.loads((self.root / 'marker.json').read_text())['terminal_reason'],
                        'completed' if status == 0 else 'source_failed')
                finally:
                    if pidfile.exists():
                        try:
                            os.kill(int(pidfile.read_text()), signal.SIGKILL)
                        except ProcessLookupError:
                            pass

    def test_optional_refusal_skips_action_without_failing_green_core(self):
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        optional = (('Allocate cache budget', 'Restore Rust build cache (best effort)', 'cache_budget'),
            ('Allocate advisory review budget', 'UB Review (advisory)', 'review_budget'),
            ('Allocate receipt upload budget', 'Upload UB Review observability receipt', 'upload_budget'))
        env = dict(os.environ, CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 55_000_000_000), GITHUB_OUTPUT=str(self.root / 'output'))
        for allocator, action, identifier in optional:
            with self.subTest(allocator=allocator):
                allocation = block(source, allocator)
                self.assertIn('continue-on-error: true', allocation)
                run = subprocess.run(['bash', '-e', '-c', script(allocation)], cwd=ROOT, env=env, capture_output=True, timeout=3)
                self.assertEqual(run.returncode, 124)
                condition = re.search(r'^ +if: \$\{\{ (.+) \}\}', block(source, action), re.MULTILINE).group(1)
                program = """const condition=process.argv[1]; const success=()=>true,always=()=>true,cancelled=()=>false;
const github={event_name:'pull_request',event:{pull_request:{head:{repo:{fork:false}}}}};
const needs={admission:{outputs:{admitted:'true'}}};
const steps=new Proxy({}, {get:()=>({outcome:'failure'})});process.stdout.write(String(eval(condition)));"""
                call = subprocess.run([NODE, '-e', program, condition], capture_output=True, text=True, timeout=3)
                self.assertEqual(call.returncode, 0, call.stderr)
                self.assertEqual(call.stdout, 'false', identifier)
        markers = self.root / 'target/ci-core'
        markers.mkdir(parents=True)
        for marker in ('core_exit', 'test_exit', 'xtask_test_exit', 'proof_policy_exit'):
            (markers / marker).write_text('0\n')
        run = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', step_script(source, 'Assert core gate verdict')], cwd=self.root,
            env=dict(env, RUNNER_KIND='self-hosted', GITHUB_STEP_SUMMARY=str(self.root / 'summary')), capture_output=True, text=True, timeout=3)
        self.assertEqual(run.returncode, 0, run.stderr)
        run = subprocess.run([sys.executable, str(ROOT / 'scripts/ci_budget.py'), 'finalize', '--receipt', str(self.root / 'final.json')],
            env=env, capture_output=True, timeout=3)
        self.assertEqual(run.returncode, 0)

    def test_near_expiry_source_read_is_bounded_and_cleaned_up(self):
        bindir = self.root / 'bin'
        bindir.mkdir()
        pidfile = self.root / 'source-pid'
        git = bindir / 'git'
        git.write_text('#!' + sys.executable + '\nimport os,time\nopen(os.environ["SOURCE_PID"],"w").write(str(os.getpid()))\ntime.sleep(10)\n')
        git.chmod(0o755)
        code = 'import ci_qualification as q; q.live_refs("734")'
        env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ['PATH'], PYTHONPATH=str(ROOT / 'scripts'),
            CI_BUDGET_MONO_NS=str(time.monotonic_ns() + 400_000_000), QUALIFICATION_DEADLINE=str(int(time.time()) + 600), SOURCE_PID=str(pidfile))
        started = time.monotonic()
        try:
            run = subprocess.run([sys.executable, '-c', code], env=env, capture_output=True, text=True, timeout=2)
            self.assertNotEqual(run.returncode, 0)
            self.assertLess(time.monotonic() - started, 1.5)
            if pidfile.exists():
                path = Path(f'/proc/{int(pidfile.read_text())}/stat')
                self.assertIn(path.read_text().split()[2] if path.exists() else None, (None, 'Z'))
        finally:
            if pidfile.exists():
                try:
                    os.kill(int(pidfile.read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass


if __name__ == '__main__':
    unittest.main()
