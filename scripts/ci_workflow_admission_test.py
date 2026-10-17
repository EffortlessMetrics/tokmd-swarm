"""Exercise the same production PR guard in the existing companion workflows."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import ci_qualification_test as request_fixture

ROOT, step_script = request_fixture.ROOT, request_fixture.step_script

WORKFLOWS = ('bindings-parity', 'ci-policy', 'clippy-exceptions-policy', 'cockpit', 'coverage',
             'no-panic-policy', 'pr-plan', 'proof-executor', 'ripr', 'test-action')


def jobs(source):
    parts = re.split(r'^  ([a-z][a-z0-9-]*):\n', source.split('jobs:\n', 1)[1], flags=re.MULTILINE)
    return list(zip(parts[1::2], parts[2::2]))


class CompanionAdmissionTests(unittest.TestCase):
    def test_actual_triggers_and_every_job_gate_precede_heavywork(self):
        count = 0
        for name in WORKFLOWS:
            source = (ROOT / f'.github/workflows/{name}.yml').read_text()
            pr = source.split('  pull_request:\n', 1)[1].split('\npermissions:', 1)[0]
            self.assertIn('    types: [labeled]\n', pr, name)
            self.assertNotRegex(pr, r'types:.*(?:opened|synchronize|ready_for_review|converted_to_draft)')
            for job, body in jobs(source):
                with self.subTest(workflow=name, job=job):
                    count += 1
                    guard = body.index('- name: Admit explicit PR qualification')
                    self.assertLess(body.index('uses: actions/checkout@'), guard)
                    after_checkout = body.split('uses: actions/checkout@', 1)[1].split('- name: Admit explicit PR qualification', 1)[0]
                    self.assertNotIn('cargo ', after_checkout)
                    for marker in ('uses: dtolnay/', 'uses: Swatinem/', 'uses: ./'):
                        if marker in body:
                            self.assertLess(guard, body.index(marker))
                    self.assertIn('Finalize explicit PR qualification budget', body)
        self.assertEqual(count, 16)

    def test_actual_guard_entrypoints_accept_subject_and_refuse_stale_or_unrequested(self):
        # Every job invokes the same checked-in shell and validator. Execute each
        # distinct literal body; equivalent copies need not repeat source reads.
        scripts = set()
        for name in WORKFLOWS:
            for _, body in jobs((ROOT / f'.github/workflows/{name}.yml').read_text()):
                scripts.add(step_script(body, 'Admit explicit PR qualification'))
        self.assertEqual(len(scripts), 1)
        fixture = request_fixture.AdmissionTests()
        fixture.setUp()
        event, context, refs = fixture.native_request()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            for name in ('ci_qualification.py', 'ci_budget.py'):
                shutil.copyfile(ROOT / 'scripts' / name, root / 'scripts' / name)
            bindir = root / 'bin'
            bindir.mkdir()
            git = bindir / 'git'
            git.write_text('#!/bin/bash\nif [ "$1" = rev-parse ]; then echo "$CHECKOUT_SHA"; else printf "%s\\n" "$SOURCE_REFS"; fi\n')
            git.chmod(0o755)
            event_path = root / 'event'
            for action, stale, checkout, expected in (('labeled', False, context['sha'], 0), ('opened', False, context['sha'], 1),
                    ('ready_for_review', False, context['sha'], 1), ('labeled', True, context['sha'], 1), ('labeled', False, 'd' * 40, 1)):
                with self.subTest(action=action, stale=stale, checkout=checkout):
                    output, environment = root / 'output', root / 'env'
                    output.unlink(missing_ok=True)
                    environment.unlink(missing_ok=True)
                    event_path.write_text(json.dumps(dict(event, action=action)))
                    source_refs = dict(refs, **({'refs/heads/main': 'e' * 40} if stale else {}))
                    env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ['PATH'], GITHUB_EVENT_PATH=str(event_path),
                        GITHUB_EVENT_NAME='pull_request', GITHUB_REPOSITORY=context['repository'], GITHUB_REF=context['ref'],
                        GITHUB_SHA=context['sha'], GITHUB_WORKFLOW_SHA=context['workflow_sha'], GITHUB_OUTPUT=str(output),
                        GITHUB_ENV=str(environment), CHECKOUT_SHA=checkout,
                        SOURCE_REFS='\n'.join(f'{sha}\t{ref}' for ref, sha in source_refs.items()))
                    run = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', next(iter(scripts))], cwd=root, env=env,
                                         capture_output=True, text=True, timeout=3)
                    self.assertEqual(run.returncode, expected, run.stderr)
                    if expected == 0:
                        self.assertIn('admitted=true\n', output.read_text())
                        self.assertIn('QUALIFICATION_DEADLINE=' + request_fixture.Q.request_inputs(event, context)[0]['deadline_epoch'] + '\n', environment.read_text())
                        self.assertIn('CI_BUDGET_MONO_NS=', environment.read_text())
                    else:
                        self.assertFalse(output.exists())

    def test_actual_companion_concurrency_never_collides_with_observation(self):
        program = """const [expression, label, sha] = process.argv.slice(1);
const github={event_name:'pull_request',event:{action:'labeled',label:{name:label},pull_request:{number:734}},sha,ref:'refs/pull/734/merge'};
const format=(s,...args)=>s.replace(/\\{(\\d+)\\}/g,(_,i)=>args[Number(i)]);
process.stdout.write(String(eval(expression)));"""
        for name in WORKFLOWS:
            source = (ROOT / f'.github/workflows/{name}.yml').read_text()
            block = source.split('concurrency:\n', 1)[1].split('\nenv:', 1)[0]
            expression = re.search(r'\$\{\{ (.+) \}\}', block).group(1)
            self.assertIn('cancel-in-progress: false', block)
            self.assertIn('queue: max', block)
            values = []
            for label, sha in (('ci:qualify', 'a' * 40), ('ordinary', 'a' * 40), ('ci:qualify', 'b' * 40)):
                run = subprocess.run(['node', '-e', program, expression, label, sha], capture_output=True, text=True, timeout=3)
                self.assertEqual(run.returncode, 0, run.stderr)
                values.append(run.stdout)
            self.assertEqual(len(set(values)), 3, name)

    def test_non_pr_failure_reporting_is_not_disabled_by_the_pr_gate(self):
        source = (ROOT / '.github/workflows/coverage.yml').read_text()
        block = re.search(r'^      - name: Fail on coverage command failure\n(.*?)(?=^      - |\Z)', source, re.MULTILINE | re.DOTALL).group(1)
        expression = re.search(r'^        if: \$\{\{ (.+) \}\}$', block, re.MULTILINE).group(1)
        program = """const [expression,event,qualified,phase]=process.argv.slice(1);
const github={event_name:event}; const always=()=>true;
const steps=new Proxy({}, {get:(_,key)=>({outcome:key==='qualification_gate'?qualified:phase,outputs:{overall_status:'1'}})});
process.stdout.write(String(eval(expression)));"""
        for event, qualified, phase, expected in (('push','skipped','skipped','true'),('workflow_dispatch','skipped','skipped','true'),
                    ('pull_request','failure','skipped','false'),('pull_request','success','failure','false'),('pull_request','success','success','true')):
            run = subprocess.run(['node','-e',program,expression,event,qualified,phase], capture_output=True,text=True,timeout=3)
            self.assertEqual(run.returncode,0,run.stderr)
            self.assertEqual(run.stdout,expected)

    def test_satellite_windows_and_companion_phases_refresh_the_same_deadline(self):
        paths = [ROOT / '.github/workflows/ci.yml'] + [ROOT / f'.github/workflows/{name}.yml' for name in WORKFLOWS]
        checked = 0
        for path in paths:
            for name, body in jobs(path.read_text()):
                if path.name == 'ci.yml' and name in ('admission','route','tokmd-rust-result'):
                    continue
                phases = re.findall(r'(?m)^ +id: (phase_budget_[0-9]+)$',body)
                with self.subTest(path=path.name,job=name):
                    self.assertTrue(phases)
                    for phase in phases:
                        self.assertIn(f'fromJSON(steps.{phase}.outputs.minutes',body)
                    self.assertIn('scripts/ci_budget.py allowance --reserve-seconds 0',body)
                    self.assertNotIn('fromJSON(steps.job_budget.outputs.minutes',body)
                    self.assertIn('scripts/ci_budget.py finalize',body)
                    checked += 1
        self.assertEqual(checked,33)


if __name__ == '__main__':
    unittest.main()
