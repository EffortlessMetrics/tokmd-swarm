"""Contract for the low-space local gate launcher."""

import os
import unittest
from unittest import mock

import gate


class GateLauncherTest(unittest.TestCase):
    def test_defaults_reach_the_locked_bootstrap_and_status_propagates(self):
        with mock.patch.dict(os.environ, {}, clear=True), mock.patch(
            "gate.subprocess.run"
        ) as run:
            run.return_value.returncode = 17
            self.assertEqual(gate.main(), 17)
        args, kwargs = run.call_args
        self.assertEqual(args[0], ["cargo", "--locked", "xtask", "gate", "--check"])
        self.assertEqual(kwargs["env"]["CARGO_PROFILE_DEV_DEBUG"], "0")
        self.assertEqual(kwargs["env"]["CARGO_INCREMENTAL"], "0")
        self.assertNotIn("CARGO_PROFILE_TEST_DEBUG", kwargs["env"])

    def test_explicit_settings_are_preserved(self):
        supplied = {
            "CARGO_PROFILE_DEV_DEBUG": "2",
            "CARGO_PROFILE_TEST_DEBUG": "1",
            "CARGO_INCREMENTAL": "1",
            "RUSTFLAGS": "-C debuginfo=2",
            "CARGO_ENCODED_RUSTFLAGS": "-C\x1fdebuginfo=1",
        }
        with mock.patch.dict(os.environ, supplied, clear=True), mock.patch(
            "gate.subprocess.run"
        ) as run:
            gate.main()
        self.assertEqual(run.call_args.kwargs["env"], supplied)


if __name__ == "__main__":
    unittest.main()
