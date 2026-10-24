"""Automated game runs must never inherit a real player's data directories."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

from tools.integration_env import game_environment, run_game


class IntegrationEnvironmentTests(unittest.TestCase):
    def test_all_data_homes_and_network_settings_are_overridden(self):
        keys = ('HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME')
        with tempfile.TemporaryDirectory() as root:
            env = game_environment({**dict.fromkeys(keys, '/real/player'), 'DISPLAY': ':123'}, root)
            homes = [Path(env[key]) for key in keys]
            self.assertEqual(len(set(homes)), 4)
            self.assertTrue(all(p.is_dir() and p.is_relative_to(root) for p in homes))
            self.assertEqual(env['DISPLAY'], ':123')
            self.assertEqual(env['HORIZON_TELEMETRY_URL'], 'off')
            self.assertEqual(env['HORIZON_LAUNCHER_NO_SELF_UPDATE'], '1')
            self.assertEqual(env['HORIZON_NO_AUDIO'], '1')

    def test_each_fixture_process_gets_fresh_homes_and_cleanup(self):
        script = ('import json, os; from pathlib import Path; '
                  'homes = [os.environ[k] for k in '
                  '("HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME")]; '
                  'assert all(Path(p).is_dir() for p in homes); print(json.dumps(homes))')
        runs = [json.loads(run_game([sys.executable, '-c', script], env=os.environ,
                                    capture_output=True, text=True, check=True).stdout) for _ in range(2)]
        self.assertFalse(set(runs[0]) & set(runs[1]))
        self.assertTrue(all(not Path(p).exists() for homes in runs for p in homes))
