"""Coordinator-only controls for a disposable, capacity-admitted Linux worker.

Pins production from 90b9bc4, the bounded native-test follow-up and the export
supervisor imported from 410675b. Never run in a live checkout or alongside
another writer. No Git, native KiCad or network calls.
"""

import argparse
import hashlib
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[3]
TYPES = Path("crates/konnect-schematic-editor/src/types.rs")
COMPONENTS = Path("crates/konnect-core/src/tools/sch_components.rs")
HASHES = {
    TYPES: "651566a7f62c3db7b1c1f65eea173fdd46615a92b50ba03c4a5ac42e1b6ffcc3",
    COMPONENTS: "a98c7f3f89fef70d7e49e2045d9b4bf2d972d1e4e0a90b9d0c498cb6f78dca2e",
    Path("crates/konnect-schematic-editor/tests/private_properties.rs"):
        "5e569e9171c5700092fa09b6864fd7c99e77b60f4bb375f55309d9bd041274ec",
    Path("crates/konnect-core/src/tools/sch_private_fields_tests.rs"):
        "a9e0c9ba63aa903f5a3a1292eb1a2d74ae3ce14e8a51932ee0117bbadf7ce55f",
    Path("crates/konnect-core/tests/support/pcb_export_supervisor.py"):
        "95108143509ca77a8b3e112ebe6e9386daf33e21712c67afa3213bd57fa0a665",
    Path("crates/konnect-core/tests/fixtures/private_fields.rs"):
        "930adb1b261babfe9b4db47d9ebe10ad12bf830ad58de5ca1d51178799cf620a",
    Path("crates/konnect-core/tests/fixtures/component_fields_kicad10.kicad_sch"):
        "80d57b769749c8b9828be52a09ad947e7a0aa55715e404805f80b70d688d9f54",
}
SERVED = "tools::sch_components::private_fields_tests::"
TESTS = {
    "model-roundtrip": (
        "model", "private_property_round_trip_keeps_flag_name_value_and_ordered_children",
    ),
    "model-save": ("model", "private_properties_survive_a_whole_file_model_save"),
    "served-read": (
        "served", SERVED + "served_private_properties_read_real_names_on_single_and_multiunit_parts",
    ),
    "served-move": (
        "served", SERVED + "served_whole_file_move_keeps_unselected_private_values_and_accepts_two_flags",
    ),
    "served-update": (
        "served", SERVED + "served_private_and_quoted_field_updates_preserve_headers_on_every_unit",
    ),
    "served-placement": (
        "served", SERVED + "served_private_field_placement_changes_only_its_at_child",
    ),
    "served-harmless": (
        "served", SERVED + "served_harmless_edit_preserves_private_fields_and_every_unrelated_byte",
    ),
    "served-readback-only": (
        "served", SERVED + "served_readback_only_edit_keeps_private_fields_and_source_unchanged",
    ),
}
CONTROLS = {
    "wrong-parse": ("model-roundtrip", "served-read"),
    "writer-flag-loss": ("model-save", "served-move"),
    "wrong-binding": ("served-move",),
    "quoted-name-confusion": ("model-roundtrip", "served-read"),
    "ordinary-only-setter": ("served-update", "served-placement"),
}
MODEL_FLAG = (
    '        let private = matches!(args.first(), '
    'Some(SexpNode::Atom(flag)) if flag == "private");\n'
)
RAW_OFFSET = (
    "        let offset = usize::from(\n"
    '            matches!(node.get(1), Some(konnect_sexp::SexpNode::Atom(flag)) if flag == "private"),\n'
    "        );\n"
)
MAX_LOG_BYTES = 4 * 1024 * 1024


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"expected exactly one source target: {old!r}")
    return source.replace(old, new, 1)


def scoped(source, start, end, edits):
    if source.count(start) != 1 or source.count(end) != 1:
        raise RuntimeError("exact source boundaries changed")
    prefix, rest = source.split(start, 1)
    body, suffix = rest.split(end, 1)
    for old, new in edits:
        body = replace_once(body, old, new)
    return prefix + start + body + end + suffix


def mutate(originals, name):
    if name in ("wrong-parse", "writer-flag-loss", "quoted-name-confusion"):
        edits = {
            "wrong-parse": [
                (MODEL_FLAG, "        let private = false;\n"),
                (
                    "        let sub_nodes = args[offset + 2..].to_vec();\n",
                    "        let sub_nodes = args[offset + 2..].iter()\n"
                    "            .filter(|node| node.is_list()).cloned().collect();\n",
                ),
            ],
            "writer-flag-loss": [
                (
                    "        if self.private {\n"
                    '            children.push(atom("private"));\n'
                    "        }\n",
                    "",
                ),
            ],
            "quoted-name-confusion": [
                (
                    MODEL_FLAG,
                    '        let private = args.first().and_then(|node| node.text()) == Some("private");\n',
                ),
            ],
        }[name]
        path = TYPES
        source = scoped(
            originals[path].decode("utf-8"),
            "impl Property {\n",
            "// ---- Effects (preserved verbatim)",
            edits,
        )
    elif name == "wrong-binding":
        path = COMPONENTS
        source = scoped(
            originals[path].decode("utf-8"),
            "pub(crate) fn component_target_from_source(\n",
            "\nfn component_mutation_readback_from_schematic(",
            [(
                "            let offset = usize::from(\n"
                '                matches!(property.get(1), Some(konnect_sexp::SexpNode::Atom(flag)) if flag == "private"),\n'
                "            );\n",
                "            let offset = 0;\n",
            )],
        )
    elif name == "ordinary-only-setter":
        path = COMPONENTS
        source = scoped(
            originals[path].decode("utf-8"),
            "fn placed_property_value_span(\n",
            "\n/// Build the insertion for a custom property",
            [(
                RAW_OFFSET,
                '        if matches!(node.get(1), Some(konnect_sexp::SexpNode::Atom(flag)) if flag == "private") {\n'
                "            continue;\n"
                "        }\n"
                "        let offset = 0;\n",
            )],
        )
    else:
        raise RuntimeError(f"unknown control: {name}")
    changed = source.encode("utf-8")
    if changed == originals[path]:
        raise RuntimeError(f"{name}: mutation changed no bytes")
    return {path: changed}


def check_bytes(originals):
    for path, expected in originals.items():
        if (ROOT / path).read_bytes() != expected:
            raise RuntimeError(f"source bytes differ from the reviewed snapshot: {path}")


def restore(originals):
    # Only these two files are ever mutated. Test/fixture drift is refused,
    # not overwritten. This worker must have no concurrent source writers.
    errors = []
    for path in (TYPES, COMPONENTS):
        try:
            if not (ROOT / path).exists() or (ROOT / path).read_bytes() != originals[path]:
                (ROOT / path).write_bytes(originals[path])
        except OSError as error:
            errors.append(f"{path}: {error}")
    if errors:
        raise RuntimeError("restoration failed: " + "; ".join(errors))
    check_bytes(originals)


def stop_owned_group(process):
    # Cargo and its test/build children own this new session. Do not kill
    # another worker's processes. Escalation is bounded to five seconds.
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        if sig == signal.SIGTERM:
            time.sleep(0.2)
    process.wait(timeout=5)


def command_for(key):
    kind, test = TESTS[key]
    target = (
        ["-p", "konnect-schematic-editor", "--locked", "--test", "private_properties"]
        if kind == "model" else ["-p", "konnect-core", "--locked", "--lib"]
    )
    # Keep panic output captured so the harness emits one complete result line.
    return ["cargo", "test", "--offline", *target, test, "--", "--exact", "--test-threads=1"]


def run_test(key, failure, label, directory, deadline, command_timeout):
    command = command_for(key)
    name = TESTS[key][1]
    timeout = min(command_timeout, deadline - time.monotonic())
    if timeout <= 0:
        raise RuntimeError(f"{label}: phase budget exhausted before {name}")
    log_path = directory / f"{label}-{key}.log"
    print(f"\n{label}: {shlex.join(command)}", flush=True)
    process = None
    stopped = False
    try:
        with log_path.open("xb") as output:
            process = subprocess.Popen(
                command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT,
                env={**os.environ, "CARGO_TERM_COLOR": "never"},
                start_new_session=True,
            )
            expires = time.monotonic() + timeout
            while process.poll() is None:
                if log_path.stat().st_size > MAX_LOG_BYTES:
                    raise RuntimeError(f"{label}: log exceeded 4 MiB: {log_path}")
                if time.monotonic() >= expires:
                    raise RuntimeError(f"{label}: command exceeded {timeout:.1f}s")
                time.sleep(0.1)
        stop_owned_group(process)
        stopped = True
        if log_path.stat().st_size > MAX_LOG_BYTES:
            raise RuntimeError(f"{label}: log exceeded 4 MiB: {log_path}")
        text = log_path.read_text(encoding="utf-8", errors="replace")
        print(text, end="", flush=True)
        wanted = "FAILED" if failure else "ok"
        # An exit of 101 alone can mean a compiler or build failure. Require
        # one actual named test result and its matching harness summary.
        results = re.findall(r"^test ([\w:]+) \.\.\. (ok|FAILED)$", text, re.M)
        summary = (
            r"^test result: FAILED\. 0 passed; 1 failed; 0 ignored;"
            if failure else r"^test result: ok\. 1 passed; 0 failed; 0 ignored;"
        )
        if (
            process.returncode != (101 if failure else 0)
            or results != [(name, wanted)]
            or len(re.findall(r"^running 1 test$", text, re.M)) != 1
            or len(re.findall(summary, text, re.M)) != 1
            or re.search(
                r"^error(?:\[E\d+\]:|: (?:could not compile|linking with|failed to run custom build command))",
                text, re.M,
            )
        ):
            raise RuntimeError(
                f"{label}: expected actual {name} {wanted}, "
                f"got exit {process.returncode}, results {results}; see {log_path}"
            )
    finally:
        if process is not None and not stopped:
            # Also clean up descendants if Cargo exited first or we received
            # a catchable interrupt. Reap before restoring source bytes.
            stop_owned_group(process)


def interrupted(signum, _frame):
    raise KeyboardInterrupt(f"signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("control", choices=[*CONTROLS, "all"])
    parser.add_argument("--coordinator-admitted", action="store_true", required=True)
    parser.add_argument("--output-dir", default="private-field-controls")
    parser.add_argument("--command-timeout", type=int, default=120)
    parser.add_argument("--work-budget", type=int, default=360)
    parser.add_argument("--recovery-budget", type=int, default=180)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("requires a coordinator-admitted disposable Linux worker")
    if not 1 <= args.command_timeout <= 180:
        parser.error("--command-timeout must be 1..180 seconds")
    if not 1 <= args.work_budget <= 360 or not 1 <= args.recovery_budget <= 180:
        parser.error("work/recovery budgets must be 1..360 and 1..180 seconds")
    signal.signal(signal.SIGTERM, interrupted)
    originals = {}
    for path, digest in HASHES.items():
        source = (ROOT / path).read_bytes()
        if hashlib.sha256(source).hexdigest() != digest:
            raise RuntimeError(f"pinned private-fields source precondition changed: {path}")
        originals[path] = source
    names = list(CONTROLS) if args.control == "all" else [args.control]
    # Validate every requested source replacement before starting any command.
    changes = {name: mutate(originals, name) for name in names}
    # Always exercise a genuine changed-byte edit and a readback-only request
    # before/after controls, even though neither is itself a mutation target.
    keys = list(dict.fromkeys(
        ["served-harmless", "served-readback-only"]
        + [key for name in names for key in CONTROLS[name]]
    ))
    directory = (ROOT / args.output_dir).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + args.work_budget
    for key in keys:
        run_test(key, False, "positive-before", directory, deadline, args.command_timeout)
    check_bytes(originals)
    try:
        for name in names:
            check_bytes(originals)
            try:
                for path, source in changes[name].items():
                    (ROOT / path).write_bytes(source)
                for key in CONTROLS[name]:
                    run_test(key, True, name, directory, deadline, args.command_timeout)
            finally:
                restore(originals)
            print(f"CONTROL REJECTED AS EXPECTED: {name}; exact bytes restored", flush=True)
    finally:
        restore(originals)
        # A separate reserved budget lets restoration verification run even if
        # mutation execution exhausted its phase or raised an exception.
        recovery_deadline = time.monotonic() + args.recovery_budget
        for key in keys:
            run_test(key, False, "positive-restored", directory, recovery_deadline, args.command_timeout)
        check_bytes(originals)
    print("All requested controls rejected; restored positives passed.", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
