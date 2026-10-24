"""Per-process scratch data for game integration tests (never a player's home)."""
from pathlib import Path
from contextlib import contextmanager
import os
import subprocess
import tempfile


def game_environment(base, root):
    root = Path(root)
    env = dict(base, HORIZON_LAUNCHER_NO_SELF_UPDATE='1', HORIZON_NO_AUDIO='1',
               HORIZON_TELEMETRY_URL='off')
    for key, folder in [('HORIZON_HOME', 'home'), ('HORIZON_LAUNCHER_HOME', 'launcher'),
                        ('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config')]:
        path = root / folder
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    return env


def run_game(command, **kwargs):
    """Run a headless game fixture with the same isolation as real-client tests."""
    with tempfile.TemporaryDirectory(prefix='hz-fixture-') as root:
        kwargs['env'] = game_environment(kwargs.pop('env', os.environ), root)
        if any(key in kwargs['env'] for key in
               ('HORIZON_COMMAND_SCRIPT', 'HORIZON_DISPENSE_FIXTURE', 'HORIZON_SCULK_FIXTURE')):
            # These compare seeded rule/callback traces, not the client/server boundary.
            command = [*command, '--in-frame']
        return subprocess.run(command, **kwargs)


@contextmanager
def suite_lock():
    """Real-client CI suites share a GPU, even when parity.py runs harnesses in parallel."""
    import fcntl
    with (Path(tempfile.gettempdir()) / 'horizon-integration-gpu.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield
