#!/usr/bin/env python3
"""Linux-only native export test supervisor. Not a production CLI wrapper.

The adjacent JSON file supplies one executable, a child runtime limit and an
audit directory. No shell is involved. This process must survive cancellation
of the Rust tools/call future so its deadline can still stop and reap the child.
The trusted native tools and test stubs must not escape their process group.
"""

import ctypes
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def main():
    if sys.platform != "linux":
        raise RuntimeError("native export supervision is supported on Linux only")
    config = json.loads(Path(__file__).with_suffix(".json").read_text())
    limit = config["seconds"]
    if not isinstance(limit, int) or not 1 <= limit <= 60:
        raise ValueError("child runtime limit must be 1..60 seconds")

    # Adopt descendants so cleanup can reap the entire test-owned group, not
    # just kill the CLI leader and leave its grandchildren as PID 1's zombies.
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:  # PR_SET_CHILD_SUBREAPER
        raise OSError(ctypes.get_errno(), "PR_SET_CHILD_SUBREAPER failed")

    audit = Path(config["audit"]) / str(os.getpid())
    audit.mkdir(parents=True)

    def record(name, data):
        temporary = audit / (name + ".tmp")
        temporary.write_text(json.dumps(data))
        temporary.replace(audit / name)

    child = None
    interrupted = 0

    def stop_group():
        if child is not None:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    def interrupt(signum, _frame):
        nonlocal interrupted
        interrupted = signum
        stop_group()

    signal.signal(signal.SIGTERM, interrupt)
    signal.signal(signal.SIGINT, interrupt)
    if interrupted:
        return 128 + interrupted

    deadline = time.monotonic() + limit
    child = subprocess.Popen(
        [config["program"], *sys.argv[1:]],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    timed_out = False
    descendants = []
    try:
        if interrupted:
            stop_group()
        record("started.json", {"supervisor": os.getpid(), "child": child.pid})
        try:
            child.communicate(timeout=max(0, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            timed_out = True
    finally:
        # Always stop remaining group members, including descendants after an
        # otherwise successful CLI exit, then collect pipes and reap the leader.
        stop_group()
        stdout, stderr = child.communicate()
        while True:
            try:
                pid, status = os.waitpid(-child.pid, 0)
                descendants.append(
                    {"pid": pid, "status": os.waitstatus_to_exitcode(status)}
                )
            except ChildProcessError:
                break

    record(
        "finished.json",
        {
            "supervisor": os.getpid(),
            "child": child.pid,
            "status": child.returncode,
            "timed_out": timed_out,
            "interrupted": interrupted,
            "group_reaped": True,
            "descendants": descendants,
        },
    )
    # The caller may already have dropped its output future. Cleanup and audit
    # precede forwarding, and a closed pipe must not prevent process cleanup.
    if timed_out:
        stderr += b"native export supervisor: child runtime limit exceeded\n"
    for fd, data in ((1, stdout), (2, stderr)):
        try:
            while data:
                data = data[os.write(fd, data) :]
        except BrokenPipeError:
            pass
    if timed_out:
        return 124
    if interrupted:
        return 128 + interrupted
    return child.returncode if child.returncode >= 0 else 128 - child.returncode


if __name__ == "__main__":
    sys.exit(main())
