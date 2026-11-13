"""Coordinator-only controls on an exclusively owned, admitted Linux worker.

No Git, network or native calls. Offline Cargo only. Mutate symbol.rs alone,
restore exact bytes, and rerun positives. Compilation errors, zero tests,
timeouts and unrelated failures never count as a caught assertion.
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
SYMBOL = Path("crates/konnect-schematic-editor/src/schematic/symbol.rs")
BEFORE = Path("crates/konnect-core/tests/support/symbol_before.rs.txt")
BASE_SYMBOL_SHA256 = "3e8d312d2cfdc556d2d90919ad57eea51c84624ada5ea70d81c898526e8996e2"
# Filled from the final reviewed files, never learned or refreshed at runtime.
HASHES = {
    SYMBOL: "226286fc2887f9f2a96fd629d74f34bc584c69372fd4ee42d0460d6d23ef8807",
    BEFORE: BASE_SYMBOL_SHA256,
    Path("crates/konnect-schematic-editor/tests/symbol_serialization.rs"):
        "f2d6ae0e5fc76d0e3b6b363ce5e6842923ce06f3d33f5a32b4c0128e3056f8f7",
    Path("crates/konnect-schematic-editor/tests/support/symbol_serialization.rs"):
        "9603a06666ec09bb8d2e127d89e5cfa177cbbe5927a1a0b34a3b749bc1824543",
    Path("crates/konnect-core/tests/symbol_serialization.rs"):
        "8e94f82cd98301556121df1a0412adcc2cd2254252ab2cd6ade2b4861fad396b",
    Path("crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.kicad_sch"):
        "1bbd268c32a780f2314fb0a280405be92800086ff2ac1a5cf50b15f5ca266398",
    Path("crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.provenance.json"):
        "84ca2f14ff5d64a80930a2c2b60e0d7c61dc9481c5530967c4b3e4cc3a4d8fa0",
    Path("crates/konnect-schematic-editor/tests/fixtures/text_preservation_kicad10.README.md"):
        "7cb53b2e7c95d435e14ff840d961da7ec7f991605f000a038bd53fa562d22ac5",
    Path("crates/konnect-core/tests/support/pcb_export_supervisor.py"):
        "95108143509ca77a8b3e112ebe6e9386daf33e21712c67afa3213bd57fa0a665",
    Path("crates/konnect-schematic-editor/src/schematic/mod.rs"):
        "5be8d45a5bb7f5cd17b263b70237a2ad092727f58f20673bd868b7699117b5f3",
    Path("crates/konnect-schematic-editor/src/types.rs"):
        "651566a7f62c3db7b1c1f65eea173fdd46615a92b50ba03c4a5ac42e1b6ffcc3",
    Path("crates/konnect-schematic-editor/src/sexp/mod.rs"):
        "80128b7f01bbbc691b532a9eee39f5c24ba2119c3c8b1defacf5e04a04f60c17",
    Path("crates/konnect-schematic-editor/src/sexp/writer.rs"):
        "28f0cc661003e3487b0b67c0dfd9e7800597d05c2a367d5d0cb4c0ada1422a39",
    Path("crates/konnect-core/src/tools/sch_wiring.rs"):
        "45d63b98af7b63a283f134edf76bd55d9a559f9ba6c67924b740b8b667c020ee",
    Path("Cargo.lock"): "60d54cdb64f5e686f0475abe0cab72c655fca7cb6756b6cb03f66404daebef99",
}
TESTS = {
    "model-native": ("model", "native_25_symbol_blocks_round_trip_exactly"),
    "model-flags": ("model", "flag_syntax_survives_unrelated_writes"),
    "model-raw": ("model", "invalid_raw_attributes_and_duplicate_clauses_survive_once"),
    "model-unknown": ("model", "unknown_scalar_and_list_children_keep_content_and_mutual_order"),
    "model-intentional": ("model", "intentional_bool_edits_override_raw_syntax_with_yes_or_no"),
    "model-new": ("model", "constructors_and_existing_exhaustive_literals_remain_source_compatible"),
    "served-native": ("served", "served_nonnoop_add_wire_preserves_all_25_native_symbol_blocks"),
    "served-flags": ("served", "served_nonnoop_add_wire_preserves_flag_syntax"),
    "served-raw": ("served", "served_nonnoop_add_wire_preserves_raw_attributes_and_unknown_children"),
}
CONTROLS = {
    "failing-before": ("model-native", "served-native"),
    "flag-bare": ("model-native", "served-native"),
    "no-is-presence": ("model-flags",),
    "flag-capture-drop": ("model-flags", "served-flags"),
    "body-tail": ("model-native", "served-native"),
    "positions-tail": ("model-native", "served-native"),
    "flag-tail": ("model-native", "served-native"),
    "raw-tail-duplication": ("model-native", "served-native"),
    "invalid-raw-drop": ("model-raw", "served-raw"),
    "unknown-drop": ("model-unknown", "served-raw"),
    "unknown-reorder": ("model-unknown", "served-raw"),
    "invented-absence": ("model-native", "model-new", "served-native"),
    "raw-always-wins": ("model-intentional",),
    "false-removes-clause": ("model-intentional",),
    "invented-raw-defaults": ("model-new", "model-raw"),
}
RAW_PREDICATE = """!matches!(
                        node.tag(),
                        Some("body_style" | "in_pos_files" | "fields_autoplaced")
                    )"""
TAIL = "        c.extend(\n            self.raw_sub_nodes\n"
EDIT = "        if self.fields_autoplaced != fields_autoplaced_value(original) {\n"
CANONICAL_FLAG = """            children.push(tagged(
                "fields_autoplaced",
                vec![atom(bool_kw(self.fields_autoplaced))],
            ));
"""
MAX_LOG_BYTES = 4 * 1024 * 1024


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"expected exactly one source target: {old!r}")
    return source.replace(old, new, 1)


def mutate(originals, name):
    source = originals[SYMBOL].decode("utf-8")
    if name == "failing-before":
        changed = originals[BEFORE]
        if hashlib.sha256(changed).hexdigest() != BASE_SYMBOL_SHA256:
            raise RuntimeError("failing-before must be the complete original exact symbol.rs")
        return changed
    if name == "flag-bare":
        source = replace_once(
            source, '        self.emit_fields_autoplaced(&mut c);\n',
            '        if self.fields_autoplaced {\n'
            '            c.push(tagged("fields_autoplaced", vec![]));\n'
            '        }\n',
        )
    elif name == "no-is-presence":
        source = replace_once(
            source, '        Some(node) => !matches!(node.args(), [SexpNode::Atom(value)] if value == "no"),\n',
            "        Some(_) => true,\n",
        )
    elif name == "flag-capture-drop":
        source = replace_once(
            source, '            "dnp",\n            "uuid",\n',
            '            "dnp",\n            "fields_autoplaced",\n            "uuid",\n',
        )
    elif name in ("body-tail", "positions-tail"):
        tag = "body_style" if name == "body-tail" else "in_pos_files"
        emission = f'        self.emit_raw_attribute(&mut c, "{tag}");\n'
        source = replace_once(source, emission, "")
        source = replace_once(source, TAIL, emission + TAIL)
    elif name == "flag-tail":
        emission = "        self.emit_fields_autoplaced(&mut c);\n"
        source = replace_once(source, emission, "")
        source = replace_once(source, TAIL, emission + TAIL)
    elif name == "raw-tail-duplication":
        source = replace_once(
            source, ".filter(|node| {\n                    " + RAW_PREDICATE + "\n                })",
            ".filter(|_node| true)",
        )
    elif name == "invalid-raw-drop":
        source = replace_once(
            source, ".filter(|node| node.tag() == Some(tag))",
            '.filter(|node| node.tag() == Some(tag) && node.value().is_some_and(|value| match tag {\n'
            '                    "body_style" => value.parse::<u32>().is_ok(),\n'
            '                    "in_pos_files" => matches!(value, "yes" | "no"),\n'
            '                    _ => true,\n'
            '                }))',
        )
    elif name == "unknown-drop":
        source = replace_once(
            source, RAW_PREDICATE,
            RAW_PREDICATE + '\n                    && node.tag() != Some("future_symbol_child")'
            '\n                    && node.text() != Some("future_bare_flag")'
            '\n                    && node.text() != Some("future quoted argument")',
        )
    elif name == "unknown-reorder":
        source = replace_once(
            source, TAIL + "                .iter()\n",
            TAIL + "                .iter().rev()\n",
        )
    elif name == "invented-absence":
        signature = "    fn emit_fields_autoplaced(&self, children: &mut Vec<SexpNode>) {\n"
        source = replace_once(
            source, signature,
            signature + '        if !self.fields_autoplaced && !self.raw_sub_nodes.iter().any(|n| n.tag() == Some("fields_autoplaced")) {\n'
            '            children.push(tagged("fields_autoplaced", vec![atom("no")]));\n'
            '        }\n',
        )
    elif name == "raw-always-wins":
        source = replace_once(
            source, EDIT, "        if original.is_none() && self.fields_autoplaced {\n",
        )
    elif name == "false-removes-clause":
        source = replace_once(
            source, CANONICAL_FLAG,
            "            if self.fields_autoplaced {\n" + CANONICAL_FLAG + "            }\n",
        )
    elif name == "invented-raw-defaults":
        signature = "    fn emit_raw_attribute(&self, children: &mut Vec<SexpNode>, tag: &str) {\n"
        source = replace_once(
            source, signature,
            signature + '        if matches!(tag, "body_style" | "in_pos_files") && !self.raw_sub_nodes.iter().any(|n| n.tag() == Some(tag)) {\n'
            '            children.push(tagged(tag, vec![atom(if tag == "body_style" { "1" } else { "yes" })]));\n'
            '        }\n',
        )
    else:
        raise RuntimeError(f"unknown control {name}")
    changed = source.encode("utf-8")
    if changed == originals[SYMBOL]:
        raise RuntimeError(f"{name}: no changed source bytes")
    return changed


def check_bytes(originals):
    for path, expected in originals.items():
        if (ROOT / path).read_bytes() != expected:
            raise RuntimeError(f"reviewed source drift: {path}")


def restore(originals, owned):
    current = (ROOT / SYMBOL).read_bytes()
    if current not in owned:
        raise RuntimeError("refusing to overwrite an unowned symbol.rs revision")
    if current != originals[SYMBOL]:
        (ROOT / SYMBOL).write_bytes(originals[SYMBOL])
    check_bytes(originals)


def stop_owned_group(process):
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
    package = "konnect-schematic-editor" if kind == "model" else "konnect-core"
    return [
        "cargo", "test", "--offline", "--locked", "-p", package,
        "--test", "symbol_serialization", test, "--", "--exact", "--test-threads=1",
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
    parser.add_argument("--output-dir", default="symbol-serialization-controls")
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
    if SYMBOL not in HASHES or BEFORE not in HASHES or len(HASHES) < 8:
        raise RuntimeError("reviewed source hashes were not pinned")
    for path, digest in HASHES.items():
        source = (ROOT / path).read_bytes()
        if hashlib.sha256(source).hexdigest() != digest:
            raise RuntimeError(f"pinned symbol-serialization source changed: {path}")
        originals[path] = source
    if hashlib.sha256(originals[BEFORE]).hexdigest() != BASE_SYMBOL_SHA256:
        raise RuntimeError("original complete symbol.rs snapshot changed")
    names = list(CONTROLS) if args.control == "all" else [args.control]
    changes = {name: mutate(originals, name) for name in names}
    owned = [originals[SYMBOL], *changes.values()]
    directory = (ROOT / args.output_dir).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + args.work_budget
    try:
        for key in TESTS:
            run_test(key, False, "positive-before", directory, deadline, args.command_timeout)
        check_bytes(originals)
        for name in names:
            check_bytes(originals)
            try:
                (ROOT / SYMBOL).write_bytes(changes[name])
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
