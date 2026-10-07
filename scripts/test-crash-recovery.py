"""Terminate an owned native probe at the media-file/SQLite commit boundary."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time

parser = argparse.ArgumentParser()
for name in ('tools', 'corpus', 'video', 'output'):
    parser.add_argument('--' + name, required=True)
args = parser.parse_args()
workspace = Path(__file__).resolve().parents[1]
output = Path(args.output).resolve()
output.relative_to((workspace / '.tools').resolve())
if output.exists():
    raise RuntimeError('Use a new dedicated private output directory.')
probe = workspace / 'src-tauri' / 'target' / 'debug' / 'examples' / 'crash_recovery_probe.exe'
environment = os.environ.copy()
environment['PATH'] = os.path.join(environment['WINDIR'], 'System32') + ';' + environment['WINDIR']
base = [str(probe), '', str(output), str(Path(args.tools).resolve())]

def phase(name, extra=()):
    command = base.copy()
    command[1] = name
    result = subprocess.run(command + list(extra), env=environment, capture_output=True, text=True, encoding='utf-8', timeout=45, creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode:
        raise RuntimeError(f'Native {name} phase failed: {result.stderr}')
    return [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]

phase('prepare', [str(Path(args.corpus).resolve()), str(Path(args.video).resolve())])
metadata = json.loads((output / 'crash-fixture.json').read_text(encoding='utf-8'))
existing_files = set((output / 'media').glob('*.m4a'))
command = base.copy()
command[1] = 'run'
process = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8', creationflags=subprocess.CREATE_NO_WINDOW)
database = output / 'vocabulary.sqlite3'
connection = None
try:
    ready_line = process.stdout.readline()
    if not ready_line:
        raise RuntimeError('Native probe did not queue its task: ' + process.stderr.read())
    ready = json.loads(ready_line)
    if ready['pid'] != process.pid:
        raise RuntimeError('The task does not belong to the owned probe process.')
    connection = sqlite3.connect(database, timeout=5)
    connection.execute('BEGIN IMMEDIATE')
    (output / 'writer-locked').write_text('owned fault-injection writer', encoding='utf-8')
    deadline = time.monotonic() + 15
    orphan = None
    while time.monotonic() < deadline:
        added = set((output / 'media').glob('*.m4a')) - existing_files
        if added:
            orphan = next(iter(added))
            break
        if process.poll() is not None:
            raise RuntimeError('Probe exited before the intended commit boundary.')
        time.sleep(0.02)
    if orphan is None:
        raise RuntimeError('Media-file boundary was not observed.')
    relative = str(orphan.relative_to(output)).replace('\\', '/')
    if connection.execute('SELECT COUNT(*) FROM media_assets WHERE relative_path=?', (relative,)).fetchone()[0] != 0:
        raise RuntimeError('The observed file is already committed; do not mislabel this boundary.')
    queued = connection.execute('SELECT state FROM tasks WHERE id=?', (ready['taskId'],)).fetchone()[0]
    process.kill()  # Only the Popen handle created above; no user processes.
    process.wait(timeout=5)
    connection.rollback()
finally:
    if connection is not None:
        connection.close()
    if process.poll() is None:
        process.kill()
        process.wait(timeout=5)

with sqlite3.connect(database) as inspected:
    before = {table: inspected.execute('SELECT COUNT(*) FROM ' + table).fetchone()[0] for table in ('entries', 'media_assets', 'collection_actions', 'review_attempts')}
    assert before == {'entries': 1, 'media_assets': 1, 'collection_actions': 1, 'review_attempts': 1}, before
first = phase('recover')[-1]
retry = phase('recover')[-1]
assert first['taskId'] == retry['taskId'], 'Retry must reuse the successful receipt.'
assert first['result'] == retry['result']
with sqlite3.connect(database) as inspected:
    integrity = inspected.execute('PRAGMA integrity_check').fetchone()[0]
    relationships = inspected.execute('PRAGMA foreign_key_check').fetchall()
    after = {table: inspected.execute('SELECT COUNT(*) FROM ' + table).fetchone()[0] for table in before}
    referenced = {output / row[0] for row in inspected.execute('SELECT relative_path FROM media_assets')}
    assert after == {'entries': 2, 'media_assets': 2, 'collection_actions': 2, 'review_attempts': 1}, after
    assert integrity == 'ok' and not relationships
    assert referenced == set((output / 'media').glob('*.m4a')), 'No unreferenced output should remain after successful recovery.'
    baseline_path = output / inspected.execute('SELECT relative_path FROM media_assets WHERE id=?', (metadata['baselineAsset'],)).fetchone()[0]
    assert hashlib.sha256(baseline_path.read_bytes()).hexdigest() == metadata['baselineDigest']
report = {'killedPid': process.pid, 'killedTaskState': queued, 'fileExistedBeforeDatabaseCommit': True, 'beforeRecovery': before, 'afterRecovery': after, 'baselinePreserved': True, 'repeatedRecoveryTaskId': first['taskId'], 'repeatDidNotDuplicate': True, 'integrity': integrity, 'foreignKeyErrors': relationships, 'unreferencedMediaAfterRecovery': 0}
(output / 'verification.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps(report))
