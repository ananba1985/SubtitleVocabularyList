"""Prepare the pinned player for the manually requested Windows installer."""

import argparse
import ctypes
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fetch(path, url, expected):
    if not path.is_file():
        path.parent.mkdir(parents=True, exist_ok=True)
        partial = path.with_suffix(path.suffix + ".partial")
        try:
            print(f"Downloading {path.name}...", flush=True)
            request = urllib.request.Request(url, headers={"User-Agent": "SubtitleVocabularyList-packaging"})
            with urllib.request.urlopen(request, timeout=120) as response, partial.open("wb") as output:
                shutil.copyfileobj(response, output)
            if digest(partial) != expected:
                raise RuntimeError(f"Checksum mismatch: {path.name}")
            partial.replace(path)
        finally:
            partial.unlink(missing_ok=True)
    if digest(path) != expected:
        raise RuntimeError(f"Checksum mismatch: {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--resources-directory", default=".tools/release-resources")
    args = parser.parse_args()
    config_path = ROOT / "scripts/mpv-runtime.json"
    config = json.loads(config_path.read_text(encoding="utf-8-sig"))
    runtime = ROOT / ".tools/runtime/mpv"
    archive = runtime.parent / config["archive"]
    fetch(archive, config["archiveUrl"], config["archiveSha256"])
    library = runtime / config["library"]
    if not library.is_file() or digest(library) != config["librarySha256"]:
        runtime.mkdir(parents=True, exist_ok=True)
        subprocess.run(["7z", "x", "-y", f"-o{runtime}", str(archive)], check=True, stdout=subprocess.DEVNULL)
    if digest(library) != config["librarySha256"]:
        raise RuntimeError("libmpv DLL checksum mismatch")
    with library.open("rb") as stream:
        if stream.read(2) != b"MZ":
            raise RuntimeError("Invalid libmpv PE file")
        stream.seek(0x3C)
        offset = struct.unpack("<I", stream.read(4))[0]
        stream.seek(offset)
        if stream.read(6) != b"PE\0\0\x64\x86":
            raise RuntimeError("libmpv must target Windows x86_64")
    # Loads the DLL and its dependencies without initializing playback or a window.
    api = ctypes.CDLL(str(library))
    api.mpv_client_api_version.restype = ctypes.c_ulong
    api_version = api.mpv_client_api_version()
    if api_version >> 16 != 2:
        raise RuntimeError(f"Unsupported libmpv client API: {api_version}")
    cache = ROOT / ".tools/release-cache/mpv"
    for source in config["sourceArchives"]:
        fetch(cache / source["file"], source["url"], source["sha256"])
    target = (ROOT / args.resources_directory).resolve() / "mpv"
    target.mkdir(parents=True, exist_ok=True)
    shutil.copy2(library, target / config["library"])
    shutil.copy2(config_path, target / "mpv-runtime.json")
    shutil.copy2(ROOT / "scripts/mpv-runtime-notice.md", target / "NOTICE.md")
    with zipfile.ZipFile(cache / config["sourceArchives"][0]["file"]) as source:
        for name in ("Copyright", "LICENSE.GPL", "LICENSE.LGPL"):
            matches = [entry for entry in source.namelist() if entry.endswith("/" + name)]
            if len(matches) != 1:
                raise RuntimeError(f"Missing mpv license: {name}")
            (target / name).write_bytes(source.read(matches[0]))
    print(f"Player resource ready: {target}; client API {api_version >> 16}.{api_version & 0xffff}", flush=True)


if __name__ == "__main__":
    main()
