"""Locate an existing 7-Zip installation without changing the system PATH."""

import json
import os
from pathlib import Path
import shutil


def find_7zip():
    candidates = []
    command = shutil.which("7z")
    if command:
        candidates.append(Path(command))
    if os.name == "nt":
        import winreg

        for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
            for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
                try:
                    with winreg.OpenKey(hive, r"SOFTWARE\7-Zip", 0, winreg.KEY_READ | view) as key:
                        for name in ("Path64", "Path"):
                            try:
                                candidates.append(Path(winreg.QueryValueEx(key, name)[0]) / "7z.exe")
                            except OSError:
                                pass
                except OSError:
                    pass
    for name in ("ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"):
        if os.environ.get(name):
            candidates.append(Path(os.environ[name]) / "7-Zip/7z.exe")
    scoop_roots = [Path.home() / "scoop"]
    for name in ("SCOOP", "SCOOP_GLOBAL"):
        if os.environ.get(name):
            scoop_roots.append(Path(os.environ[name]))
    if os.environ.get("ProgramData"):
        scoop_roots.append(Path(os.environ["ProgramData"]) / "scoop")
    candidates.extend(root / "apps/7zip/current/7z.exe" for root in scoop_roots)
    return next((path.absolute() for path in candidates if path.is_file()), None)


if __name__ == "__main__":
    path = find_7zip()
    print(json.dumps({"path": str(path) if path else None}))
