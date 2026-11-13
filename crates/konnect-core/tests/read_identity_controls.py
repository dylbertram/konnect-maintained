"""Run only in a coordinator-admitted disposable remote source snapshot.

Each control changes response construction, runs the served regression tests,
requires the exact expected test failures, then restores the original bytes.
No Git history, CAD program, network service or additional Python package is needed.
"""

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[3]
COMPONENTS = ROOT / "crates/konnect-core/src/tools/sch_components.rs"
ANALYSIS = ROOT / "crates/konnect-core/src/tools/sch_analysis.rs"
COMMAND = [
    "cargo", "test", "-p", "konnect-core", "--locked", "--test",
    "read_identities", "--", "--test-threads=1",
]
PIN_TESTS = {
    "served_single_pin_locations_report_native_mirrors",
    "served_batch_pin_locations_report_native_mirrors",
    "served_pin_locations_keep_each_units_own_mirror",
}
LABEL_TESTS = {
    "served_listed_labels_carry_stable_native_uuids",
    "served_listed_label_uuids_delete_only_the_selected_instances",
}
EXPECTED = {
    "before": PIN_TESTS | LABEL_TESTS,
    "unit-mirror-false": PIN_TESTS,
    "anchor-mirror-false": PIN_TESTS,
    "unit-mirror-anchor": {"served_pin_locations_keep_each_units_own_mirror"},
    "unit-mirror-swapped": PIN_TESTS,
    "anchor-mirror-swapped": PIN_TESTS,
    "net-uuid-empty": LABEL_TESTS,
    "global-uuid-empty": LABEL_TESTS,
    "hierarchical-uuid-empty": LABEL_TESTS,
}


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f"control target must occur exactly once: {old!r}")
    return source.replace(old, new, 1)


def scoped(source, start, end, edits):
    if source.count(start) != 1 or source.count(end) != 1:
        raise RuntimeError("control function boundaries changed")
    prefix, rest = source.split(start, 1)
    body, suffix = rest.split(end, 1)
    for old, new in edits:
        body = replace_once(body, old, new)
    return prefix + start + body + end + suffix


def mutate(originals, name):
    component_edits = []
    label_edits = []
    if name == "before":
        for owner, indent in [("instance", 12), ("anchor", 8)]:
            for axis in ["x", "y"]:
                component_edits.append((
                    f'{" " * indent}"mirror_{axis}": {owner}.mirror_{axis},\n', ""
                ))
        for variable in ["l", "g", "h"]:
            label_edits.append((f'"uuid": {variable}.uuid, ', ""))
    elif name.startswith(("unit-", "anchor-")):
        owner = "instance" if name.startswith("unit-") else "anchor"
        for axis, opposite in [("x", "y"), ("y", "x")]:
            if name.endswith("-false"):
                value = "false"
            elif name.endswith("-anchor"):
                value = f"anchor.mirror_{axis}"
            else:
                value = f"{owner}.mirror_{opposite}"
            component_edits.append((
                f'"mirror_{axis}": {owner}.mirror_{axis},',
                f'"mirror_{axis}": {value},',
            ))
    else:
        variable = {"net-uuid-empty": "l", "global-uuid-empty": "g",
                    "hierarchical-uuid-empty": "h"}[name]
        label_edits.append((f'"uuid": {variable}.uuid,', '"uuid": "",'))

    changed = {}
    if component_edits:
        changed[COMPONENTS] = scoped(
            originals[COMPONENTS].decode(),
            "fn pin_locations_for_reference(\n",
            "\nasync fn handle_batch_get_pin_locations(",
            component_edits,
        ).encode()
    if label_edits:
        changed[ANALYSIS] = scoped(
            originals[ANALYSIS].decode(),
            "async fn handle_list_labels(\n",
            "\nasync fn handle_get_net_connections(",
            label_edits,
        ).encode()
    return changed


def run_tests():
    result = subprocess.run(
        COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        env={**os.environ, "CARGO_TERM_COLOR": "never"},
    )
    print(result.stdout, end="", flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("control", choices=[*EXPECTED, "all"])
    args = parser.parse_args()
    # Do not mistake a missing dependency or a compiler failure for reproduction.
    positive = run_tests()
    if positive.returncode != 0:
        raise RuntimeError("passing-after prerequisite failed; no control applied")
    originals = {path: path.read_bytes() for path in [COMPONENTS, ANALYSIS]}
    names = list(EXPECTED) if args.control == "all" else [args.control]
    for name in names:
        changes = mutate(originals, name)
        print(f"\nREAD IDENTITY CONTROL: {name}", flush=True)
        try:
            for path, source in changes.items():
                path.write_bytes(source)
            result = run_tests()
            failed = set(re.findall(r"^test (\w+) \.\.\. FAILED$", result.stdout, re.M))
            if result.returncode != 101 or failed != EXPECTED[name]:
                raise RuntimeError(
                    f"{name}: expected {sorted(EXPECTED[name])}, "
                    f"got exit {result.returncode} and {sorted(failed)}"
                )
        finally:
            for path in changes:
                path.write_bytes(originals[path])
        print(f"CONTROL REJECTED AS EXPECTED: {name}; source bytes restored", flush=True)
    # Recheck after restoration, rather than assuming finally was sufficient.
    if any(path.read_bytes() != source for path, source in originals.items()):
        raise RuntimeError("response source restoration failed")
    if run_tests().returncode != 0:
        raise RuntimeError("passing-after verification failed after controls")
    return 0


if __name__ == "__main__":
    sys.exit(main())
