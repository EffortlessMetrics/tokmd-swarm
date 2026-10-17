"""Fail-closed workflow_dispatch admission using native context and Git source refs.

This script never dispatches a run, changes a status, cancels work, or uses a
credential. GitHub supplies the event/context; git reads only public source refs.
The required Rust job repeats native-event and source validation before and after proof.
"""

import json
import os
import re
import subprocess
import sys
import time


SHA = re.compile(r"[0-9a-f]{40}\Z")
REPOSITORY = "EffortlessMetrics/tokmd-swarm"


def request_inputs(event, context):
    if context["event_name"] == "workflow_dispatch":
        return event.get("inputs", {}), None
    if context["event_name"] != "pull_request" or event.get("action") != "labeled" or event.get("label", {}).get("name") != "ci:qualify":
        raise ValueError("explicit qualification request required")
    pr = event.get("pull_request", {})
    if pr.get("state") != "open" or pr.get("base", {}).get("ref") != "main":
        raise ValueError("qualification requires an open PR targeting main")
    body = pr.get("body") or ""
    manifests = re.findall(r"<!-- tokmd-qualification:v1\s*\n(.*?)\n-->", body, re.DOTALL)
    if len(manifests) != 1:
        raise ValueError("one exact qualification request manifest is required")
    request = json.loads(manifests[0])
    if not isinstance(request, dict):
        raise ValueError("qualification request must be a JSON object")
    return request, pr


def admit(event, context, read_refs):
    if context["event_name"] == "push" and context["ref"] == "refs/heads/main":
        return {"mode": "main-push", "head": context["sha"], "base": "", "pr": ""}
    inputs, native_pr = request_inputs(event, context)
    if not isinstance(inputs, dict):
        raise ValueError("qualification inputs must be an object")
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
    if context["repository"] != REPOSITORY:
        raise ValueError("native run repository must equal the requested repository")
    if native_pr is None:
        if context["sha"] != head or context["workflow_sha"] != head:
            raise ValueError("native run and workflow source must both equal the requested head")
        proof_sha = None
    else:
        proof_sha = inputs.get("merge_sha", "")
        if not isinstance(proof_sha, str) or not SHA.fullmatch(proof_sha):
            raise ValueError("merge_sha must be the full exact composition commit")
        if (str(event.get("number")) != pr or native_pr.get("head", {}).get("sha") != head
                or native_pr.get("base", {}).get("sha") != base
                or context["sha"] != proof_sha or context["workflow_sha"] != proof_sha):
            raise ValueError("native PR source or merge composition differs from the request")
    decision = {"mode": "qualification" if native_pr is None else "pr-qualification", "head": head, "base": base, "pr": pr}
    if native_pr is not None:
        deadline = inputs.get("deadline_epoch", "")
        if not isinstance(deadline, str) or not deadline.isdecimal() or not 0 < int(deadline) - time.time() <= 20 * 60:
            raise ValueError("request deadline is missing, expired or exceeds 20 minutes")
        decision["deadline_epoch"] = deadline
    refs = read_refs(pr)
    if refs.get(f"refs/pull/{pr}/head") != head or refs.get("refs/heads/main") != base:
        raise ValueError("requested head or base is stale")
    if proof_sha is not None and refs.get(f"refs/pull/{pr}/merge") != proof_sha:
        raise ValueError("requested merge composition is stale")
    return decision


def live_refs(pr):
    result = subprocess.run(
        ["git", "ls-remote", f"https://github.com/{REPOSITORY}.git", f"refs/pull/{pr}/head", f"refs/pull/{pr}/merge", "refs/heads/main"],
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
    local_deadline = os.environ.get("QUALIFICATION_DEADLINE", "")
    deadline = decision.get("deadline_epoch", local_deadline)
    if local_deadline.isdecimal() and deadline.isdecimal():
        deadline = str(min(int(local_deadline), int(deadline)))
    if not deadline.isdecimal() or not 0 < int(deadline) - int(time.time()) <= 20 * 60:
        print("qualification refused: missing, expired or invalid shared work deadline", file=sys.stderr)
        return 1
    decision["deadline_epoch"] = deadline
    # Other existing PR workflows reuse this guard without another controller.
    # Revalidation must retain the first job-local monotonic anchor.
    if os.environ.get("GITHUB_ENV") and not os.environ.get("CI_BUDGET_MONO_NS"):
        with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as environment:
            print(f"QUALIFICATION_DEADLINE={deadline}", file=environment)
            print(f"CI_BUDGET_MONO_NS={time.monotonic_ns() + int((int(deadline) - time.time()) * 1e9)}", file=environment)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for key, value in decision.items():
            print(f"{key}={value}", file=output)
        print("admitted=true", file=output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
