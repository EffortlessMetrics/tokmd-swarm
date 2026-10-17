"""Fail-closed workflow_dispatch admission using native context and Git source refs.

This script never dispatches a run, changes a status, cancels work, or uses a
credential. GitHub supplies the event/context; git reads only public source refs.
The required Rust job separately checks live PR metadata with its existing token.
"""

import json
import os
import re
import subprocess
import sys
import time


SHA = re.compile(r"[0-9a-f]{40}\Z")
REPOSITORY = "EffortlessMetrics/tokmd-swarm"


def admit(event, context, read_refs):
    if context["event_name"] == "push" and context["ref"] == "refs/heads/main":
        return {"mode": "main-push", "head": context["sha"], "base": "", "pr": ""}
    if context["event_name"] != "workflow_dispatch":
        raise ValueError("explicit qualification request required")
    inputs = event.get("inputs", {})
    pr = inputs.get("pull_request", "")
    head, base = inputs.get("head_sha", ""), inputs.get("base_sha", "")
    if not isinstance(pr, str) or not re.fullmatch(r"[1-9][0-9]*", pr):
        raise ValueError("pull_request must be a positive integer")
    if not isinstance(head, str) or not isinstance(base, str) or not SHA.fullmatch(head) or not SHA.fullmatch(base):
        raise ValueError("head_sha and base_sha must be full lowercase commit SHAs")
    if inputs.get("generation") != f"{head}:{base}":
        raise ValueError("generation must equal head_sha:base_sha")
    if inputs.get("expected_green") != "true":
        raise ValueError("request must explicitly assert expected-green local source and review")
    if context["repository"] != REPOSITORY or context["sha"] != head or context["workflow_sha"] != head:
        raise ValueError("native run and workflow source must both equal the requested head")
    refs = read_refs(pr)
    if refs.get(f"refs/pull/{pr}/head") != head or refs.get("refs/heads/main") != base:
        raise ValueError("requested head or base is stale")
    return {"mode": "qualification", "head": head, "base": base, "pr": pr}


def live_refs(pr):
    result = subprocess.run(
        ["git", "ls-remote", f"https://github.com/{REPOSITORY}.git", f"refs/pull/{pr}/head", "refs/heads/main"],
        check=True, text=True, capture_output=True, timeout=20,
    )
    return {ref: sha for sha, ref in (line.split() for line in result.stdout.splitlines())}


def main():
    try:
        with open(os.environ["GITHUB_EVENT_PATH"], encoding="utf-8") as stream:
            event = json.load(stream)
        context = {key: os.environ[env] for key, env in (
            ("event_name", "GITHUB_EVENT_NAME"), ("ref", "GITHUB_REF"),
            ("sha", "GITHUB_SHA"), ("workflow_sha", "GITHUB_WORKFLOW_SHA"),
            ("repository", "GITHUB_REPOSITORY"),
        )}
        decision = admit(event, context, live_refs)
    except (KeyError, ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"qualification refused: {error}", file=sys.stderr)
        return 1
    # One deadline includes admission, routing, checkout/cache and all core commands.
    # Reserve one minute for TERM/KILL, receipts and native teardown. This does not
    # establish a platform queue bound or successful completion under 20 minutes.
    if decision["mode"] == "qualification":
        print("qualification refused: dispatch lacks the native PR context required by the current risk/proof lanes; migration is not qualified", file=sys.stderr)
        return 1
    deadline = os.environ.get("QUALIFICATION_DEADLINE", "")
    if not deadline.isdecimal() or not 0 < int(deadline) - int(time.time()) <= 19 * 60:
        print("qualification refused: missing, expired or invalid shared work deadline", file=sys.stderr)
        return 1
    decision["deadline_epoch"] = deadline
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in decision.items():
            print(f"{key}={value}", file=output)
        print("admitted=true", file=output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
