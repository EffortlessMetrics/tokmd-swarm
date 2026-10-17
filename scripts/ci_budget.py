"""Existing CI command/phase deadline boundary; no dispatch, status or runner API."""

import argparse
import datetime
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def remaining(deadline_mono_ns=None):
    """Use the job's one monotonic anchor; refuse missing/invalid admission state."""
    end = int(os.environ['CI_BUDGET_MONO_NS'] if deadline_mono_ns is None else deadline_mono_ns)
    left = (end - time.monotonic_ns()) / 1_000_000_000
    if left > 20 * 60:
        raise ValueError('budget exceeds admission maximum')
    return left


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_text(json.dumps(value, sort_keys=True) + '\n', encoding='utf-8')
    temporary.replace(path)


def run_command(command, marker, grace=2.0, reserve=60.0, deadline_mono_ns=None, stdout=None, stderr=None):
    if not 0 < grace <= 2 or reserve < 0:
        raise ValueError('invalid cancellation/finalization reserve')
    started = time.monotonic_ns()
    interrupted = []
    old_handlers = {}
    process = None
    reason, status = 'budget_exhausted', 124
    timed_out = False
    def interrupt(signum, frame):
        interrupted.append(signum)
    for signum in (signal.SIGTERM, signal.SIGINT):
        old_handlers[signum] = signal.signal(signum, interrupt)
    try:
        allowance = remaining(deadline_mono_ns) - reserve - grace
        if allowance > 0:
            try:
                process = subprocess.Popen(command, start_new_session=True, stdout=stdout, stderr=stderr)
            except OSError:
                reason, status = 'spawn_failed', 127
            else:
                deadline = time.monotonic() + allowance
                def exited():
                    # Observe without reaping: the unique leader PID cannot be
                    # reused by an unrelated job before group cleanup finishes.
                    return os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is not None
                while not exited() and not interrupted and time.monotonic() < deadline:
                    time.sleep(min(0.02, max(0, deadline - time.monotonic())))
                timed_out = time.monotonic() >= deadline
                # A normally exited leader can leave a descendant holding stdout.
                # Settle its unique group before reaping on every terminal path.
                stop_signal = interrupted[0] if interrupted else signal.SIGTERM
                try:
                    os.killpg(process.pid, stop_signal)
                except ProcessLookupError:
                    pass
                until = time.monotonic() + grace
                while time.monotonic() < until:
                    time.sleep(min(0.01, max(0, until - time.monotonic())))
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                result = process.wait()
                if interrupted or timed_out:
                    reason = 'cancelled' if interrupted else 'budget_exhausted'
                    status = 128 + interrupted[0] if interrupted else 124
                else:
                    status = result if result >= 0 else 128 - result
                    reason = 'completed' if status == 0 else 'source_failed'
    finally:
        for signum, handler in old_handlers.items():
            signal.signal(signum, handler)
        receipt = dict(schema_version='tokmd.ci.command-budget.v1', command=command,
                       deadline_mono_ns=str(deadline_mono_ns) if deadline_mono_ns is not None else os.environ.get('CI_BUDGET_MONO_NS'),
                       terminal_reason=reason, exit=status, child_started=process is not None,
                       timed_out=timed_out, elapsed_ns=time.monotonic_ns() - started)
        write_json(str(marker) + '.json', receipt)
        Path(marker).write_text(str(status) + '\n', encoding='utf-8')
    return status


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    allowance = sub.add_parser('allowance')
    allowance.add_argument('--reserve-seconds', type=float, default=60)
    allowance.add_argument('--max-minutes', type=int, default=20)
    allowance.add_argument('--output', default=os.environ.get('GITHUB_OUTPUT'))
    runner = sub.add_parser('run')
    runner.add_argument('--marker', required=True)
    runner.add_argument('--grace-seconds', type=float, default=2)
    runner.add_argument('--reserve-seconds', type=float, default=60)
    runner.add_argument('command', nargs=argparse.REMAINDER)
    check = sub.add_parser('finalize')
    check.add_argument('--receipt', required=True)
    args = parser.parse_args()
    try:
        if args.mode == 'run':
            command = args.command[1:] if args.command[:1] == ['--'] else args.command
            if not command:
                raise ValueError('missing command')
            return run_command(command, args.marker, args.grace_seconds, args.reserve_seconds)
        left = remaining()
        if args.mode == 'finalize':
            write_json(args.receipt, dict(schema_version='tokmd.ci.budget-finalization.v1',
                terminal_reason='within_budget' if left > 0 else 'budget_exhausted',
                remaining_seconds=left, finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat()))
            return 0 if left > 0 else 124
        if not 1 <= args.max_minutes <= 20:
            raise ValueError('invalid native action cap')
        minutes = min(args.max_minutes, int((left - args.reserve_seconds) // 60))
        if minutes < 1:
            print('budget exhausted before action admission', file=sys.stderr)
            return 124
        with open(args.output, 'a', encoding='utf-8') as output:
            print(f'minutes={minutes}', file=output)
        return 0
    except (KeyError, ValueError, OSError) as error:
        print(f'budget refused: {error}', file=sys.stderr)
        return 124


if __name__ == '__main__':
    sys.exit(main())
