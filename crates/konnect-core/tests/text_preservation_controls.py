"""Coordinator-only A5 controls on a disposable, capacity-admitted Linux worker.

No Git, network or native KiCad calls. Uses offline Cargo integration targets.
Only misc.rs is changed, one isolated variant at a time, then restored exactly.
Compilation failures and zero-test results never count as caught controls.
"""

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[3]
MISC = Path("crates/konnect-schematic-editor/src/schematic/misc.rs")
BEFORE = Path("crates/konnect-core/tests/support/text_before.rs.txt")
# Filled from the final reviewed source snapshot, not discovered at runtime.
HASHES = {
    MISC: "8589889ecd32827384ab9737f6ccd1a2c4222621999ff1eea7e432e09a6be92e",
    BEFORE: "6f6917c3543d29fbc846bf9f0ef35105eb0cf10f72451f9a11341ffc472824f6",
    Path("crates/konnect-schematic-editor/tests/text_round_trip.rs"):
        "4aad19fc8137b0af3a948567100c69c6f313f8647508a07fc74246771dcb2d16",
    Path("crates/konnect-schematic-editor/tests/support/text_preservation.rs"):
        "d3487448a894ff300802a6a3f6d8b8883b08510d8d5b1ce99f5d6cbafa9c6fad",
    Path("crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.kicad_sch"):
        "1bbd268c32a780f2314fb0a280405be92800086ff2ac1a5cf50b15f5ca266398",
    Path("crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.provenance.json"):
        "84ca2f14ff5d64a80930a2c2b60e0d7c61dc9481c5530967c4b3e4cc3a4d8fa0",
    Path("crates/konnect-core/tests/text_preservation.rs"):
        "622db83ed30437d2a042e31b79738c469df7174f8857d0d56b8a15e36c46d985",
    Path("crates/konnect-core/tests/support/pcb_export_supervisor.py"):
        "95108143509ca77a8b3e112ebe6e9386daf33e21712c67afa3213bd57fa0a665",
}
TESTS = {
    "model-native": ("model", "native_text_blocks_round_trip_exactly"),
    "model-states": ("model", "yes_no_and_absence_keep_their_distinct_values_and_position"),
    "model-unknown": ("model", "unknown_scalar_and_list_children_survive_without_duplicate_content"),
    "model-new": ("model", "constructor_does_not_invent_an_exclusion_flag_or_replay_content"),
    "served-native": ("served", "served_add_wire_preserves_native_text_blocks_exactly"),
    "served-states": ("served", "served_add_wire_preserves_yes_no_absence_and_unknown_children"),
}
CONTROLS = {
    "failing-before": ("model-native", "served-native"),
    "flag-drop": ("model-native", "served-native"),
    "flag-order": ("model-native", "served-native"),
    "presence-is-true": ("model-states", "served-states"),
    "unknown-drop": ("model-unknown", "served-states"),
    "text-duplication": ("model-native", "served-native"),
    "invented-absence": ("model-states", "served-states"),
}
START = "// ---- Text -------------------------------------------------------------------\n"
END = "// ---- NoConnect --------------------------------------------------------------\n"
BASE_MISC_SHA256 = "6eabe5fb06f5aaa5c6d5359b51dc55f894e2a587dc2daf988fdfb07874a8d1a8"
FLAG = (
    "        if let Some(excluded) = self.exclude_from_sim {\n"
    "            c.push(tagged(\n"
    '                "exclude_from_sim",\n'
    '                vec![atom(if excluded { "yes" } else { "no" })],\n'
    "            ));\n"
    "        }\n"
)
READ_FLAG = '        let exclude_from_sim = node.get_bool("exclude_from_sim");\n'
MAX_LOG_BYTES = 4 * 1024 * 1024


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"expected exactly one source target: {old!r}")
    return source.replace(old, new, 1)


def mutate(originals, name):
    source = originals[MISC].decode("utf-8")
    if source.count(START) != 1 or source.count(END) != 1:
        raise RuntimeError("Text section boundaries changed")
    prefix, rest = source.split(START, 1)
    body, suffix = rest.split(END, 1)
    if name == "failing-before":
        before = originals[BEFORE].decode("utf-8")
        if not before.startswith(START) or not before.endswith(END):
            raise RuntimeError("baseline Text section boundaries changed")
        body = before[len(START):-len(END)]
    elif name == "flag-drop":
        body = replace_once(body, FLAG, "")
    elif name == "flag-order":
        body = replace_once(body, FLAG, "")
        at = "        c.push(self.at.to_sexp());\n"
        body = replace_once(body, at, at + FLAG)
    elif name == "presence-is-true":
        body = replace_once(
            body, READ_FLAG,
            '        let exclude_from_sim = node.find("exclude_from_sim").map(|_| true);\n',
        )
    elif name == "unknown-drop":
        body = replace_once(body, "        c.extend(self.raw_sub_nodes.iter().cloned());\n", "")
    elif name == "text-duplication":
        body = replace_once(body, "            .skip(1)\n", "            .skip(0)\n")
    elif name == "invented-absence":
        body = replace_once(
            body, READ_FLAG,
            '        let exclude_from_sim = Some(node.get_bool("exclude_from_sim").unwrap_or(false));\n',
        )
    else:
        raise RuntimeError(f"unknown control {name}")
    changed = (prefix + START + body + END + suffix).encode("utf-8")
    if name == "failing-before" and hashlib.sha256(changed).hexdigest() != BASE_MISC_SHA256:
        raise RuntimeError("failing-before must reproduce the exact original misc.rs")
    if changed == originals[MISC]:
        raise RuntimeError(f"{name}: no changed source bytes")
    return changed


def check_bytes(originals):
    for path, expected in originals.items():
        if (ROOT / path).read_bytes() != expected:
            raise RuntimeError(f"reviewed source drift: {path}")


def restore(originals, owned):
    current = (ROOT / MISC).read_bytes()
    if current not in owned:
        raise RuntimeError("refusing to overwrite an unowned misc.rs revision")
    if current != originals[MISC]:
        (ROOT / MISC).write_bytes(originals[MISC])
    check_bytes(originals)


def stop_owned_group(process):
    # This runner is a Linux subreaper. Reap adopted group descendants as well
    # as Cargo itself, including when Cargo exited before one of its children.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait(timeout=5)
    deadline = time.monotonic() + 5
    reaped = []
    while True:
        try:
            pid, status = os.waitpid(-process.pid, os.WNOHANG)
        except ChildProcessError:
            return reaped
        if pid:
            reaped.append({"pid": pid, "status": os.waitstatus_to_exitcode(status)})
        elif time.monotonic() >= deadline:
            raise RuntimeError("owned command group did not finish reaping")
        else:
            time.sleep(0.05)


def command_for(key):
    kind, test = TESTS[key]
    target = (
        ["-p", "konnect-schematic-editor", "--test", "text_round_trip"]
        if kind == "model" else ["-p", "konnect-core", "--test", "text_preservation"]
    )
    return [
        "cargo", "test", "--offline", "--locked", *target,
        test, "--", "--exact", "--test-threads=1",
    ]


def run_test(key, failure, label, directory, deadline, command_timeout):
    command = command_for(key)
    test = TESTS[key][1]
    budget = min(command_timeout, deadline - time.monotonic())
    if budget <= 0:
        raise RuntimeError(f"{label}: phase budget exhausted before {test}")
    stem = f"{label}-{key}"
    log = directory / (stem + ".log")
    audit = {"command": command, "expected_failure": failure, "group_reaped": False}
    print(f"{label}: {shlex.join(command)}", flush=True)
    process = None
    cleaned = False
    try:
        with log.open("xb") as output:
            process = subprocess.Popen(
                command, cwd=ROOT, stdin=subprocess.DEVNULL,
                stdout=output, stderr=subprocess.STDOUT,
                env={**os.environ, "CARGO_TERM_COLOR": "never"},
                start_new_session=True,
            )
            audit["leader"] = process.pid
            expires = time.monotonic() + budget
            while process.poll() is None:
                if log.stat().st_size > MAX_LOG_BYTES:
                    raise RuntimeError(f"{label}: log exceeded 4 MiB")
                if time.monotonic() >= expires:
                    audit["timed_out"] = True
                    raise RuntimeError(f"{label}: command exceeded {budget:.1f}s")
                time.sleep(0.1)
        audit["descendants"] = stop_owned_group(process)
        cleaned = True
        audit.update(group_reaped=True, status=process.returncode)
        if log.stat().st_size > MAX_LOG_BYTES:
            raise RuntimeError(f"{label}: log exceeded 4 MiB")
        text = log.read_text(encoding="utf-8", errors="replace")
        print(text, end="", flush=True)
        wanted = "FAILED" if failure else "ok"
        results = re.findall(r"^test ([\w:]+) \.\.\. (ok|FAILED)$", text, re.M)
        summary = (
            r"^test result: FAILED\. 0 passed; 1 failed; 0 ignored;"
            if failure else r"^test result: ok\. 1 passed; 0 failed; 0 ignored;"
        )
        if (
            process.returncode != (101 if failure else 0)
            or results != [(test, wanted)]
            or len(re.findall(r"^running 1 test$", text, re.M)) != 1
            or len(re.findall(summary, text, re.M)) != 1
            or re.search(
                r"^error(?:\[E\d+\]:|: (?:could not compile|linking with|failed to run custom build command))",
                text, re.M,
            )
            or (failure and not re.search(r"^assertion .*failed", text, re.M))
        ):
            raise RuntimeError(
                f"{label}: expected actual {test} {wanted}, "
                f"got exit {process.returncode}, results {results}; see {log}"
            )
        audit["accepted"] = True
    finally:
        if process is not None and not cleaned:
            audit["descendants"] = stop_owned_group(process)
            audit.update(group_reaped=True, status=process.returncode)
        (directory / (stem + ".audit.json")).write_text(
            json.dumps(audit, indent=2), encoding="utf-8",
        )


def interrupted(signum, _frame):
    raise KeyboardInterrupt(f"signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("control", choices=[*CONTROLS, "all"])
    parser.add_argument("--coordinator-admitted", action="store_true", required=True)
    parser.add_argument("--output-dir", default="text-preservation-controls")
    parser.add_argument("--command-timeout", type=int, default=120)
    parser.add_argument("--work-budget", type=int, default=360)
    parser.add_argument("--recovery-budget", type=int, default=180)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("requires a disposable admitted Linux worker")
    if not 1 <= args.command_timeout <= 180:
        parser.error("--command-timeout must be 1..180 seconds")
    if not 1 <= args.work_budget <= 360 or not 1 <= args.recovery_budget <= 180:
        parser.error("work/recovery budgets must be 1..360 and 1..180 seconds")
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "PR_SET_CHILD_SUBREAPER failed")
    signal.signal(signal.SIGTERM, interrupted)
    originals = {}
    if MISC not in HASHES or BEFORE not in HASHES or len(HASHES) < 7:
        raise RuntimeError("reviewed source hashes were not pinned")
    for path, digest in HASHES.items():
        source = (ROOT / path).read_bytes()
        if hashlib.sha256(source).hexdigest() != digest:
            raise RuntimeError(f"pinned text-preservation source changed: {path}")
        originals[path] = source
    names = list(CONTROLS) if args.control == "all" else [args.control]
    changes = {name: mutate(originals, name) for name in names}
    owned = [originals[MISC], *changes.values()]
    directory = (ROOT / args.output_dir).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + args.work_budget
    # Every control starts and ends with all six exact positive regressions.
    # An abort during positives still enters exact restoration/recovery.
    try:
        for key in TESTS:
            run_test(key, False, "positive-before", directory, deadline, args.command_timeout)
        check_bytes(originals)
        for name in names:
            check_bytes(originals)
            try:
                (ROOT / MISC).write_bytes(changes[name])
                for key in CONTROLS[name]:
                    run_test(key, True, name, directory, deadline, args.command_timeout)
            finally:
                restore(originals, owned)
            print(f"CONTROL REJECTED: {name}; exact source bytes restored", flush=True)
    finally:
        restore(originals, owned)
        recovery_deadline = time.monotonic() + args.recovery_budget
        for key in TESTS:
            run_test(
                key, False, "positive-restored", directory,
                recovery_deadline, args.command_timeout,
            )
        check_bytes(originals)
    print("Requested controls rejected; exact restoration and positive gates passed.", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
