"""Measure the owned local workflow and process counters without UI input."""
import argparse
import ctypes
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser()
for name in ('tools', 'video', 'output'):
    parser.add_argument('--' + name, required=True)
parser.add_argument('--model-pid', type=int)
args = parser.parse_args()
workspace = Path(__file__).resolve().parents[1]
output = Path(args.output).resolve()
output.relative_to((workspace / '.tools').resolve())
if output.exists():
    raise RuntimeError('Use a new dedicated measurement directory.')
output.mkdir(parents=True)

class ProcessEntry(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('usage', w.DWORD), ('pid', w.DWORD), ('heap', ctypes.c_size_t), ('module', w.DWORD), ('threads', w.DWORD), ('parent', w.DWORD), ('priority', w.LONG), ('flags', w.DWORD), ('exe', w.WCHAR * 260)]

class MemoryCounters(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('faults', w.DWORD)] + [(name, ctypes.c_size_t) for name in ('peak_working', 'working', 'peak_paged', 'paged', 'peak_nonpaged', 'nonpaged', 'pagefile', 'peak_pagefile', 'private')]

kernel = ctypes.WinDLL('kernel32', use_last_error=True)
psapi = ctypes.WinDLL('psapi', use_last_error=True)
kernel.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
kernel.CreateToolhelp32Snapshot.restype = w.HANDLE
kernel.Process32FirstW.argtypes = [w.HANDLE, ctypes.POINTER(ProcessEntry)]
kernel.Process32NextW.argtypes = [w.HANDLE, ctypes.POINTER(ProcessEntry)]
kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
kernel.OpenProcess.restype = w.HANDLE
kernel.CloseHandle.argtypes = [w.HANDLE]
psapi.GetProcessMemoryInfo.argtypes = [w.HANDLE, ctypes.POINTER(MemoryCounters), w.DWORD]

def descendants(pid):
    snapshot = kernel.CreateToolhelp32Snapshot(2, 0)
    if snapshot == ctypes.c_void_p(-1).value:
        return [pid]
    parents = {}
    try:
        entry = ProcessEntry(); entry.size = ctypes.sizeof(entry)
        more = kernel.Process32FirstW(snapshot, ctypes.byref(entry))
        while more:
            parents[entry.pid] = entry.parent
            more = kernel.Process32NextW(snapshot, ctypes.byref(entry))
    finally:
        kernel.CloseHandle(snapshot)
    included = {pid}
    while True:
        grown = included | {child for child, parent in parents.items() if parent in included}
        if grown == included:
            return list(included)
        included = grown

def memory(pid):
    handle = kernel.OpenProcess(0x410, False, pid)
    if not handle:
        return None
    try:
        counters = MemoryCounters(); counters.size = ctypes.sizeof(counters)
        if not psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.size):
            return None
        return {'working': counters.working, 'private': counters.private}
    finally:
        kernel.CloseHandle(handle)

environment = os.environ.copy()
environment['PATH'] = environment['WINDIR'] + '\\System32;' + environment['WINDIR']
model_before = memory(args.model_pid) if args.model_pid else None
stdout = output / 'workflow.jsonl'; stderr = output / 'workflow-errors.txt'
command = [str(workspace / 'src-tauri/target/debug/examples/core_workflow_probe.exe'), str(output / 'store'), str(Path(args.tools).resolve()), str(Path(args.video).resolve())]
started = time.perf_counter(); samples = []; model_samples = []
with stdout.open('wb') as out, stderr.open('wb') as err:
    process = subprocess.Popen(command, env=environment, stdout=out, stderr=err, creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        while process.poll() is None:
            counters = [value for pid in descendants(process.pid) if (value := memory(pid)) is not None]
            if counters:
                samples.append({'elapsedMs': round((time.perf_counter() - started) * 1000), 'working': sum(value['working'] for value in counters), 'private': sum(value['private'] for value in counters)})
            if args.model_pid and (value := memory(args.model_pid)) is not None:
                model_samples.append(value)
            if time.perf_counter() - started > 85:
                raise TimeoutError('Bounded workflow measurement expired.')
            time.sleep(0.1)
    finally:
        if process.poll() is None:
            # The process is ours; the shared model service is never terminated.
            process.kill(); process.wait(timeout=5)
elapsed = round((time.perf_counter() - started) * 1000)
if process.returncode:
    raise RuntimeError(stderr.read_text(encoding='utf-8'))
records = [json.loads(line) for line in stdout.read_text(encoding='utf-8').splitlines() if line.startswith('{')]
result = {'workflow': records[-1], 'elapsedMsIncludingSetup': elapsed, 'sampleSleepMs': 100, 'samples': len(samples), 'maxSampledWorkingSetBytes': max(value['working'] for value in samples), 'maxSampledPrivateBytes': max(value['private'] for value in samples), 'modelBefore': model_before, 'modelMaxSampledWorkingSetBytes': max((value['working'] for value in model_samples), default=None), 'modelMaxSampledPrivateBytes': max((value['private'] for value in model_samples), default=None)}
(output / 'samples.json').write_text(json.dumps(samples), encoding='utf-8')
(output / 'verification.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps(result))
