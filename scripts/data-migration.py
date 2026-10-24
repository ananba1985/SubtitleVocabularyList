"""SQLite snapshot and path retargeting for an offline directory migration."""
import argparse
import json
from pathlib import Path
import sqlite3


def ident(name):
    return '"' + name.replace('"', '""') + '"'


def readonly(path):
    return sqlite3.connect(Path(path).resolve().as_uri() + '?mode=ro', uri=True)


def tables(connection):
    return [row[0] for row in connection.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['snapshot', 'retarget'])
    parser.add_argument('database')
    parser.add_argument('backup')
    parser.add_argument('--old-root')
    parser.add_argument('--new-root')
    args = parser.parse_args()
    backup = Path(args.backup)
    if args.action == 'snapshot':
        if backup.exists():
            raise RuntimeError('Snapshot already exists')
        backup.parent.mkdir(parents=True, exist_ok=True)
        with readonly(args.database) as source, sqlite3.connect(backup) as target:
            source.backup(target)
        print('SQLite snapshot saved')
        return

    old, new = args.old_root, args.new_root
    if not old or not new:
        raise RuntimeError('Both data roots are required')
    replacements = [(old.replace('\\', '\\\\'), new.replace('\\', '\\\\')),
                    (old.replace('\\', '/'), new.replace('\\', '/')), (old, new)]

    def retarget(value):
        if isinstance(value, str):
            for before, after in replacements:
                value = value.replace(before, after)
        return value

    # These fields hold managed absolute paths. User vocabulary/content is not edited.
    fields = {'sources': {'corpus_path', 'audio_path'}, 'tasks': {'snapshot_json'}}
    with readonly(backup) as source, sqlite3.connect(args.database) as target:
        names = tables(source)
        updated = {}
        with target:
            for table, columns in fields.items():
                if table not in names:
                    continue
                available = {row[1] for row in source.execute('PRAGMA table_info(' + ident(table) + ')')}
                for column in columns & available:
                    for before, after in replacements:
                        changed = target.execute(
                            'UPDATE ' + ident(table) + ' SET ' + ident(column) + '=replace(' + ident(column) + ',?,?) WHERE instr(' + ident(column) + ',?)>0',
                            (before, after, before)).rowcount
                        if changed:
                            updated[table + '.' + column] = updated.get(table + '.' + column, 0) + changed
        counts = {}
        for table in names:
            columns = [row[1] for row in source.execute('PRAGMA table_info(' + ident(table) + ')')]
            expected = [tuple(retarget(value) if columns[i] in fields.get(table, set()) else value
                              for i, value in enumerate(row))
                        for row in source.execute('SELECT * FROM ' + ident(table))]
            actual = target.execute('SELECT * FROM ' + ident(table)).fetchall()
            if expected != actual:
                raise RuntimeError('Migration comparison failed: ' + table)
            counts[table] = len(actual)
        if target.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
            raise RuntimeError('SQLite integrity check failed')
        target.execute('PRAGMA wal_checkpoint(TRUNCATE)')
        (backup.parent / 'migration-verified.json').write_text(json.dumps({
            'formatVersion': 1, 'verified': True, 'targetRoot': str(Path(new).resolve()),
            'tablesCompared': len(names), 'integrity': 'ok',
        }, ensure_ascii=False), encoding='utf-8')
        print(json.dumps({'tablesCompared': len(names), 'counts': counts, 'pathsUpdated': updated,
                          'integrity': 'ok'}, ensure_ascii=False))


if __name__ == '__main__':
    main()
