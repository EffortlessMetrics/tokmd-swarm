"""Qualify and explicitly publish the exact swarm OCI archive; never rebuild on publish.

The automatic path needs no registry credential. Publication is dispatch-only,
uses a fixed destination, and stops after any uncertain write. Registry tags do
not offer compare-and-swap; the workflow serializes its own publishers only.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
import zipfile

REPOSITORY = "EffortlessMetrics/tokmd-swarm"
IMAGE = "ghcr.io/effortlessmetrics/tokmd-swarm"
QUALIFICATION_WORKFLOW = ".github/workflows/swarm-ghcr.yml"
SHA = re.compile(r"[0-9a-f]{40}\Z")
DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
MAX_ARCHIVE = 1024 * 1024 * 1024
MAX_JSON = 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return "sha256:" + digest.hexdigest()


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def command(args, *, timeout=120, stdout=subprocess.PIPE):
    result = subprocess.run(args, stdout=stdout, stderr=subprocess.PIPE, timeout=timeout, check=False)
    # Do not include a subprocess's diagnostics in a public receipt: registry
    # tools may include authentication or remote response details.
    require(result.returncode == 0, f"{Path(args[0]).name} failed (exit {result.returncode})")
    return result.stdout


def git(*args):
    env = os.environ.copy()
    for key in list(env):
        if key.startswith("GIT_"):
            del env[key]
    result = subprocess.run(["git", *args], env=env, capture_output=True, timeout=30, check=True)
    return result.stdout.decode("utf-8").strip()


def source_identity():
    return git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")


def context():
    return {key: os.environ.get("GITHUB_" + key.upper(), "") for key in
            ("repository", "ref", "sha", "workflow_sha", "event_name", "run_id", "run_attempt")}


def validate_context(ctx, operation):
    require(ctx["repository"] == REPOSITORY, "wrong repository")
    require(SHA.fullmatch(ctx["sha"]) is not None, "missing exact native source")
    require(ctx["workflow_sha"] == ctx["sha"], "workflow and source identity differ")
    require(re.fullmatch(r"[1-9][0-9]*", ctx["run_id"]) is not None, "invalid run ID")
    require(re.fullmatch(r"[1-9][0-9]*", ctx["run_attempt"]) is not None, "invalid run attempt")
    if operation == "publish":
        require(ctx["event_name"] == "workflow_dispatch" and ctx["ref"] == "refs/heads/main",
                "publication requires deliberate dispatch from main")
    else:
        require((ctx["event_name"] == "push" and ctx["ref"] == "refs/heads/main")
                or ctx["event_name"] == "workflow_dispatch", "unsupported qualification event")


def artifact_name(ctx):
    return f"swarm-image-{ctx['sha']}-{ctx['run_id']}-{ctx['run_attempt']}"


def inspect_oci(path):
    """Verify bytes without extracting or executing archive-supplied content."""
    require(0 < Path(path).stat().st_size <= MAX_ARCHIVE, "archive size outside limit")
    bodies = {}
    hashes = {}
    sizes = {}
    total = 0
    with tarfile.open(path, "r:") as archive:
        names = set()
        for count, member in enumerate(archive, 1):
            require(count <= 4096, "too many OCI members")
            name = member.name.removeprefix("./").rstrip("/")
            require(name not in names, "duplicate OCI member")
            names.add(name)
            if member.isdir():
                require(name in {"blobs", "blobs/sha256"}, "unexpected OCI directory")
                continue
            require(member.isfile(), "OCI links and special files are forbidden")
            require(name in {"oci-layout", "index.json"} or
                    re.fullmatch(r"blobs/sha256/[0-9a-f]{64}", name), "unexpected OCI member")
            total += member.size
            require(total <= MAX_ARCHIVE, "expanded OCI archive exceeds limit")
            digest = hashlib.sha256()
            body = bytearray()
            stream = archive.extractfile(member)
            require(stream is not None, "missing OCI member")
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
                if member.size <= MAX_JSON:
                    body.extend(block)
            hashes[name] = "sha256:" + digest.hexdigest()
            sizes[name] = member.size
            if member.size <= MAX_JSON:
                bodies[name] = bytes(body)
            if name.startswith("blobs/"):
                require(hashes[name] == "sha256:" + name.rsplit("/", 1)[1], "corrupt OCI blob")
    require(json.loads(bodies["oci-layout"]) == {"imageLayoutVersion": "1.0.0"}, "wrong OCI layout")
    index = json.loads(bodies["index.json"])
    require(index.get("schemaVersion") == 2 and len(index.get("manifests", [])) == 1,
            "exactly one OCI image is required")
    referenced = set()

    def descriptor(value, parse=False):
        digest = value.get("digest", "")
        require(DIGEST.fullmatch(digest) is not None, "invalid OCI digest")
        name = "blobs/sha256/" + digest.split(":")[1]
        referenced.add(name)
        require(hashes.get(name) == digest and sizes.get(name) == value.get("size"), "missing or wrong-size OCI blob")
        return json.loads(bodies[name]) if parse else None

    selected = index["manifests"][0]
    manifest = descriptor(selected, True)
    require(manifest.get("schemaVersion") == 2 and manifest.get("mediaType") in {
        "application/vnd.oci.image.manifest.v1+json", "application/vnd.docker.distribution.manifest.v2+json"},
        "unsupported image manifest")
    config = descriptor(manifest["config"], True)
    require(config.get("os") == "linux" and config.get("architecture") == "amd64", "unsupported image platform")
    for layer in manifest["layers"]:
        descriptor(layer)
    require(set(hashes) == referenced | {"oci-layout", "index.json"}, "unreferenced OCI content")
    return {"manifest_digest": selected["digest"], "config_digest": manifest["config"]["digest"],
            "labels": config.get("config", {}).get("Labels", {}), "platform": "linux/amd64"}


def qualify(directory, ctx):
    validate_context(ctx, "qualify")
    head, tree = source_identity()
    require(head == ctx["sha"], "checkout differs from native source")
    directory = Path(directory)
    archive = directory / "image.tar"
    identity = inspect_oci(archive)
    require(identity["labels"].get("org.opencontainers.image.revision") == head, "image revision label differs")
    require(identity["labels"].get("org.opencontainers.image.source") == f"https://github.com/{REPOSITORY}",
            "wrong image source label")
    version = tomllib.loads(Path("Cargo.toml").read_text())["workspace"]["package"]["version"]
    tag = "tokmd-swarm-qualification:" + head
    smoke = []
    try:
        command(["skopeo", "copy", "oci-archive:" + str(archive), "docker-daemon:" + tag])
        actual_id = command(["docker", "image", "inspect", "--format", "{{.Id}}", tag]).decode().strip()
        require(actual_id == identity["config_digest"], "loaded image does not match archive config")
        for argument in ("--version", "--help"):
            output = command(["docker", "run", "--rm", "--network", "none", actual_id, argument], timeout=30)
            if argument == "--version":
                require(output.decode().strip() == f"tokmd {version}", "packaged binary version differs")
            else:
                require(b"Usage:" in output and b"tokmd" in output, "packaged help is unavailable")
            smoke.append({"argument": argument, "exit_code": 0, "stdout_sha256": sha256(output)})
    finally:
        command(["docker", "image", "rm", tag], timeout=30)
    receipt = {"schema": "tokmd.swarm_image_qualification.v1", "repository": REPOSITORY,
               "source_sha": head, "source_tree": tree, "workflow": QUALIFICATION_WORKFLOW,
               "run_id": ctx["run_id"], "run_attempt": ctx["run_attempt"], "event": ctx["event_name"],
               "ref": ctx["ref"], "artifact_name": artifact_name(ctx), "version": version,
               "archive_sha256": file_digest(archive), "archive_bytes": archive.stat().st_size,
               "manifest_digest": identity["manifest_digest"], "config_digest": identity["config_digest"],
               "platform": identity["platform"], "smoke": smoke, "registry_write": False, "result": "qualified"}
    write_json(directory / "qualification.json", receipt)
    return receipt


def api(path):
    return json.loads(command(["gh", "api", "--method", "GET", f"repos/{REPOSITORY}/{path}"]))


def validate_request(request, ctx):
    validate_context(ctx, "publish")
    require(isinstance(request, dict), "publication request must be an object")
    request = {"expected_main_digest": "", **request}
    require(set(request) == {"source_sha", "source_tree", "qualification_run_id", "artifact_id",
                             "archive_sha256", "manifest_digest", "move_main", "expected_main_digest"},
            "publication request fields differ from the supported contract")
    for key in ("source_sha", "source_tree"):
        require(SHA.fullmatch(request.get(key, "")) is not None, f"invalid {key}")
    require(request["source_sha"] == ctx["sha"], "request differs from dispatch source")
    for key in ("qualification_run_id", "artifact_id"):
        require(re.fullmatch(r"[1-9][0-9]*", request.get(key, "")) is not None, f"invalid {key}")
    for key in ("archive_sha256", "manifest_digest"):
        require(DIGEST.fullmatch(request.get(key, "")) is not None, f"invalid {key}")
    require(request.get("move_main") in {"true", "false"}, "explicit main movement policy required")
    previous = request.get("expected_main_digest", "")
    require((request["move_main"] == "false" and previous == "") or
            (request["move_main"] == "true" and (previous == "absent" or DIGEST.fullmatch(previous))),
            "main movement requires its exact prior digest or explicit absence")
    return request


def validate_provenance(request, run, artifact):
    require(run.get("repository", {}).get("full_name") == REPOSITORY, "wrong qualification repository")
    require(str(run.get("id")) == request["qualification_run_id"], "wrong qualification run")
    require(run.get("path") == QUALIFICATION_WORKFLOW and run.get("event") in {"push", "workflow_dispatch"} and
            run.get("head_branch") == "main", "qualification must run on protected main")
    require(run.get("status") == "completed" and run.get("conclusion") == "success", "qualification is not successful")
    require(run.get("head_sha") == request["source_sha"], "stale qualification source")
    name = artifact_name({"sha": request["source_sha"], "run_id": request["qualification_run_id"],
                          "run_attempt": str(run.get("run_attempt"))})
    require(str(artifact.get("id")) == request["artifact_id"] and artifact.get("name") == name,
            "artifact identity differs")
    require(artifact.get("expired") is False and 0 < artifact.get("size_in_bytes", 0) <= MAX_ARCHIVE,
            "artifact expired or oversized")
    require(DIGEST.fullmatch(artifact.get("digest", "")) is not None, "artifact lacks archive digest")
    owner = artifact.get("workflow_run", {})
    require(str(owner.get("id")) == request["qualification_run_id"] and
            owner.get("head_sha") == request["source_sha"], "artifact belongs to another subject")
    return name


def unpack_artifact(zip_path, directory, digest):
    require(file_digest(zip_path) == digest, "Actions ZIP digest mismatch")
    with zipfile.ZipFile(zip_path) as archive:
        files = archive.infolist()
        require(len(files) == 2 and {entry.filename for entry in files} == {"image.tar", "qualification.json"},
                "unexpected or duplicate artifact members")
        for entry in files:
            require(not entry.is_dir() and (entry.external_attr >> 16) & 0o170000 not in {0o120000},
                    "artifact links are forbidden")
            maximum = MAX_JSON if entry.filename == "qualification.json" else MAX_ARCHIVE
            require(0 < entry.file_size <= maximum, "artifact member exceeds bound")
            with archive.open(entry) as source, (Path(directory) / entry.filename).open("xb") as output:
                for block in iter(lambda: source.read(1024 * 1024), b""):
                    output.write(block)


def validate_receipt(request, run, receipt, identity, archive):
    require(receipt.get("schema") == "tokmd.swarm_image_qualification.v1" and
            receipt.get("result") == "qualified" and receipt.get("registry_write") is False,
            "missing successful nonpublishing qualification")
    expected = {"repository": REPOSITORY, "source_sha": request["source_sha"], "source_tree": request["source_tree"],
                "workflow": QUALIFICATION_WORKFLOW, "run_id": request["qualification_run_id"],
                "run_attempt": str(run["run_attempt"]), "event": run["event"], "ref": "refs/heads/main",
                "artifact_name": artifact_name({"sha": request["source_sha"], "run_id": request["qualification_run_id"],
                                                "run_attempt": str(run["run_attempt"])}),
                "manifest_digest": request["manifest_digest"], "archive_sha256": request["archive_sha256"],
                "config_digest": identity["config_digest"], "platform": "linux/amd64"}
    require(all(receipt.get(key) == value for key, value in expected.items()), "qualification receipt identity mismatch")
    require(identity["manifest_digest"] == request["manifest_digest"] and
            file_digest(archive) == request["archive_sha256"] and
            Path(archive).stat().st_size == receipt.get("archive_bytes"), "qualified OCI bytes differ")
    require(identity["labels"].get("org.opencontainers.image.revision") == request["source_sha"] and
            identity["labels"].get("org.opencontainers.image.source") == f"https://github.com/{REPOSITORY}",
            "qualified image source differs")
    smoke = receipt.get("smoke", [])
    require(len(smoke) == 2 and [item.get("argument") for item in smoke] == ["--version", "--help"] and
            all(item.get("exit_code") == 0 and DIGEST.fullmatch(item.get("stdout_sha256", "")) for item in smoke),
            "missing packaged image smoke")


def current_source(request):
    main = api("branches/main")["commit"]
    require(main["sha"] == request["source_sha"] and main["commit"]["tree"]["sha"] == request["source_tree"],
            "main moved; requalify the intended source")


def registry_manifest(tag, token):
    request = urllib.request.Request(f"https://{IMAGE}/manifests/{tag}".replace("ghcr.io/", "ghcr.io/v2/", 1),
        headers={"Authorization": "Bearer " + token, "Accept":
                 "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json, "
                 "application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(MAX_JSON + 1)
            require(len(data) <= MAX_JSON, "registry manifest exceeds bound")
            digest = sha256(data)
            require(response.headers.get("Docker-Content-Digest") == digest, "registry digest header mismatch")
            return digest
    except urllib.error.HTTPError as error:
        data = error.read(MAX_JSON + 1)
        if error.code == 404 and len(data) <= MAX_JSON:
            payload = json.loads(data)
            # GHCR also uses MANIFEST_UNKNOWN for existing indexes when media
            # negotiation fails. Unknown messages are unavailable, never absence.
            if payload == {"errors": [{"code": "MANIFEST_UNKNOWN", "message": "manifest unknown"}]}:
                return None
        raise ValueError(f"registry lookup unavailable (HTTP {error.code}); absence is unproved") from None


def registry_token():
    # ghcr's token service is the sole recipient, and tokens never enter receipts.
    import base64
    credential = (os.environ["GITHUB_ACTOR"] + ":" + os.environ["GH_TOKEN"]).encode()
    request = urllib.request.Request("https://ghcr.io/token?service=ghcr.io&scope=repository:effortlessmetrics/tokmd-swarm:pull,push",
        headers={"Authorization": "Basic " + base64.b64encode(credential).decode()})
    with urllib.request.urlopen(request, timeout=30) as response:
        data = response.read(MAX_JSON + 1)
    require(len(data) <= MAX_JSON, "oversized registry token response")
    token = json.loads(data).get("token")
    require(isinstance(token, str) and token, "registry token unavailable")
    return token


def publish_verified(request, archive, receipt_path, lookup, copy, source_check):
    """The invoked write controller; injectable effects keep failure tests offline."""
    immutable = "sha-" + request["source_sha"]
    receipt = {"schema": "tokmd.swarm_image_publication.v1", "request": request,
               "image": IMAGE, "immutable_tag": immutable, "state": "not_started", "steps": []}

    def record(state):
        receipt["state"] = state
        receipt["steps"].append({"state": state, "time_epoch": int(time.time())})
        write_json(receipt_path, receipt)
        print("swarm image publication state: " + state, flush=True)

    def check_main():
        if request["move_main"] == "true":
            expected = None if request["expected_main_digest"] == "absent" else request["expected_main_digest"]
            require(lookup("main") == expected, "main tag changed; explicit new authorization required")

    record("validating_registry")
    try:
        source_check()
        require(lookup(immutable) is None, "immutable tag already exists; inspect prior request, never overwrite")
        check_main()
        # Repeat source and absence immediately before the first write.
        source_check()
        require(lookup(immutable) is None, "immutable tag appeared; refusing conflicting request")
        record("immutable_write_attempted")
        copy(immutable, archive)
        require(lookup(immutable) == request["manifest_digest"], "immutable digest readback differs")
        record("immutable_verified")
        if request["move_main"] == "true":
            source_check()
            check_main()
            record("main_write_attempted")
            copy("main", archive)
            require(lookup("main") == request["manifest_digest"], "main digest readback differs")
            record("main_verified")
        record("complete")
    except BaseException:
        # Keep attempted writes distinguishable from refusal or verified partial
        # publication. Cancellation/kill may leave only the preceding checkpoint.
        receipt["terminal"] = "failed_or_interrupted"
        write_json(receipt_path, receipt)
        raise
    return receipt


def publish_candidate(directory, request, ctx):
    request = validate_request(request, ctx)
    checkpoint = Path(directory) / "publication.json"
    def phase(name):
        write_json(checkpoint, {"schema": "tokmd.swarm_image_publication.v1", "state": name,
                               "request": request, "registry_write_attempted": False})
    phase("validating_source")
    require(source_identity() == (request["source_sha"], request["source_tree"]), "publisher checkout differs")
    current_source(request)
    phase("validating_provenance")
    run = api("actions/runs/" + request["qualification_run_id"])
    artifact = api("actions/artifacts/" + request["artifact_id"])
    validate_provenance(request, run, artifact)
    phase("validating_artifact")
    with tempfile.TemporaryDirectory(prefix="swarm-image-") as temporary:
        root = Path(temporary)
        zipped = root / "artifact.zip"
        with zipped.open("xb") as output:
            command(["gh", "api", "--method", "GET", f"repos/{REPOSITORY}/actions/artifacts/{request['artifact_id']}/zip"], stdout=output)
        unpack_artifact(zipped, root, artifact["digest"])
        archive = root / "image.tar"
        identity = inspect_oci(archive)
        receipt = json.loads((root / "qualification.json").read_text())
        validate_receipt(request, run, receipt, identity, archive)
        phase("authenticating_registry")
        token = registry_token()
        authfile = root / "auth.json"
        credential = subprocess.run(["skopeo", "login", "--authfile", str(authfile), "--username",
                                    os.environ["GITHUB_ACTOR"], "--password-stdin", "ghcr.io"],
                                   input=os.environ["GH_TOKEN"].encode(), capture_output=True, timeout=30, check=False)
        require(credential.returncode == 0, "registry login failed")
        def copy(tag, path):
            command(["skopeo", "copy", "--preserve-digests", "--retry-times", "0", "--authfile", str(authfile),
                     "oci-archive:" + str(path), "docker://" + IMAGE + ":" + tag], timeout=300)
        return publish_verified(request, archive, Path(directory) / "publication.json",
                                lambda tag: registry_manifest(tag, token), copy, lambda: current_source(request))


def publish(directory, request, ctx):
    checkpoint = Path(directory) / "publication.json"
    write_json(checkpoint, {"schema": "tokmd.swarm_image_publication.v1",
                           "state": "validating_request", "registry_write_attempted": False})
    try:
        return publish_candidate(directory, request, ctx)
    except BaseException:
        receipt = json.loads(checkpoint.read_text())
        receipt["terminal"] = "failed_or_interrupted"
        # Preserve any write-attempt checkpoint created by publish_verified.
        write_json(checkpoint, receipt)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("qualify", "publish"))
    parser.add_argument("--directory", required=True)
    args = parser.parse_args()
    if args.operation == "qualify":
        qualify(args.directory, context())
    else:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        publish(args.directory, event.get("inputs", {}), context())


if __name__ == "__main__":
    main()
