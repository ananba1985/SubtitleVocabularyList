"""One real libmpv queue/EOF/speed normal path, using null video/audio outputs."""
import ctypes as c
import json
from pathlib import Path
import sys
import time


class Event(c.Structure):
    _fields_ = [("id", c.c_int), ("error", c.c_int), ("userdata", c.c_uint64), ("data", c.c_void_p)]


class Hook(c.Structure):
    _fields_ = [("name", c.c_char_p), ("id", c.c_uint64)]


def main():
    dll, root = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    saved = json.loads((root / "persistence.json").read_text(encoding="utf-8"))
    mpv = c.CDLL(str(dll))

    def api(name, result, *args):
        f = getattr(mpv, name)
        f.restype, f.argtypes = result, args
        return f

    create = api("mpv_create", c.c_void_p)
    option = api("mpv_set_option_string", c.c_int, c.c_void_p, c.c_char_p, c.c_char_p)
    initialize = api("mpv_initialize", c.c_int, c.c_void_p)
    command = api("mpv_command", c.c_int, c.c_void_p, c.POINTER(c.c_char_p))
    get = api("mpv_get_property_string", c.c_void_p, c.c_void_p, c.c_char_p)
    free = api("mpv_free", None, c.c_void_p)
    wait = api("mpv_wait_event", c.POINTER(Event), c.c_void_p, c.c_double)
    hook_add = api("mpv_hook_add", c.c_int, c.c_void_p, c.c_uint64, c.c_char_p, c.c_int)
    proceed = api("mpv_hook_continue", c.c_int, c.c_void_p, c.c_uint64)
    destroy = api("mpv_terminate_destroy", None, c.c_void_p)
    handle = create()
    assert handle

    def run(*args):
        values = [str(x).encode("utf-8") for x in args]
        result = command(handle, (c.c_char_p * (len(values) + 1))(*values, None))
        assert result >= 0, (args, result)

    def prop(name):
        value = get(handle, name.encode())
        if not value:
            return ""
        try:
            return c.string_at(value).decode("utf-8")
        finally:
            free(value)

    loaded, hooks, events = [], [], []

    def until(predicate):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            event = wait(handle, 0.05).contents
            if event.id == 25:
                hook = c.cast(event.data, c.POINTER(Hook)).contents
                path = prop("path")
                assert Path(path).is_file(), path
                hooks.append(path)
                assert proceed(handle, hook.id) >= 0
            elif event.id == 8:
                loaded.append(prop("path"))
            elif event.id == 7:
                events.append("end-file")
            if predicate():
                return
        raise AssertionError("Timed out waiting for normal playback state")

    try:
        for key, value in {"vo": "null", "ao": "null", "audio": "no", "terminal": "no", "config": "no", "load-scripts": "no", "idle": "yes", "keep-open": "yes", "pause": "yes"}.items():
            assert option(handle, key.encode(), value.encode()) >= 0
        assert initialize(handle) >= 0
        assert hook_add(handle, 0, b"on_load", 0) >= 0
        for item in saved["playlist"]:
            run("loadfile", item["path"], "append")
        # The same insertion-point adjustment used by the queue's up/down controls.
        run("playlist-move", 0, 2)
        run("playlist-move", 1, 0)
        assert int(prop("playlist-count")) == 2
        run("set", "speed", saved["preferences"]["speed"])
        run("set", "volume", saved["preferences"]["volume"])
        run("set", "mute", "yes")
        run("playlist-play-index", 0)
        until(lambda: len(loaded) == 1)
        assert prop("pause") == "yes"
        assert abs(float(prop("speed")) - 1.25) < .001
        run("set", "keep-open", "always")
        assert prop("keep-open") == "always"
        run("set", "keep-open", "yes")
        run("seek", max(0, float(prop("duration")) - .15), "absolute+exact")
        run("set", "pause", "no")
        until(lambda: len(loaded) == 2)
        assert loaded == [i["path"] for i in saved["playlist"]], loaded
        assert hooks == loaded
        assert int(prop("playlist-pos")) == 1
        assert abs(float(prop("speed")) - 1.25) < .001
        run("set", "pause", "yes")
        run("playlist-prev", "weak")
        until(lambda: len(loaded) == 3)
        run("playlist-remove", 1)
        assert int(prop("playlist-count")) == 1
        result = {"loaded": loaded, "hooks": hooks, "events": events, "speed": prop("speed"), "volume": prop("volume"), "muted": prop("mute"), "autoNext": True, "headless": True, "soundOutput": False}
        (root / "mpv.json").write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
        print("PASS: real libmpv queue reorder/remove/previous, on_load hooks, EOF auto-next and saved speed/volume/mute; no window or sound.")
    finally:
        destroy(handle)


if __name__ == "__main__":
    main()
