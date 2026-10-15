"""One normal release-resource flow; does not build or run an installer."""
import hashlib
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    root = Path(__file__).resolve().parents[1]
    staged = root / ".tools/release-dictionaries"
    manifest = json.loads((staged / "manifest.json").read_text(encoding="utf-8"))
    expected = {item["path"]: item["sha256"] for item in manifest["files"]}
    expected["manifest.json"] = digest(staged / "manifest.json")
    config = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    prefix = "../.tools/release-dictionaries/"
    mapped = {source.removeprefix(prefix): destination for source, destination in config["bundle"]["resources"].items()
              if source.startswith(prefix)}
    assert set(mapped) == set(expected)
    assert all(destination == "Data/dictionaries/" + source for source, destination in mapped.items())
    shell = shutil.which("pwsh") or shutil.which("powershell")
    assert shell, "PowerShell is required for the standalone resource step."
    with tempfile.TemporaryDirectory(prefix="svl-dictionaries-") as temporary:
        install = Path(temporary) / "SubtitleVocabularyList"

        def deploy():
            subprocess.run([shell, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                            str(root / "scripts/copy-release-dictionaries.ps1"),
                            "-InstallDirectory", str(install)], check=True)
            for name, sha256 in expected.items():
                assert digest(install / mapped[name]) == sha256, name

        deploy()  # First installation: Data/dictionaries is created with both dictionaries.
        personal = install / "Data/vocabulary.sqlite3"
        connection = sqlite3.connect(personal)
        connection.executescript("CREATE TABLE entries(word TEXT); INSERT INTO entries VALUES ('reluctant');"
                                 "CREATE TABLE learning_records(word TEXT,score INTEGER);"
                                 "INSERT INTO learning_records VALUES ('reluctant',3);")
        connection.close()
        personal_digest = digest(personal)
        # A prior dictionary revision is upgraded; personal records remain a separate file.
        for name in ("base.sqlite3", "ecdict.sqlite3"):
            connection = sqlite3.connect(install / "Data/dictionaries" / name)
            connection.execute("UPDATE metadata SET value='previous-release' WHERE key='source_commit'")
            connection.commit()
            connection.close()
        deploy()
        assert digest(personal) == personal_digest
        connection = sqlite3.connect(personal)
        assert connection.execute("SELECT word FROM entries").fetchall() == [("reluctant",)]
        assert connection.execute("SELECT word,score FROM learning_records").fetchall() == [("reluctant", 3)]
        connection.close()
    print(json.dumps({"firstInstallResources": "passed", "upgradeResources": "passed",
                      "personalDatabase": "unchanged", "packagedFiles": len(expected),
                      "dictionaryProfiles": manifest["profile"], "installerBuilt": False},
                     ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
