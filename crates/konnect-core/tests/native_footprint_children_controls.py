#!/usr/bin/env python3
"""Coordinator-only A8 controls in an admitted disposable remote checkout.

Compile first, then require named assertion failures, not compile errors or
zero tests. Restore exact original bytes in finally, including failing gates.
No native evidence, package installation or job submission happens here.
"""
import argparse
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / "crates/konnect-core/src/tools"
BASE = ["cargo", "test", "-p", "konnect-core", "--locked", "--lib"]
EACH = "a8_served_each_kind_refuses_dry_run_and_apply_without_writes"
RECHECK = "a8_served_apply_rechecks_native_children_despite_unchanged_protobuf"
MIXED = "a8_served_mixed_plan_refuses_safe_update_and_addition_too"
CONTROLS = {
    "ignore-group": ("native_footprint_children.rs",
        "if self.groups > 0 {", "if false {", {EACH: "status"}),
    "ignore-point": ("native_footprint_children.rs",
        "if self.points > 0 {", "if false {", {EACH: "status"}),
    "ignore-variant": ("native_footprint_children.rs",
        "if self.variants > 0 {", "if false {", {EACH: "status"}),
    "sync-no-inventory": ("pcb_sync.rs",
        "&targets, // A8 sync inventory targets",
        "&[], // A8 sync inventory targets",
        {EACH: "status", RECHECK: "status", MIXED: "status"}),
    "refresh-no-inventory": ("pcb_footprint_update.rs",
        "&targets, // A8 refresh inventory targets",
        "&[], // A8 refresh inventory targets",
        {EACH: "status", RECHECK: "status", MIXED: "status"}),
    "sync-no-apply-recheck": ("pcb_sync.rs",
        "&targets, // A8 sync inventory targets",
        "if dry_run { targets.as_slice() } else { &[] }, // A8 sync inventory targets",
        {EACH: "diagnostic", RECHECK: "status", MIXED: "diagnostic"}),
    "refresh-no-apply-recheck": ("pcb_footprint_update.rs",
        "&targets, // A8 refresh inventory targets",
        "if dry_run { targets.as_slice() } else { &[] }, // A8 refresh inventory targets",
        {EACH: "diagnostic", RECHECK: "status", MIXED: "diagnostic"}),
    "mixed-unsafe-overlooked": ("native_footprint_children.rs",
        ".iter() // A8 all replacement targets",
        ".iter().take(1) // A8 all replacement targets", {MIXED: "status"}),
}


def assertion_pins():
    """Resolve exact reviewed assertion sites without a stale numeric line pin."""
    source = (TOOLS / "native_footprint_children_tests.rs").read_text()
    markers = {
        "status": ('    assert_eq!(\n        result["status"], "conflict",',
                   "a8 native children must refuse:"),
        "diagnostic": ('        .expect("a8 named compatibility diagnostic");',
                       "a8 named compatibility diagnostic"),
    }
    pins = {}
    for kind, (marker, message) in markers.items():
        if source.count(marker) != 1:
            raise RuntimeError(f"{kind}: assertion marker must occur exactly once")
        pins[kind] = (source[:source.index(marker)].count("\n") + 1, message)
    return pins


def require_assertion_sites(output, expected, pins):
    """Reject mock/RPC panics and failures at unrelated assertions."""
    parts = re.split(r"^---- (\S+) stdout ----$", output, flags=re.M)
    sections = {parts[i].split("::")[-1]: parts[i + 1]
                for i in range(1, len(parts), 2)}
    for test, kind in expected.items():
        section = sections.get(test, "")
        location = re.search(
            r"panicked at [^\n]*native_footprint_children_tests\.rs:(\d+):\d+:",
            section,
        )
        line, message = pins[kind]
        if not location or int(location[1]) != line or message not in section:
            raise RuntimeError(f"{test}: need {kind} assertion at "
                               f"native_footprint_children_tests.rs:{line}, not another panic")


def command(args):
    result = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT,
                            env={**os.environ, "CARGO_TERM_COLOR": "never"})
    print(result.stdout, end="", flush=True)
    return result


def compile_first():
    if command([*BASE, "--no-run"]).returncode:
        raise RuntimeError("compile failed; never accepted as a control result")


def tests():
    return command([*BASE, "a8_", "--", "--test-threads=1"])


def require_positive():
    compile_first()
    result = tests()
    if result.returncode or not re.search(r"test result: ok\. [1-9]\d* passed;", result.stdout):
        raise RuntimeError("positive gate failed or ran zero tests")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("control", choices=[*CONTROLS, "all"])
    args = parser.parse_args()
    pins = assertion_pins()
    require_positive()
    for name in CONTROLS if args.control == "all" else [args.control]:
        file, old, new, expected = CONTROLS[name]
        path = TOOLS / file
        original = path.read_bytes()
        source = original.decode()
        if source.count(old) != 1:
            raise RuntimeError(f"{name}: expected exactly one control target")
        print(f"A8 CONTROL: {name}", flush=True)
        try:
            path.write_text(source.replace(old, new, 1))
            compile_first()
            result = tests()
            failures = {name.split("::")[-1] for name in re.findall(
                r"^test (\S+) \.\.\. FAILED$", result.stdout, re.M)}
            if (result.returncode != 101 or not set(expected) <= failures
                    or "could not compile" in result.stdout
                    or re.search(r"error\[E\d+\]", result.stdout)):
                raise RuntimeError(f"{name}: need assertion failures {sorted(expected)}, "
                                   f"got exit={result.returncode}, failures={sorted(failures)}")
            require_assertion_sites(result.stdout, expected, pins)
        finally:
            path.write_bytes(original)
            if path.read_bytes() != original:
                raise RuntimeError(f"{name}: exact restoration failed")
            # Also run on an unexpectedly passing or broken control, after
            # exact restoration. Never leave only a mutated failure as evidence.
            require_positive()
        print(f"REJECTED AS EXPECTED: {name}; exact bytes restored and positive passed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
