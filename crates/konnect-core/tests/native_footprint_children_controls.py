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
        "if self.groups > 0 {", "if false {", {EACH}),
    "ignore-point": ("native_footprint_children.rs",
        "if self.points > 0 {", "if false {", {EACH}),
    "ignore-variant": ("native_footprint_children.rs",
        "if self.variants > 0 {", "if false {", {EACH}),
    "sync-no-inventory": ("pcb_sync.rs",
        "&targets, // A8 sync inventory targets",
        "&[], // A8 sync inventory targets", {EACH, RECHECK, MIXED}),
    "refresh-no-inventory": ("pcb_footprint_update.rs",
        "&targets, // A8 refresh inventory targets",
        "&[], // A8 refresh inventory targets", {EACH, RECHECK, MIXED}),
    "sync-no-apply-recheck": ("pcb_sync.rs",
        "&targets, // A8 sync inventory targets",
        "if dry_run { targets.as_slice() } else { &[] }, // A8 sync inventory targets",
        {EACH, RECHECK, MIXED}),
    "refresh-no-apply-recheck": ("pcb_footprint_update.rs",
        "&targets, // A8 refresh inventory targets",
        "if dry_run { targets.as_slice() } else { &[] }, // A8 refresh inventory targets",
        {EACH, RECHECK, MIXED}),
    "mixed-unsafe-overlooked": ("native_footprint_children.rs",
        ".iter() // A8 all replacement targets",
        ".iter().take(1) // A8 all replacement targets", {MIXED}),
}


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
            assertions = "panicked at" in result.stdout and (
                "assertion" in result.stdout or "a8 " in result.stdout)
            if (result.returncode != 101 or not expected <= failures or not assertions
                    or "could not compile" in result.stdout
                    or re.search(r"error\[E\d+\]", result.stdout)):
                raise RuntimeError(f"{name}: need assertion failures {sorted(expected)}, "
                                   f"got exit={result.returncode}, failures={sorted(failures)}")
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
