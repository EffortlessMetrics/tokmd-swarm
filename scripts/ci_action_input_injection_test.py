"""Regression tests for GitHub Action input interpolation safety.

GitHub Actions substitutes ``${{ ... }}`` expressions textually into a ``run:``
script body *before* the shell parses it. A composite action that interpolates
an ``inputs.*`` expression directly into a ``run:`` body therefore lets a value
containing a shell metacharacter break out of the intended quoting and execute
attacker-chosen commands on the runner.

Every ``inputs.*`` value must reach a script body through the step's ``env:``
block instead. These tests enforce that, and enforce that the binary-runtime
``version`` input is validated against a strict release-tag shape before it is
used to build a download URL.
"""

import re
import subprocess
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ACTION = ROOT / "action.yml"


def action_run_bodies(source):
    """Yield ``(line_number, script_text)`` for every ``run: |`` block."""
    lines = source.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        if re.match(r"^ *run: *\|", line):
            start = index + 1
            body = []
            index = start
            while index < len(lines):
                current = lines[index]
                if current.strip() and not current.startswith(
                    " " * (len(line) - len(line.lstrip()) + 2)
                ):
                    break
                body.append(current)
                index += 1
            yield start + 1, textwrap.dedent("\n".join(body))
            continue
        index += 1


def action_env_blocks(source):
    """Yield the text of every ``env:`` block in the action manifest."""
    lines = source.splitlines()
    for index, line in enumerate(lines):
        if not re.match(r"^ *env: *$", line):
            continue
        indent = len(line) - len(line.lstrip())
        block = []
        cursor = index + 1
        while cursor < len(lines):
            current = lines[cursor]
            if current.strip() and (len(current) - len(current.lstrip())) <= indent:
                break
            block.append(current)
            cursor += 1
        yield "\n".join(block)


class ActionInputInterpolationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = ACTION.read_text()

    def test_no_input_expression_is_interpolated_into_a_run_body(self):
        """``inputs.*`` must never appear inside a ``run:`` script body.

        A direct interpolation lets a value containing a quote or ``$(...)``
        escape the intended quoting and execute commands on the runner.
        """
        offenders = []
        for line_number, body in action_run_bodies(self.source):
            for offset, line in enumerate(body.splitlines()):
                if "${{" in line and "inputs." in line:
                    offenders.append(f"action.yml:{line_number + offset}: {line.strip()}")
        self.assertEqual(
            offenders,
            [],
            "inputs.* must be passed through env:, not interpolated into a run body:\n"
            + "\n".join(offenders),
        )

    def test_every_input_is_bound_to_an_environment_variable(self):
        """Each declared input must be reachable from a script body via ``env:``."""
        env_text = "\n".join(action_env_blocks(self.source))
        for name in ("version", "runtime", "image", "artifact", "comment"):
            self.assertIn(
                f"${{{{ inputs.{name} }}}}",
                env_text,
                f"input '{name}' must be bound through an env: block",
            )

    def test_binary_runtime_version_is_validated_against_a_release_tag_shape(self):
        """The binary runtime must reject any version that is not a release tag.

        The container runtime already gates ``version`` against an explicit
        allowlist; the binary runtime path (the default) must reject a value
        that could carry shell metacharacters into the download URL.
        """
        self.assertIn(
            r"grep -Eq '^v?[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'",
            self.source,
            "binary runtime must validate the version input against a strict release-tag shape",
        )


class ActionVersionValidationTests(unittest.TestCase):
    """Exercise the real validation block extracted from ``action.yml``."""

    @classmethod
    def setUpClass(cls):
        cls.source = ACTION.read_text()
        cls.block = cls._extract_binary_runtime_block(cls.source)

    @staticmethod
    def _extract_binary_runtime_block(source):
        lines = source.splitlines()
        start = next(
            index
            for index, line in enumerate(lines)
            if line.strip().startswith('version="${TOKMD_ACTION_VERSION-}"')
        )
        guard = next(
            index for index in range(start, len(lines)) if 'if [ -z "$version" ]' in lines[index]
        )
        indent = len(lines[guard]) - len(lines[guard].lstrip())
        end = next(
            index
            for index in range(guard + 1, len(lines))
            if lines[index].strip() == "fi"
            and (len(lines[index]) - len(lines[index].lstrip())) == indent
        )
        block = "\n".join(lines[start : end + 1])
        assert "grep -Eq" in block, "binary runtime version validation block not found"
        assert "requested_version" not in block, "extracted the container-runtime block"
        return block

    def _judge(self, version):
        """Return ``ACCEPTED`` or ``REJECTED`` for a candidate version value."""
        # Turn the failure branch into an observable marker so the block can be
        # sourced without terminating the harness.
        testable = self.block.replace("exit 1", "echo REJECTED >&2; exit 7")
        script = f"export TOKMD_ACTION_VERSION={version!r}\n{testable}\necho ACCEPTED\n"
        completed = subprocess.run(
            ["bash", "-c", script], capture_output=True, text=True, timeout=10
        )
        if completed.returncode == 7:
            return "REJECTED"
        if "ACCEPTED" in completed.stdout:
            return "ACCEPTED"
        self.fail(f"unexpected validation result: {completed.stderr.strip()}")

    def test_published_release_tags_are_accepted(self):
        for version in (
            "latest",
            "v1.15.0",
            "1.15.0",
            "v1.15.0-rc.3",
            "1.15.0-rc.3",
            "v1.15.0-alpha.1",
            "v2.0.0",
            "v0.0.1",
            "v10.20.30",
        ):
            with self.subTest(version=version):
                self.assertEqual(self._judge(version), "ACCEPTED")

    def test_shell_metacharacter_versions_are_rejected(self):
        for version in (
            '1.15.0"; echo INJECTED; #',
            '1.15.0"',
            "1.15.0'",
            "$(id)",
            "`id`",
            "1.15.0 && curl evil",
            "v1.15.0; rm -rf /",
            "v1.15.0;id",
            "1.15.0|cat",
            "1.15.0&",
            "1.15.0>out",
            "1.15.0 extra",
            "1.15.0/../x",
            "1.15.0 ${HOME}",
            "../../etc/passwd",
            " ",
            "1.15.0\nid",
            "v1.15.0\n",
        ):
            with self.subTest(version=version):
                self.assertEqual(self._judge(version), "REJECTED")

    def test_empty_version_falls_back_to_latest(self):
        """An omitted or empty version resolves to ``latest``.

        The pre-fix manifest turned an empty version into the malformed tag
        ``v``; falling back to ``latest`` keeps the download URL well-formed.
        """
        self.assertEqual(self._judge(""), "ACCEPTED")


if __name__ == "__main__":
    unittest.main()
