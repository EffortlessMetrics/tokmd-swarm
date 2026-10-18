"""Real child process deadline/cancellation and production phase budget tests."""

import json
import os
from pathlib import Path
import signal
import shutil
import shlex
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
BUDGET = ROOT / 'scripts/ci_budget.py'


class BudgetTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def env(self, seconds):
        return dict(os.environ, CI_BUDGET_MONO_NS=str(time.monotonic_ns() + int(seconds * 1e9)))

    def command(self, marker, code):
        return [sys.executable, str(BUDGET), 'run', '--marker', str(self.root / marker),
                '--grace-seconds', '0.08', '--reserve-seconds', '0.1', '--', sys.executable, '-c', code]

    def test_actual_children_consume_same_budget_across_phases(self):
        env = self.env(0.65)
        start = time.monotonic()
        first = subprocess.run(self.command('setup', 'import time; time.sleep(0.15)'), env=env, timeout=3)
        second = subprocess.run(self.command('core', 'import time; time.sleep(3)'), env=env, timeout=3)
        third = subprocess.run(self.command('advisory', 'raise Exception("must not launch")'), env=env, timeout=3)
        self.assertEqual((first.returncode, second.returncode, third.returncode), (0, 124, 124))
        self.assertLess(time.monotonic() - start, 1.8)
        receipt = json.loads((self.root / 'advisory.json').read_text())
        self.assertFalse(receipt['child_started'])

    def test_source_failures_including124_and137_are_preserved_as_source_failures(self):
        for status in (2, 124, 137):
            with self.subTest(status=status):
                run = subprocess.run(self.command('source', f'raise SystemExit({status})'), env=self.env(3), timeout=4)
                self.assertEqual(run.returncode, status)
                self.assertEqual(json.loads((self.root / 'source.json').read_text())['terminal_reason'], 'source_failed')

    def test_timeout_kills_term_ignoring_child_group_and_records_truthful_marker(self):
        pidfile = self.root / 'descendant'
        code = '''import subprocess,sys,time,signal
signal.signal(signal.SIGTERM, signal.SIG_IGN)
p=subprocess.Popen([sys.executable,"-c","import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(30)"])
open(sys.argv[1],"w").write(str(p.pid))
time.sleep(30)
'''
        command = self.command('timeout', code)
        command.append(str(pidfile))
        run = subprocess.run(command, env=self.env(0.6), timeout=3)
        self.assertEqual(run.returncode, 124)
        descendant = int(pidfile.read_text())
        proc = Path(f'/proc/{descendant}/stat')
        try:
            state = proc.read_text().split()[2]
        except FileNotFoundError:
            state = None
        self.assertIn(state, (None, 'Z'), 'descendant still executing')
        receipt = json.loads((self.root / 'timeout.json').read_text())
        self.assertEqual(receipt['terminal_reason'], 'budget_exhausted')
        self.assertTrue(receipt['timed_out'])

    def test_cancellation_terminates_unique_child_and_next_command_can_recover(self):
        ready = self.root / 'ready'
        command = self.command('cancel', f'import pathlib,time; pathlib.Path({str(ready)!r}).touch(); time.sleep(30)')
        process = subprocess.Popen(command, env=self.env(10))
        self.addCleanup(lambda: process.poll() is None and process.kill())
        limit = time.monotonic() + 2
        while not ready.exists() and time.monotonic() < limit:
            time.sleep(.01)
        self.assertTrue(ready.exists())
        process.send_signal(signal.SIGTERM)
        self.assertEqual(process.wait(timeout=3), 143)
        self.assertEqual(json.loads((self.root / 'cancel.json').read_text())['terminal_reason'], 'cancelled')
        recovery = subprocess.run(self.command('recovery', 'raise SystemExit(0)'), env=self.env(3), timeout=4)
        self.assertEqual(recovery.returncode, 0)

    def test_action_allowance_and_finalization_share_same_unreset_anchor(self):
        output, receipt = self.root / 'output', self.root / 'final.json'
        env = self.env(125)
        run = subprocess.run([sys.executable, str(BUDGET), 'allowance', '--output', str(output)], env=env, timeout=3)
        self.assertEqual(run.returncode, 0)
        self.assertEqual(output.read_text(), 'minutes=1\n')
        for seconds, expected in ((1, 0), (-1, 124)):
            run = subprocess.run([sys.executable, str(BUDGET), 'finalize', '--receipt', str(receipt)], env=self.env(seconds), timeout=3)
            self.assertEqual(run.returncode, expected)
            self.assertEqual(json.loads(receipt.read_text())['terminal_reason'], 'within_budget' if expected == 0 else 'budget_exhausted')

    def test_actual_setup_anchor_and_action_phases_refuse_expired_budget(self):
        from ci_qualification_test import step_script
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        body = step_script(source, 'Anchor shared setup budget')
        output, environment = self.root / 'output', self.root / 'env'
        env = dict(os.environ, QUALIFICATION_DEADLINE=str(int(time.time()) + 190),
                   GITHUB_OUTPUT=str(output), GITHUB_ENV=str(environment))
        run = subprocess.run(['bash', '-e', '-c', body], env=env, timeout=3)
        self.assertEqual(run.returncode, 0)
        anchor = dict(line.split('=', 1) for line in environment.read_text().splitlines())
        self.assertGreater(int(anchor['CI_BUDGET_MONO_NS']), time.monotonic_ns())
        self.assertEqual(output.read_text(), 'minutes=2\n')
        for phase in ('Allocate toolchain budget', 'Allocate cache budget', 'Allocate advisory review budget', 'Allocate receipt upload budget'):
            script = step_script(source, phase)
            run = subprocess.run(['bash', '-e', '-c', script], cwd=ROOT,
                                 env=dict(self.env(-1), GITHUB_OUTPUT=str(output)), capture_output=True, timeout=3)
            self.assertEqual(run.returncode, 124, phase)
        run = subprocess.run(['bash', '-e', '-c', body], env=dict(env, QUALIFICATION_DEADLINE='1'), timeout=3)
        self.assertEqual(run.returncode, 124)

    def test_production_producer_stops_after_real_child_cancellation(self):
        from ci_qualification_test import step_script
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        (self.root / 'scripts').mkdir()
        shutil.copyfile(BUDGET, self.root / 'scripts/ci_budget.py')
        bindir = self.root / 'bin'
        bindir.mkdir()
        ready, calls = self.root / 'ready', self.root / 'calls'
        cargo = bindir / 'cargo'
        cargo.write_text('#!/bin/bash\necho "$*" >> "$CALLS"\nif [ "$(wc -l < "$CALLS")" = 1 ]; then echo "$PPID" > "$READY"; sleep 10; fi\n')
        cargo.chmod(0o755)
        env = dict(self.env(125), PATH=str(bindir) + os.pathsep + os.environ['PATH'],
                   RUNNER_KIND='github', GITHUB_EVENT_NAME='pull_request', CALLS=str(calls), READY=str(ready))
        with open(self.root / 'producer.log', 'w') as log:
            process = subprocess.Popen(['bash', '-e', '-o', 'pipefail', '-c', step_script(source, 'Fast precontext and launch core gate')],
                                       cwd=self.root, env=env, stdout=log, stderr=subprocess.STDOUT)
            self.addCleanup(lambda: process.poll() is None and process.kill())
            until = time.monotonic() + 2
            while not ready.exists() and time.monotonic() < until:
                time.sleep(.01)
            self.assertTrue(ready.exists())
            os.kill(int(ready.read_text()), signal.SIGTERM)
            self.assertEqual(process.wait(timeout=5), 143)
        self.assertEqual(len(calls.read_text().splitlines()), 1)
        receipt = json.loads((self.root / 'target/ci-core/core_exit.json').read_text())
        self.assertEqual(receipt['terminal_reason'], 'cancelled')
        self.assertFalse((self.root / 'target/ci-core/test_exit').exists())

    def test_production_final_receipt_preserves_source124_and_cancellation(self):
        from ci_qualification_test import step_script
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        markers = self.root / 'target/ci-core'
        markers.mkdir(parents=True)
        env = dict(os.environ, UB_OUTCOME='skipped', RUNNER_KIND='github', HEAD_SHA='a' * 40,
                   GITHUB_WORKFLOW='CI', GITHUB_RUN_ID='734', GITHUB_RUN_ATTEMPT='1',
                   GITHUB_JOB='tokmd-rust-result', GITHUB_STEP_SUMMARY=str(self.root / 'summary'))
        for reason, status, expected in (('source_failed', 124, 'core_proof_failed'), ('cancelled', 143, 'core_proof_cancelled')):
            for marker in ('core_exit', 'test_exit', 'xtask_test_exit', 'proof_policy_exit'):
                (markers / marker).write_text(str(status if marker == 'core_exit' else 0) + '\n')
            (markers / 'core_exit.json').write_text(json.dumps(dict(terminal_reason=reason)))
            run = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', step_script(source, 'Write UB Review observability receipt')],
                                 cwd=self.root, env=env, capture_output=True, text=True, timeout=3)
            self.assertEqual(run.returncode, 0, run.stderr)
            receipt = json.loads((markers / 'ub-review-receipt.json').read_text())
            self.assertEqual(receipt['terminal_reason'], expected)
            self.assertEqual(receipt['timeout_classification'], 'not_timeout')

    def test_reporter_reuses_only_the_exact_selected_compiler_through_actual_boundary(self):
        from ci_qualification_test import step_script
        source = (ROOT / '.github/workflows/ci.yml').read_text()
        body = step_script(source, 'Reuse an already selected compiler')
        (self.root / 'scripts').mkdir()
        shutil.copyfile(BUDGET, self.root / 'scripts/ci_budget.py')
        script = self.root / 'probe.sh'
        script.write_text(body)
        bindir = self.root / 'bin'
        bindir.mkdir()
        rustup = bindir / 'rustup'
        rustup.write_text('#!/bin/bash\nprintf "%s\\n" "$INSTALLED_RUSTC"\n')
        rustup.chmod(0o755)
        shell = 'python3 scripts/ci_budget.py run --marker target/ci-budget/compiler-probe --reserve-seconds 0 -- bash --noprofile --norc -e -o pipefail {0}'
        self.assertIn('shell: ' + shell, source)
        for installed, expected in (('rustc 1.95.0 (qualified)', 'true'), ('rustc 1.94.0 (older)', 'false'), ('', 'false')):
            output, environment = self.root / 'output', self.root / 'environment'
            output.unlink(missing_ok=True)
            environment.unlink(missing_ok=True)
            run = subprocess.run(shlex.split(shell.replace('{0}', str(script))), cwd=self.root,
                env=dict(self.env(5), PATH=str(bindir) + os.pathsep + os.environ['PATH'], GITHUB_OUTPUT=str(output),
                         GITHUB_ENV=str(environment), EXPECTED_RUSTC='rustc 1.95.0 (qualified)', EXPECTED_TOOLCHAIN='stable-x86_64-unknown-linux-gnu', INSTALLED_RUSTC=installed),
                capture_output=True,text=True,timeout=4)
            self.assertEqual(run.returncode,0,run.stderr)
            self.assertEqual(output.read_text(),'ready=' + expected + '\n')
            self.assertEqual(environment.exists(),expected == 'true')


if __name__ == '__main__':
    unittest.main()
