"""Run the RIPR inventory profile against a two-commit corpus and a clean RIPR checkout."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

RIPR_REMOTE = "https://github.com/EffortlessMetrics/ripr-swarm.git"
ROOTS = ("crates/ripr/src", "editors/vscode/src", "xtask/src")
EXCLUSIONS = {
    "generated": ["**/generated/**", "**/*.generated.rs", "**/*.generated.ts"],
    "test": [
        "**/test/**", "**/tests/**", "**/*_test.rs", "**/*_tests.rs",
        "**/*.test.ts", "**/*.spec.ts",
    ],
    "fixture": ["**/fixture/**", "**/fixtures/**"],
    "vendored": ["**/vendor/**", "**/vendored/**", "**/node_modules/**"],
    "build-output": ["**/dist/**", "**/out/**", "**/target/**"],
}
METRICS = {"file.lines": "lines", "file.code": "lines", "module.lines": "lines", "module.files": "files"}
OPTIONS = ["--config", "none", "--no-ignore", "--no-progress", "--profile", "ripr_inventory_v1"]
CONFIG = b"""[view.ripr_inventory_v1]
format = "json"
module_roots = ["crates", "editors"]
module_depth = 2
children = "parents-only"
top = 0
min_code = 0
max_rows = 0
redact = "none"
meta = true
"""
MAIN = "crates/ripr/src/lib.rs"
OLD = "crates/ripr/src/old.rs"
RENAMED = "crates/ripr/src/renamed.rs"
REMOVED = "crates/ripr/src/removed.rs"
EXTRACTED = "crates/ripr/src/leaf.rs"
TS = "editors/vscode/src/extension.ts"
XTASK = "xtask/src/main.rs"
EMBEDDED = "xtask/src/example.md"
CONTROL = {
    "generated": "crates/ripr/src/generated/table.rs",
    "test": "crates/ripr/src/nested/tests/check.rs",
    "fixture": "crates/ripr/src/fixtures/example.rs",
    "vendored": "crates/ripr/src/vendor/example.rs",
}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def invoke(args, cwd, env):
    return subprocess.run(args, cwd=cwd, env=env, check=True, capture_output=True, timeout=60).stdout


def git(cwd, env, *args):
    return invoke(["git", "--no-optional-locks", *args], cwd, env).decode()


def require(condition, description):
    if not condition:
        raise RuntimeError(description)


def scan(binary, repo, env, mode, exclusions):
    args = [binary, *OPTIONS, mode]
    for pattern in exclusions:
        args += ["--exclude", pattern]
    if mode == "export":
        args += ["--strip-prefix", "."]
    result = json.loads(invoke(args + list(ROOTS), repo, env))
    require(
        result["schema_version"] == 2 and result["status"] == "complete" and not result["warnings"],
        f"incomplete or unsupported {mode} receipt",
    )
    require(
        result["args"]["children"] == "parents-only"
        and result["args"]["module_roots"] == ["crates", "editors"]
        and result["args"]["module_depth"] == 2
        and result["scan"]["excluded"] == exclusions,
        f"unexpected resolved profile in {mode} receipt",
    )
    del result["generated_at_ms"]
    return result


def checkout_identity(repo, env):
    require(not git(repo, env, "status", "--porcelain", "--untracked-files=all"), f"dirty checkout: {repo}")
    return {
        "head": git(repo, env, "rev-parse", "HEAD").strip(),
        "remote": git(repo, env, "remote", "get-url", "origin").strip(),
    }


def tracked_files(repo, env, head):
    tree = {}
    for entry in git(repo, env, "ls-tree", "-r", "-z", head, "--", *ROOTS).split("\0"):
        if entry:
            attrs, path = entry.split("\t", 1)
            mode, kind, oid = attrs.split()
            if kind == "blob" and mode in ("100644", "100755"):
                tree[path] = oid
    return tree


def check_tree_bytes(repo, tree, object_format):
    for path, expected in tree.items():
        target = repo / path
        require(target.is_file() and target.resolve() == target, f"missing or redirected HEAD file: {path}")
        data = target.read_bytes()
        oid = hashlib.new(object_format, f"blob {len(data)}\0".encode() + data).hexdigest()
        require(oid == expected, f"checkout bytes differ from HEAD: {path}")


def observe(binary, repo, env, exclusions):
    identity = checkout_identity(repo, env)
    tree = tracked_files(repo, env, identity["head"])
    object_format = git(repo, env, "rev-parse", "--show-object-format").strip()
    check_tree_bytes(repo, tree, object_format)
    exports = scan(binary, repo, env, "export", exclusions)
    modules = scan(binary, repo, env, "module", exclusions)
    require(exports == scan(binary, repo, env, "export", exclusions), "export reproduction differs")
    require(modules == scan(binary, repo, env, "module", exclusions), "module reproduction differs")
    require(identity == checkout_identity(repo, env), "checkout identity changed during scans")
    check_tree_bytes(repo, tree, object_format)

    files = {}
    for row in exports["rows"]:
        path = row["path"]
        require(row["kind"] == "parent", f"unexpected embedded contribution for {path}")
        require(path in tree and "\\" not in path and path not in files, f"invalid file identity: {path}")
        files[path] = {key: row[key] for key in ("module", "lines", "code")}
        files[path]["blob"] = tree[path]

    observed_modules = {
        row["module"]: {key: row[key] for key in ("files", "lines")}
        for row in modules["rows"]
    }
    require({row["module"] for row in exports["rows"]} == set(observed_modules), "module identities differ")
    for name, row in observed_modules.items():
        require(row["files"] == sum(file["module"] == name for file in files.values()),
                f"module file count differs from export: {name}")
        require(row["lines"] == sum(file["lines"] for file in files.values() if file["module"] == name),
                f"module line count differs from export: {name}")

    return {"repository": identity, "tokmd": exports["tool"], "files": files, "modules": observed_modules}


def write(root, path, content):
    target = root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(content.encode())


def corpus(binary, root, env, exclusions):
    root.mkdir()
    git(root, env, "init", "-q", "--template=", "--object-format=sha1")
    git(root, env, "remote", "add", "origin", "https://example.invalid/ripr-profile-corpus.git")
    for path in (OLD, REMOVED, XTASK, *CONTROL.values()):
        write(root, path, "fn example() {}\n")
    write(root, OLD, "fn rename_origin() {}\n")
    write(root, MAIN, "// comment\n\nfn retained() {}\nfn extracted() {}\n")
    write(root, TS, "export const extension = 1;\n")
    write(root, EMBEDDED, "# Example\n\n```rust\nfn embedded() {}\n```\n")
    git(root, env, "add", ".")
    git(root, env, "commit", "-qm", "base")
    base = observe(binary, root, env, exclusions)
    require(set(base["files"]) == {MAIN, OLD, REMOVED, TS, XTASK, EMBEDDED}, "unexpected base selection")
    for category, path in CONTROL.items():
        other_exclusions = [
            pattern for key, rules in EXCLUSIONS.items() if key != category for pattern in rules
        ]
        control = scan(binary, root, env, "export", other_exclusions)
        require({row["path"] for row in control["rows"]} == set(base["files"]) | {path},
                f"{category} control did not change only its own case")

    git(root, env, "mv", OLD, RENAMED)
    git(root, env, "rm", "-q", REMOVED)
    write(root, MAIN, "// comment\n\nfn retained() {}\n")
    write(root, EXTRACTED, "fn extracted() {}\n")
    git(root, env, "add", ".")
    git(root, env, "commit", "-qm", "head")
    head = observe(binary, root, env, exclusions)
    diff = git(root, env, "diff", "--name-status", "--find-renames=100%", "HEAD^", "HEAD")
    require(set(head["files"]) == {MAIN, RENAMED, EXTRACTED, TS, XTASK, EMBEDDED},
            "unexpected head selection")
    require(f"R100\t{OLD}\t{RENAMED}" in diff.splitlines(), "missing exact rename")
    require(f"D\t{REMOVED}" in diff.splitlines(), "missing deletion")
    require(base["files"][OLD]["blob"] == head["files"][RENAMED]["blob"], "rename lost blob identity")
    require((base["files"][MAIN]["lines"], base["files"][MAIN]["code"]) == (4, 2), "base line units")
    require((head["files"][MAIN]["lines"], head["files"][EXTRACTED]["lines"]) == (3, 1),
            "extraction did not move one line")
    require({head["files"][p]["module"] for p in (MAIN, EXTRACTED)} == {"crates/ripr"},
            "split lost the module identity")
    require(head["modules"]["crates/ripr"]["lines"] == base["modules"]["crates/ripr"]["lines"] - 1,
            "module delta should reflect the deletion")
    require(head["files"][TS]["module"] == "editors/vscode", "incorrect editor module identity")
    git(root, env, "update-index", "--skip-worktree", TS)
    (root / TS).unlink()
    require(checkout_identity(root, env) == head["repository"], "missing-file control must appear clean")
    try:
        observe(binary, root, env, exclusions)
    except RuntimeError as error:
        require(str(error) == f"missing or redirected HEAD file: {TS}", "unexpected missing-file failure")
    else:
        raise RuntimeError("accepted a missing tracked file")
    return {"base": base, "head": head, "changes": diff.splitlines(), "category_controls": CONTROL}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tokmd", type=Path)
    parser.add_argument("ripr", type=Path)
    parser.add_argument("ripr_head")
    args = parser.parse_args()
    binary, ripr = args.tokmd.resolve(strict=True), args.ripr.resolve(strict=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith(("GIT_", "TOKMD_"))}
    with tempfile.TemporaryDirectory() as temp:
        tmp = Path(temp)
        config = tmp / "tokmd.toml"
        config.write_bytes(CONFIG)
        env.update({
            "TOKMD_CONFIG": str(config), "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_AUTHOR_NAME": "Profile Corpus", "GIT_AUTHOR_EMAIL": "corpus@example.invalid",
            "GIT_COMMITTER_NAME": "Profile Corpus", "GIT_COMMITTER_EMAIL": "corpus@example.invalid",
            "GIT_AUTHOR_DATE": "2000-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2000-01-01T00:00:00Z",
        })
        expected = {"head": args.ripr_head, "remote": RIPR_REMOTE}
        require(checkout_identity(ripr, env) == expected, "RIPR checkout does not match the requested origin and HEAD")
        tool_digest = digest(binary.read_bytes())
        exclusions = [pattern for rules in EXCLUSIONS.values() for pattern in rules]
        sample = corpus(binary, tmp / "corpus", env, exclusions)
        live = observe(binary, ripr, env, exclusions)
        require(live["repository"] == expected and live["files"], "missing or mismatched RIPR result")
        require(digest(binary.read_bytes()) == tool_digest, "tokmd executable changed during the proof")
        print(canonical({
            "profile": "ripr_inventory_v1",
            "metric_definition_version": 1,
            "metric_units": METRICS,
            "profile_sha256": digest(canonical([CONFIG.decode(), ROOTS, OPTIONS])),
            "exclusions_sha256": digest(canonical(list(EXCLUSIONS.items()))),
            "tokmd_binary_sha256": tool_digest,
            "corpus": sample,
            "ripr": live,
        }).decode())


if __name__ == "__main__":
    main()
