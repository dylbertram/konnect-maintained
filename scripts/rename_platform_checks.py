#!/usr/bin/env python3
"""Reviewed-source Windows/macOS rename checks; no installs or SMB setup.

Each invocation has one 3000s deadline, commands have at most 1200s, and failed
commands are never retried. Evidence and targets must live outside the checkout.
UNC runs only after a successful core invocation in the same GitHub Actions job.
All subprocess output is retained. This runner does not claim native KiCad
coverage from Rust tests, including tests that skip unavailable native tools.
"""

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import threading
import time
import traceback

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
START = time.monotonic()
DEADLINE = START + 3000
RESERVE = 500_000_000
HEADROOM = 256 * 1024 * 1024
MIN_BUDGET = 2 * 1024**3
MAX_BUDGET = 4 * 1024**3
PINS = "crates/konnect-core/tests/project_rename_source_pins.json"
PINS_SHA256 = "17d9aaef7926f70d424103e409bbbd2f4d878b0bdc20657926044a0f34c23104"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def inside(path, parent):
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


class WindowsJob:
    """Create before Popen; attach the suspended child before any child code."""

    def __init__(self, budget):
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.psapi = ctypes.WinDLL("psapi", use_last_error=True)
        k = self.kernel
        k.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
        k.CreateJobObjectW.restype = wintypes.HANDLE
        k.SetInformationJobObject.argtypes = [
            wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD
        ]
        k.QueryInformationJobObject.argtypes = [
            wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p,
            wintypes.DWORD, ctypes.c_void_p
        ]
        k.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        k.TerminateJobObject.argtypes = [wintypes.HANDLE, wintypes.UINT]
        k.CloseHandle.argtypes = [wintypes.HANDLE]
        k.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        k.OpenProcess.restype = wintypes.HANDLE
        k.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        self.psapi.GetProcessMemoryInfo.argtypes = [
            wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD
        ]
        self.handle = k.CreateJobObjectW(None, None)
        require(self.handle, f"CreateJobObjectW: {ctypes.get_last_error()}")

        class BasicLimits(ctypes.Structure):
            _fields_ = [
                ("process_time", ctypes.c_int64), ("job_time", ctypes.c_int64),
                ("flags", wintypes.DWORD), ("min_ws", ctypes.c_size_t),
                ("max_ws", ctypes.c_size_t), ("active_limit", wintypes.DWORD),
                ("affinity", ctypes.c_size_t), ("priority", wintypes.DWORD),
                ("scheduling", wintypes.DWORD),
            ]

        class IoCounters(ctypes.Structure):
            _fields_ = [(name, ctypes.c_uint64) for name in (
                "reads", "writes", "other", "read_bytes", "write_bytes", "other_bytes"
            )]

        class ExtendedLimits(ctypes.Structure):
            _fields_ = [
                ("basic", BasicLimits), ("io", IoCounters),
                ("process_memory", ctypes.c_size_t), ("job_memory", ctypes.c_size_t),
                ("peak_process_memory", ctypes.c_size_t), ("peak_job_memory", ctypes.c_size_t),
            ]

        self.limit_type = ExtendedLimits
        limits = ExtendedLimits()
        limits.basic.flags = 0x2000 | 0x200  # KILL_ON_JOB_CLOSE | JOB_MEMORY
        limits.job_memory = budget
        if not k.SetInformationJobObject(self.handle, 9, ctypes.byref(limits),
                                         ctypes.sizeof(limits)):
            self.close()
            raise ctypes.WinError(ctypes.get_last_error())

    def attach(self, process):
        handle = wintypes.HANDLE(int(process._handle))
        require(self.kernel.AssignProcessToJobObject(self.handle, handle),
                f"AssignProcessToJobObject: {ctypes.get_last_error()}")
        resume = ctypes.WinDLL("ntdll").NtResumeProcess
        resume.argtypes = [wintypes.HANDLE]
        resume.restype = ctypes.c_long
        require(resume(handle) == 0, "NtResumeProcess failed")

    def census(self):
        # JOBOBJECT_BASIC_PROCESS_ID_LIST uses ULONG counts followed by ULONG_PTRs.
        buffer = ctypes.create_string_buffer(65536)
        require(self.kernel.QueryInformationJobObject(
            self.handle, 3, buffer, len(buffer), None
        ), f"job process census failed: {ctypes.get_last_error()}")
        assigned, listed = struct.unpack_from("II", buffer.raw)
        require(assigned == listed and listed <= 4096, "job census truncated/oversize")
        pointer = "Q" if ctypes.sizeof(ctypes.c_size_t) == 8 else "I"
        pids = list(struct.unpack_from(pointer * listed, buffer.raw, 8)) if listed else []

        class MemoryCounters(ctypes.Structure):
            _fields_ = [
                ("size", wintypes.DWORD), ("faults", wintypes.DWORD),
            ] + [(name, ctypes.c_size_t) for name in (
                "peak_ws", "ws", "peak_paged", "paged", "peak_nonpaged",
                "nonpaged", "pagefile", "peak_pagefile"
            )]

        rss = 0
        for pid in pids:
            handle = self.kernel.OpenProcess(0x410, False, pid)
            if not handle:
                # A process may exit between the job snapshot and OpenProcess.
                require(ctypes.get_last_error() == 87,
                        f"cannot inspect owned PID {pid}: {ctypes.get_last_error()}")
                continue
            try:
                counters = MemoryCounters()
                counters.size = ctypes.sizeof(counters)
                if self.psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.size):
                    rss += counters.ws
                else:
                    exit_code = wintypes.DWORD()
                    require(self.kernel.GetExitCodeProcess(handle, ctypes.byref(exit_code))
                            and exit_code.value != 259,
                            f"cannot read live owned PID {pid} RSS")
            finally:
                self.kernel.CloseHandle(handle)
        limits = self.limit_type()
        require(self.kernel.QueryInformationJobObject(
            self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits), None
        ), "cannot read peak job memory")
        return pids, rss, limits.peak_job_memory

    def kill(self):
        require(self.kernel.TerminateJobObject(self.handle, 125),
                f"TerminateJobObject: {ctypes.get_last_error()}")

    def close(self):
        if self.handle:
            require(self.kernel.CloseHandle(self.handle), "CloseHandle(job) failed")
            self.handle = None


class Runner:
    def __init__(self, evidence, report):
        self.out = evidence
        self.report = report
        self.index = 0
        self.budget = MAX_BUDGET
        self.disks = [ROOT, evidence]
        self.env = os.environ.copy()
        self.env.update({
            "CARGO_BUILD_JOBS": "2", "RUST_TEST_THREADS": "2",
            "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0",
            "CARGO_INCREMENTAL": "0", "CARGO_TERM_COLOR": "never",
            "RUST_BACKTRACE": "1", "PYTHONDONTWRITEBYTECODE": "1",
        })
        self.report["commands"] = []
        self.report["probes"] = []
        self.known = {}

    def probe(self, command):
        """Bounded read-only macOS OS probe, in its own group, logged separately."""
        require(time.monotonic() < DEADLINE, "invocation deadline before OS probe")
        record = {"command": command, "timeout_seconds": 3, "owned_cleanup": False}
        self.report["probes"].append(record)
        process = subprocess.Popen(
            command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            stdin=subprocess.DEVNULL, start_new_session=True,
        )
        record["pid"] = process.pid
        try:
            stdout, stderr = process.communicate(timeout=min(3, max(0.01, DEADLINE - time.monotonic())))
            record.update(returncode=process.returncode,
                          stdout=stdout.decode("utf-8", "replace"),
                          stderr=stderr.decode("utf-8", "replace"))
            require(process.returncode == 0, f"OS probe failed: {command}")
            record["status"] = "passed"
            return stdout.decode("utf-8", "strict")
        except subprocess.TimeoutExpired as error:
            record.update(status="failed", timed_out=True,
                          stdout=(error.stdout or b"").decode("utf-8", "replace"),
                          stderr=(error.stderr or b"").decode("utf-8", "replace"))
            raise
        except BaseException as error:
            record.update(status="failed", error=repr(error))
            raise
        finally:
            # A successful leader must not leave helper descendants either.
            try:
                os.killpg(process.pid, signal.SIGKILL)
                record["unexpected_group_after_exit"] = process.poll() is not None
            except ProcessLookupError:
                record["unexpected_group_after_exit"] = False
            if process.poll() is None:
                process.wait(timeout=3)
                record["timed_out"] = True
            try:
                os.killpg(process.pid, 0)
            except ProcessLookupError:
                record["owned_cleanup"] = True
            if not record["owned_cleanup"] or record["unexpected_group_after_exit"]:
                record["status"] = "failed"
            with (self.out / "os-probes.jsonl").open("a", encoding="utf-8") as log:
                log.write(json.dumps(record, sort_keys=True) + "\n")
            if record.get("status") != "passed":
                tail = record.get("stdout", "") + record.get("stderr", "")
                print(tail[-12000:], file=sys.stderr)
            # Keep full census output on disk, not in the long-lived report.
            record.pop("stdout", None)
            record.pop("stderr", None)
            require(record["owned_cleanup"], f"OS probe group still present: {process.pid}")
            require(not record["unexpected_group_after_exit"],
                    f"OS probe left descendants: {command}")

    def memory(self):
        if sys.platform == "win32":
            class MemoryStatus(ctypes.Structure):
                _fields_ = [("length", wintypes.DWORD), ("load", wintypes.DWORD)] + [
                    (name, ctypes.c_uint64) for name in (
                        "total", "available", "page_total", "page_available",
                        "virtual_total", "virtual_available", "extended"
                    )
                ]
            state = MemoryStatus()
            state.length = ctypes.sizeof(state)
            require(ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(state)),
                    "GlobalMemoryStatusEx failed")
            return {"total_bytes": state.total, "available_bytes": state.available,
                    "method": "GlobalMemoryStatusEx"}
        text = self.probe(["/usr/bin/vm_stat"])
        page = re.search(r"page size of (\d+) bytes", text)
        require(page is not None, "cannot parse vm_stat page size")
        counts = {}
        for label in ("free", "inactive", "speculative"):
            found = re.search(rf"^Pages {label}:\s+(\d+)\.", text, re.MULTILINE)
            require(found is not None, f"cannot parse vm_stat Pages {label}")
            counts[label] = int(found.group(1))
        return {"total_bytes": self.report["host"]["total_bytes"],
                "available_bytes": sum(counts.values()) * int(page.group(1)),
                "method": "vm_stat free+inactive+speculative", "pages": counts}

    def capacity(self, initial=False):
        memory = self.memory()
        disks = {str(path): shutil.disk_usage(path).free for path in self.disks}
        record = dict(memory, disk_free_bytes=disks, cpu_count=os.cpu_count(),
                      elapsed_seconds=round(time.monotonic() - START, 3))
        if initial:
            self.budget = min(MAX_BUDGET, memory["available_bytes"] - RESERVE - HEADROOM)
            record["planned_tree_budget_bytes"] = self.budget
            self.report["capacity_before_build"] = record
            require(self.budget >= MIN_BUDGET, "less than 2 GiB safe tree budget")
        require(memory["available_bytes"] >= RESERVE, "available RAM below 500000000 bytes")
        require(all(free >= (6_000_000_000 if initial else RESERVE) for free in disks.values()),
                "insufficient disk reserve")
        return record

    def mac_census(self, pgid):
        text = self.probe(["/bin/ps", "-axo", "pid=,ppid=,pgid=,rss=,lstart="])
        rows = []
        for line in text.splitlines():
            if line.strip():
                fields = line.split(maxsplit=4)
                require(len(fields) == 5, "incomplete macOS process census")
                rows.append((*map(int, fields[:4]), fields[4]))
        selected = {pid for pid, _, group, _, _ in rows if group == pgid}
        # Keep tracking observed descendants even if they change their group.
        # A start-time match avoids treating a recycled PID as an owned child.
        selected.update(pid for pid, _, _, _, birth in rows if self.known.get(pid) == birth)
        while True:
            added = {pid for pid, parent, _, _, _ in rows if parent in selected}
            if added <= selected:
                break
            selected.update(added)
        self.known.update({pid: birth for pid, _, _, _, birth in rows if pid in selected})
        escaped = [pid for pid, _, group, _, _ in rows if pid in selected and group != pgid]
        rss = sum(kib * 1024 for pid, _, _, kib, _ in rows if pid in selected)
        return sorted(selected), rss, escaped

    def run(self, label, command, timeout=1200):
        require(0 < timeout <= 1200, "invalid command deadline")
        require(time.monotonic() < DEADLINE, "invocation deadline exhausted")
        self.capacity(initial=command[0] == "cargo")
        self.index += 1
        path = self.out / f"{self.index:02}-{label}.log"
        audit_path = path.with_suffix(".audit.json")
        audit = {
            "label": label, "command": command, "log": str(path),
            "timeout_seconds": min(timeout, max(0, DEADLINE - time.monotonic())),
            "status": "incomplete", "owned_cleanup": False, "samples": [],
            "tree_budget_bytes": self.budget, "autoretry": False,
            "timed_out": False, "signaled": False,
        }
        self.report["commands"].append(audit)
        process = None
        job = None
        watchdog = None
        failure = None
        self.known = {}
        # Leave time for owned cleanup and the mandatory final source audit.
        invocation_end = DEADLINE - (5 if label.endswith("-after") else 60)
        end = min(invocation_end, time.monotonic() + timeout)
        require(end > time.monotonic(), "no time remains for command plus final audit")
        audit["timeout_seconds"] = min(audit["timeout_seconds"], end - time.monotonic())
        try:
            if sys.platform == "win32":
                job = WindowsJob(self.budget)
            with path.open("wb") as log:
                options = ({"creationflags": 0x4 | 0x200} if job else
                           {"start_new_session": True})
                process = subprocess.Popen(
                    command, cwd=ROOT, env=self.env, stdin=subprocess.DEVNULL,
                    stdout=log, stderr=subprocess.STDOUT, **options
                )
                audit["pid"] = process.pid

                def expire():
                    # An independent timer enforces the deadline even if an
                    # OS resource probe stalls. It kills only this owned tree.
                    audit["timed_out"] = True
                    try:
                        if job:
                            job.kill()
                        else:
                            os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    except BaseException as error:
                        audit["watchdog_error"] = repr(error)

                watchdog = threading.Timer(max(0, end - time.monotonic()), expire)
                watchdog.daemon = True
                watchdog.start()
                if job:
                    job.attach(process)
                next_sample = time.monotonic()
                while True:
                    code = process.poll()
                    if job:
                        pids, rss, peak_commit = job.census()
                        escaped = []
                    else:
                        pids, rss, escaped = self.mac_census(process.pid)
                        peak_commit = None
                    capacity = self.capacity()
                    audit["samples"].append(dict(
                        capacity, pids=pids, rss_bytes=rss,
                        peak_job_commit_bytes=peak_commit, escaped_pids=escaped,
                    ))
                    require(not escaped, "owned descendants escaped their process group")
                    require(rss <= self.budget, "owned tree RSS exceeded budget")
                    if code is None:
                        code = process.poll()
                        if code is not None:
                            # Do not reject a stale snapshot of the leader that
                            # exited while the OS/capacity probes were running.
                            if job:
                                pids = job.census()[0]
                            else:
                                pids = self.mac_census(process.pid)[0]
                    if code is not None:
                        audit["returncode"] = code
                        audit["signaled"] = code < 0 or (sys.platform == "win32" and code >= 0x80000000)
                        require(not pids, f"command exited with remaining descendants: {pids}")
                        watchdog.cancel()
                        watchdog.join()
                        require(not audit["timed_out"], f"{label} deadline exceeded")
                        require(code == 0, f"{label} failed with status {code}")
                        audit["owned_cleanup"] = True
                        audit["status"] = "passed"
                        break
                    if time.monotonic() >= end:
                        audit["timed_out"] = True
                        raise RuntimeError(f"{label} deadline exceeded")
                    next_sample += 1
                    time.sleep(max(0, min(next_sample, end) - time.monotonic()))
        except BaseException as error:
            failure = error
            audit.update(status="failed", error=repr(error))
            raise
        finally:
            if watchdog:
                watchdog.cancel()
                watchdog.join()
            try:
                if process is not None and not audit["owned_cleanup"]:
                    if job:
                        job.kill()
                    else:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                        # Only PIDs observed in this owned tree, never global kill.
                        owned_now = self.mac_census(process.pid)[0]
                        for pid in owned_now:
                            try:
                                os.kill(pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                    if process.poll() is None:
                        # Also handles a suspended child if Job assignment failed.
                        process.kill()
                    process.wait(timeout=5)
                    cleanup_end = time.monotonic() + 5
                    while True:
                        pids = job.census()[0] if job else self.mac_census(process.pid)[0]
                        audit["remaining_pids"] = pids
                        if not pids:
                            audit["owned_cleanup"] = True
                            break
                        require(time.monotonic() < cleanup_end, "owned descendants survived cleanup")
                        time.sleep(0.1)
                if process is None:
                    audit["owned_cleanup"] = True
            except BaseException as error:
                audit.update(status="failed", cleanup_error=repr(error))
                if failure is None:
                    raise
            finally:
                if job:
                    try:
                        job.close()
                        audit["job_closed"] = True
                    except BaseException as error:
                        audit.update(status="failed", job_close_error=repr(error))
                audit["elapsed_seconds"] = round(time.monotonic() - START, 3)
                save(audit_path, audit)
                save(self.out / "report.json", self.report)
                if audit["status"] != "passed" and path.exists():
                    with path.open("rb") as log:
                        log.seek(max(0, path.stat().st_size - 12000))
                        print(log.read().decode("utf-8", "replace"), file=sys.stderr)
        require(audit["status"] == "passed" and audit["owned_cleanup"],
                f"{label} did not complete with verified owned cleanup")
        return path

    def text(self, label, command):
        return self.run(label, command, timeout=30).read_text(encoding="utf-8").strip()


def source_snapshot(runner, suffix, required_sha):
    head = runner.text(f"head-{suffix}", ["git", "rev-parse", "HEAD"])
    require(head == required_sha, f"HEAD differs from required tested SHA: {head}")
    status = runner.text(f"status-{suffix}", [
        "git", "status", "--porcelain=v1", "--untracked-files=all",
    ])
    require(not status, f"source is not clean, including untracked files: {status}")
    others = runner.text(f"untracked-{suffix}", [
        "git", "ls-files", "--others", "--directory", "-z",
    ]).split("\0")
    unexpected = [
        name for name in others
        if name and not {".git", "target"}.intersection(Path(name).parts)
    ]
    require(not unexpected, f"untracked/ignored entries outside excluded output: {unexpected}")
    names = runner.text(f"tracked-{suffix}", ["git", "ls-files", "-z"]).split("\0")
    manifest = {}
    for name in names:
        if not name:
            continue
        parts = Path(name).parts
        if ".git" in parts or "target" in parts:
            continue
        path = ROOT / name
        require(not inside(path.resolve(), runner.out), f"evidence overlaps tracked file: {name}")
        require(not path.is_symlink() and path.is_file(), f"nonregular tracked source: {name}")
        manifest[name] = digest(path)
    pins_path = ROOT / PINS
    require(digest(pins_path) == PINS_SHA256, "frozen 15-file source pin manifest changed")
    pins = json.loads(pins_path.read_text(encoding="utf-8"))
    require(len(pins) == 15, "must verify exactly 15 protected source files")
    for name, expected in pins.items():
        require(manifest.get(name) == expected, f"protected source pin mismatch: {name}")
    snapshot = {"head": head, "tracked_sha256": manifest,
                "pins_manifest_sha256": digest(pins_path), "protected_files_verified": 15}
    save(runner.out / f"source-{suffix}.json", snapshot)
    return snapshot


def test_counts(path, expected=None, prefix=None):
    text = path.read_text(encoding="utf-8", errors="replace")
    summaries = [tuple(map(int, row)) for row in re.findall(
        r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; "
        r"(\d+) measured; (\d+) filtered out;", text
    )]
    require(bool(summaries), f"missing successful test summaries: {path.name}")
    require(not re.search(r"test result: FAILED|^error:|^failures:", text, re.MULTILINE),
            f"failure output in {path.name}")
    require(all(failed == 0 and measured == 0 for _, failed, _, measured, _ in summaries),
            f"failed or measured tests in {path.name}")
    names = []
    pending = None
    for line in text.splitlines():
        start = re.match(r"^test ([^\s]+) \.\.\. (.*)$", line)
        if start:
            require(pending is None, f"overlapping/unaccounted focused output: {path.name}")
            if start.group(2).strip() == "ok":
                names.append(start.group(1))
            elif not start.group(2).startswith("ignored"):
                pending = start.group(1)
        elif line.strip() == "ok" and pending is not None:
            # --nocapture prints the test's output between its name and "ok".
            names.append(pending)
            pending = None
    require(pending is None, f"test has no observed ok result: {path.name}")
    passed = sum(row[0] for row in summaries)
    require(passed > 0 and len(names) == passed, f"zero tests/unaccounted passes: {path.name}")
    if expected is not None:
        require(passed == expected and len(set(names)) == expected,
                f"expected exactly {expected} unique successful tests in {path.name}")
        require(all(row[2] == 0 for row in summaries), f"ignored focused tests: {path.name}")
    if prefix:
        require(all(name.startswith(prefix) for name in names), "unexpected focused test identity")
    return {"passed": passed, "failed": 0, "ignored": sum(row[2] for row in summaries),
            "summaries": summaries, "passed_test_names": names}


def job_identity():
    keys = ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB", "RUNNER_NAME")
    result = {key: os.environ.get(key) for key in keys}
    require(all(result.values()), "same-job cache requires GitHub Actions job identity")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--mode", choices=("core", "unc"), required=True)
    args = parser.parse_args()
    require(re.fullmatch(r"[0-9a-fA-F]{40}", args.source_sha), "--source-sha requires full SHA")
    require(args.evidence.is_absolute(), "--evidence must be absolute")
    out = args.evidence.resolve()
    require(not inside(out, ROOT) and not inside(ROOT, out), "evidence must be outside checkout")
    out.mkdir(parents=True, exist_ok=False)
    report = {
        "status": "incomplete", "mode": args.mode, "source_sha": args.source_sha.lower(),
        "deadline_seconds": 3000, "command_max_seconds": 1200,
        "workflow_sha": os.environ.get("GITHUB_WORKFLOW_SHA"),
        "workflow_ref": os.environ.get("GITHUB_WORKFLOW_REF"),
        "github_sha": os.environ.get("GITHUB_SHA"),
        "host": {"platform": platform.platform(), "os": platform.system(),
                 "arch": platform.machine(), "cpu_count": os.cpu_count(),
                 "runner_os": os.environ.get("RUNNER_OS"),
                 "runner_arch": os.environ.get("RUNNER_ARCH")},
        "coverage_limits": "Rust tests on native filesystem, not full native KiCad/GUI tests",
        "installs": "none; workflow must provision rustc, cargo and protoc beforehand",
    }
    runner = Runner(out, report)
    before = None
    primary = None
    try:
        require(sys.platform in ("win32", "darwin"), "only Windows/macOS runners are supported")
        require(sys.version_info >= (3, 9), "Python 3.9+ required")
        require(re.fullmatch(r"[0-9a-fA-F]{40}", report["workflow_sha"] or ""),
                "GITHUB_WORKFLOW_SHA must record the full workflow revision")
        identity = job_identity()
        report["job_identity"] = identity
        if sys.platform == "darwin":
            report["host"]["total_bytes"] = int(runner.probe(["/usr/sbin/sysctl", "-n", "hw.memsize"]))
        else:
            report["host"]["total_bytes"] = runner.memory()["total_bytes"]
        temp_env = os.environ.get("RUNNER_TEMP")
        require(temp_env and Path(temp_env).is_absolute(), "absolute RUNNER_TEMP required")
        temp = Path(temp_env).resolve(strict=True)
        require(temp.is_dir() and not inside(temp, ROOT), "RUNNER_TEMP must be outside checkout")
        runner.disks.append(temp)
        runner.capacity(initial=True)
        before = source_snapshot(runner, "before", args.source_sha.lower())
        report["source_manifest_before"] = "source-before.json"
        report["tool_versions"] = {
            name: runner.text(f"version-{name}", [name, "--version"])
            for name in ("rustc", "cargo", "protoc")
        }
        report["rustc_verbose"] = runner.text("rustc-verbose", ["rustc", "-vV"])
        if args.mode == "core":
            target = Path(tempfile.mkdtemp(prefix="konnect-rename-target-", dir=temp)).resolve()
            marker = {"source_sha": args.source_sha.lower(), "job_identity": identity,
                      "core_evidence": str(out)}
            save(target / ".konnect-rename-owned.json", marker)
        else:
            require(sys.platform == "win32", "UNC checks require Windows")
            core_env = os.environ.get("KONNECT_RENAME_CORE_EVIDENCE")
            require(core_env and Path(core_env).is_absolute(), "UNC requires same-job core evidence")
            core_out = Path(core_env).resolve(strict=True)
            require(not inside(core_out, ROOT), "core evidence must be external")
            core = json.loads((core_out / "report.json").read_text(encoding="utf-8"))
            require(core["status"] == "passed" and core["mode"] == "core",
                    "UNC requires a successful completed core invocation")
            require(core["source_sha"] == args.source_sha.lower()
                    and core["job_identity"] == identity
                    and core["workflow_sha"] == report["workflow_sha"]
                    and core["host"]["os"] == report["host"]["os"]
                    and core["host"]["arch"] == report["host"]["arch"],
                    "UNC cache source/job/workflow/host mismatch")
            require(json.loads((core_out / "source-after.json").read_text(encoding="utf-8")) == before,
                    "UNC source differs from successful core source")
            target = Path(core["cargo_target_dir"]).resolve(strict=True)
            require(target.parent == temp and target.name.startswith("konnect-rename-target-"),
                    "UNC target is not a core-created external runner target")
            marker = json.loads((target / ".konnect-rename-owned.json").read_text(encoding="utf-8"))
            require(marker == {"source_sha": args.source_sha.lower(), "job_identity": identity,
                               "core_evidence": str(core_out)}, "UNC target ownership mismatch")
            require(bool(os.environ.get("KONNECT_RENAME_UNC_ROOT")), "workflow must provision UNC root")
            report["reused_core_evidence"] = str(core_out)
        runner.env["CARGO_TARGET_DIR"] = str(target)
        runner.env["TMPDIR"] = str(temp)
        runner.env["TMP"] = str(temp)
        runner.env["TEMP"] = str(temp)
        runner.disks.append(target)
        runner.capacity(initial=True)
        report["cargo_target_dir"] = str(target)
        report["build_environment"] = {key: runner.env[key] for key in (
            "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "RUST_TEST_THREADS",
            "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG", "CARGO_INCREMENTAL"
        )}
        report["tests"] = {}
        if args.mode == "core":
            runner.run("konnect-build", ["cargo", "build", "--locked", "-p", "konnect"])
            for label, module, count in (
                ("rename-focused", "rename_revision_tests", 21),
                ("rename-hierarchy", "rename_hierarchy_tests", 2),
                ("rename-rewrite", "rename_rewrite_tests", 5),
            ):
                log = runner.run(label, [
                    "cargo", "test", "--locked", "-p", "konnect-core", "--lib", module,
                    "--", "--nocapture", "--test-threads=1",
                ])
                report["tests"][label] = test_counts(
                    log, count, f"tools::project::{module}::"
                )
            log = runner.run("workspace-tests", [
                "cargo", "test", "--workspace", "--locked", "--lib", "--tests",
                "--", "--test-threads=2",
            ])
            report["tests"]["workspace"] = test_counts(log)
            remaining = DEADLINE - time.monotonic()
            if remaining >= 600:
                runner.run("workspace-clippy", [
                    "cargo", "clippy", "--workspace", "--locked", "--all-targets",
                    "--", "-D", "warnings",
                ], timeout=min(1200, int(remaining) - 60))
                report["clippy"] = "passed"
            else:
                report["clippy"] = "not run: less than 600s left in fixed invocation deadline"
            report["doctests"] = "not requested; --lib --tests does not include doctests"
            report["rustfmt"] = "not run: optional; no rustfmt install attempted"
        else:
            log = runner.run("unc-served", [
                "cargo", "test", "-p", "konnect-core", "--locked",
                "--test", "project_rename_unc", "--", "--ignored", "--exact",
                "windows::served_rename_preserves_unc_files_and_collisions",
                "--nocapture", "--test-threads=1",
            ])
            report["tests"]["unc"] = test_counts(
                log, 1, "windows::served_rename_preserves_unc_files_and_collisions"
            )
            text = log.read_text(encoding="utf-8", errors="strict")
            observations = re.findall(r"UNC_EVIDENCE (\{.+\})$", text, re.MULTILINE)
            require(len(observations) == 1, "missing exact UNC case accounting")
            report["unc_observation"] = json.loads(observations[0])
            require(report["unc_observation"]["tested_cases"] == 3, "expected three UNC cases")
            require(len(re.findall(r"UNC_PATH \{", text)) == 3,
                    "missing UNC input/canonical path observations")
        report["status"] = "checks_complete"
    except BaseException as error:
        primary = error
        report.update(status="failed", error=repr(error), traceback=traceback.format_exc())
    finally:
        try:
            if before is not None:
                after = source_snapshot(runner, "after", args.source_sha.lower())
                report["source_manifest_after"] = "source-after.json"
                require(after == before, "tracked source manifest changed during validation")
                report["source_unchanged"] = True
            else:
                report["source_unchanged"] = "unverified: initial source snapshot did not complete"
                report["status"] = "failed"
            require(time.monotonic() <= DEADLINE, "fixed invocation deadline exceeded")
            if report["status"] == "checks_complete":
                if args.mode == "core":
                    env_file = os.environ.get("GITHUB_ENV")
                    require(env_file, "GITHUB_ENV required for same-job core cache handoff")
                    require("\n" not in str(out) and "\r" not in str(out), "invalid evidence path")
                    with Path(env_file).open("a", encoding="utf-8") as handle:
                        handle.write(f"KONNECT_RENAME_CORE_EVIDENCE={out}\n")
                    report["core_cache_handoff"] = "KONNECT_RENAME_CORE_EVIDENCE in GITHUB_ENV"
                report["status"] = "passed"
        except BaseException as error:
            report.update(status="failed", final_audit_error=repr(error))
            if primary is None:
                primary = error
        report["elapsed_seconds"] = round(time.monotonic() - START, 3)
        report["target_cleanup"] = "retained owned external target for same-job UNC; runner disposal owns removal"
        save(out / "report.json", report)
    print(json.dumps({"status": report["status"], "evidence": str(out)}, sort_keys=True))
    if primary is not None or report["status"] != "passed":
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
