"""Production-path regressions for the independent continuation review."""

import json
from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import re
import shlex
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
            found = shutil.which(request_fixture.NODE)
            self.assertIsNotNone(found)
            node = os.path.realpath(found)
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
                        QUALIFICATION_DEADLINE=str(int(time.time()) + 600),
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


class HeavyStepBudgetTests(unittest.TestCase):
    CASES = (
        ('ci', 'detect', 'Compute risk-pack output flags', 'risk-detection'),
        ('ci', 'fast-proof-run', 'Resolve fast proof-run policy', 'fast-policy'),
        ('coverage', 'coverage', 'Generate coverage', 'coverage'),
    )

    def run_step(self, case, *, seconds=5, event='pull_request', cargo_status=0, delay=0,
                 startup_delay=0, expire_after_cargo=False):
        workflow, job, name, marker = case
        body = dict(jobs((ROOT / f'.github/workflows/{workflow}.yml').read_text()))[job]
        indent = re.search(r'^( +)- (?:name|uses|id):', body, re.MULTILINE).group(1)
        steps = re.split(r'^' + indent + r'- ', body, flags=re.MULTILINE)[1:]
        step = next(value for value in steps if re.search(r'(?m)^ *name: ' + re.escape(name) + '$', value))
        shell = re.search(r'(?m)^ +shell: (.+)$', step)
        shell = shell.group(1) if shell else 'bash --noprofile --norc -e -o pipefail "{0}"'
        command = textwrap.dedent(step.split('run: |\n', 1)[1])
        command = command.replace('${{ github.event_name }}', event).replace('${{ github.base_ref }}', 'main')
        with tempfile.TemporaryDirectory(prefix='tokmd budget step ') as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            shutil.copyfile(ROOT / 'scripts/ci_budget.py', root / 'scripts/ci_budget.py')
            if expire_after_cargo:
                # Execute unchanged production code with a process-local clock.
                # Expiry waits for Cargo admission, so wrapper startup consumes
                # none of the test's narrow timeout-observation window.
                (root / 'scripts/ci_budget.py').rename(root / 'scripts/ci_budget_runtime.py')
                (root / 'scripts/ci_budget.py').write_text(textwrap.dedent('''\
                    import os, runpy, time
                    from pathlib import Path

                    real_ns = time.monotonic_ns
                    offset = None

                    def clock_ns():
                        global offset
                        now = real_ns()
                        if offset is None and Path('cargo-ready').exists():
                            offset = int(os.environ['CI_BUDGET_MONO_NS']) - now + 1_000_000_000
                        return now + (offset or 0)

                    time.monotonic_ns = clock_ns
                    time.monotonic = lambda: clock_ns() / 1_000_000_000
                    runpy.run_path('scripts/ci_budget_runtime.py', run_name='__main__')
                    '''))
            (root / 'bin').mkdir()
            cargo = root / 'bin/cargo'
            cargo.write_text('#!' + sys.executable + '\n' + textwrap.dedent('''\
                import json, os, pathlib, sys, time
                with open('calls', 'a') as output:
                    print(json.dumps(sys.argv[1:]), file=output, flush=True)
                # Publish readiness only after both the call and its real-clock
                # timestamp are complete; the wrapper may terminate us next.
                ready = pathlib.Path('cargo-ready.tmp')
                ready.write_text(str(time.monotonic_ns()))
                ready.replace('cargo-ready')
                time.sleep(float(os.environ['STUB_DELAY']))
                if int(os.environ['STUB_STATUS']):
                    sys.exit(int(os.environ['STUB_STATUS']))
                for flag in ('--github-output', '--output-path'):
                    if flag in sys.argv:
                        path = pathlib.Path(sys.argv[sys.argv.index(flag) + 1])
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.write_text('profile=fast\\nartifact_name=fast-proof-run\\n' if flag == '--github-output' else '{}\\n')
                print('TOTAL 1 1 100%')
                '''))
            cargo.chmod(0o755)
            script_path = root / 'step.sh'
            script_path.write_text(command)
            env = dict(os.environ, PATH=str(root / 'bin') + os.pathsep + os.environ['PATH'],
                       GITHUB_EVENT_NAME=event, GITHUB_OUTPUT=str(root / 'output'), GITHUB_STEP_SUMMARY=str(root / 'summary'),
                       LABELS_JSON='[]', PUSH_BEFORE='a' * 40, STUB_STATUS=str(cargo_status), STUB_DELAY=str(delay))
            env.pop('CI_BUDGET_MONO_NS', None)
            if event == 'pull_request' or workflow == 'ci':
                env['CI_BUDGET_MONO_NS'] = str(time.monotonic_ns() + int(seconds * 1e9))
            started = time.monotonic()
            time.sleep(startup_delay)
            run = subprocess.run(shlex.split(shell.replace('{0}', str(script_path))),
                                 cwd=root, env=env, capture_output=True, text=True, timeout=7)
            calls = [json.loads(line) for line in (root / 'calls').read_text().splitlines()] if (root / 'calls').exists() else []
            receipt = root / f'target/ci-budget/{marker}.json'
            budget_receipt = json.loads(receipt.read_text()) if receipt.exists() else None
            if budget_receipt is not None:
                self.assertEqual(budget_receipt['deadline_mono_ns'], env['CI_BUDGET_MONO_NS'])
            if expire_after_cargo:
                # Bound cleanup from admitted Cargo, independent of startup.
                # The child interpreter retains the real monotonic clock.
                started = int((root / 'cargo-ready').read_text()) / 1_000_000_000
            return (run, calls, budget_receipt,
                    (root / 'output').read_text() if (root / 'output').exists() else '', time.monotonic() - started)

    def test_expired_budget_prevents_every_previously_unbounded_step(self):
        for case in self.CASES:
            with self.subTest(step=case[2]):
                run, calls, receipt, _, _ = self.run_step(case, seconds=-1)
                self.assertEqual(run.returncode, 124, run.stderr)
                self.assertEqual(calls, [])
                self.assertEqual(receipt['terminal_reason'], 'budget_exhausted')
                self.assertFalse(receipt['child_started'])

    def test_admitted_steps_retain_commands_outputs_and_budget_receipts(self):
        # Independent temporary process groups can share the test run's wait for
        # the production two-second cleanup grace; keep the 55s suite budget.
        with ThreadPoolExecutor(max_workers=3) as pool:
            results = list(pool.map(self.run_step, self.CASES))
        for case, count, result in zip(self.CASES, (1, 2, 5), results):
            with self.subTest(step=case[2]):
                run, calls, receipt, output, _ = result
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(len(calls), count)
                self.assertEqual(receipt['terminal_reason'], 'completed')
                if case[0] == 'coverage':
                    self.assertIn('overall_status=0\n', output)
                elif case[1] == 'fast-proof-run':
                    self.assertIn('profile=fast\n', output)

    def test_command_failure_and_budget_timeout_stay_non_passing(self):
        with ThreadPoolExecutor(max_workers=3) as pool:
            results = list(pool.map(lambda case: self.run_step(case, cargo_status=23), self.CASES))
        for case, result in zip(self.CASES[:2], results[:2]):
            with self.subTest(step=case[2]):
                run, calls, receipt, _, _ = result
                self.assertEqual(run.returncode, 23, run.stderr)
                self.assertEqual(len(calls), 1)
                self.assertEqual(receipt['terminal_reason'], 'source_failed')
        run, calls, receipt, output, _ = results[2]
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(len(calls), 1)
        self.assertEqual(receipt['terminal_reason'], 'completed')
        self.assertIn('overall_status=1\n', output, 'coverage command failure must retain its existing downstream failure signal')
        with ThreadPoolExecutor(max_workers=3) as pool:
            results = list(pool.map(lambda delay: self.run_step(self.CASES[1], seconds=60, delay=10,
                                startup_delay=delay, expire_after_cargo=True), (0, .55, 2.2)))
        for run, calls, receipt, output, elapsed in results:
            self.assertEqual(run.returncode, 124, run.stderr)
            self.assertEqual(len(calls), 1, 'a second Cargo command escaped the retained endpoint')
            self.assertEqual(receipt['terminal_reason'], 'budget_exhausted')
            self.assertTrue(receipt['child_started'])
            self.assertTrue(receipt['timed_out'])
            self.assertNotIn('profile=fast', output)
            self.assertLess(elapsed, 4)

    def test_delayed_launch_refusal_is_distinct_from_an_executed_timeout(self):
        # Negative control: the old 2.4s fixture leaves <0.4s for startup after
        # the unchanged production 2s grace. A delayed launch correctly refuses
        # admission; it must never be accepted as proof of an executed timeout.
        run, calls, receipt, output, _ = self.run_step(self.CASES[1], seconds=2.4,
                                                    startup_delay=.55, delay=10)
        self.assertEqual(run.returncode, 124, run.stderr)
        self.assertEqual(calls, [])
        self.assertEqual(receipt['terminal_reason'], 'budget_exhausted')
        self.assertFalse(receipt['child_started'])
        self.assertFalse(receipt['timed_out'])
        self.assertNotIn('profile=fast', output)

    def test_non_pr_coverage_still_runs_without_a_pr_budget_anchor(self):
        for event in ('push', 'workflow_dispatch'):
            with self.subTest(event=event):
                run, calls, receipt, output, _ = self.run_step(self.CASES[2], event=event)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(len(calls), 5)
                self.assertIsNone(receipt)
                self.assertIn('overall_status=0\n', output)


if __name__ == '__main__':
    unittest.main()
