#!/usr/bin/env python3
"""Coordinator-only mutation gate in a disposable remote source snapshot.

Run a passing baseline first. Every mutation must compile, fail its named
regression assertions, and restore the original source bytes even on failure.
This runner does not generate native evidence or install anything.
"""
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
TOOLS = ROOT / "crates/konnect-core/src/tools"
COMMAND = ["cargo", "test", "-p", "konnect-core", "--locked", "--lib", "a6_",
           "--", "--test-threads=1"]
ANGLE = "a6_raw_angles_and_backside_transforms_have_upright_controls"
REFRESH = "a6_served_refresh_raw_angle_and_unsupported_property_no_write"
ADD = "a6_served_addition_has_library_fields_and_raw_180"
READER = "a6_properties_keep_native_presentation_and_upright_is_not_a_lock"
READBACK = "a6_served_readback_detects_independent_field_losses"
FALLBACK = "a6_unreadable_even_hidden_properties_apply_with_diagnostic"
DRAWING = "a6_visible_property_is_not_drawn_twice_and_fallback_still_draws"
CONTROLS = {
    "fold-non-upright": ("pcb_footprint_update.rs",
        "if attributes.keep_upright {", "if true {", {ANGLE, REFRESH, ADD}),
    "never-fold-upright": ("pcb_footprint_update.rs",
        "if attributes.keep_upright {", "if false {", {ANGLE}),
    "upright-always-off": ("library_footprint.rs",
        "attributes.keep_upright = !unlocked.unwrap_or(false);",
        "attributes.keep_upright = false;", {READER, ADD, READBACK}),
    "fields-locked": ("library_footprint.rs",
        "locked: LockedState::LsUnlocked as i32,",
        "locked: LockedState::LsLocked as i32,", {READER}),
    "skip-library-fields": ("pcb_sync.rs",
        "apply_library_fields(footprint, fields)?;",
        "let _ = fields;", {ADD, READBACK}),
    "draw-properties-twice": ("pcb_sync.rs",
        "super::pcb_components::extract_graphic_definitions_without_properties(source)",
        "super::pcb_components::extract_graphic_definitions(source)", {DRAWING}),
    "fallback-diagnostic-missing": ("pcb_sync.rs",
        '"library_fields_not_carried",', '"lost_diagnostic",', {FALLBACK}),
    "readback-ignores-angle": ("pcb_sync.rs",
        "&& self.angle == other.angle", "", {READBACK}),
    "readback-ignores-thickness": ("pcb_sync.rs",
        "&& self.thickness_nm == other.thickness_nm", "", {READBACK}),
    "thickness-default-presence": ("pcb_sync.rs",
        ".map_or(0, |width| width.value_nm)",
        ".map_or(-1, |width| width.value_nm)",
        {"a6_canonicalized_fallback_readback_is_not_a_field_loss"}),
    "angle-default-presence": ("pcb_sync.rs",
        ".map_or(0.0, |angle| angle.value_degrees.rem_euclid(360.0))",
        ".map_or(-1.0, |angle| angle.value_degrees.rem_euclid(360.0))",
        {"a6_canonicalized_fallback_readback_is_not_a_field_loss"}),
    "apply-loses-staging": ("pcb_sync.rs",
        '"staging": plan.staging.as_ref().map(StagingEvidence::json),',
        '"staging": if applied { None } else { plan.staging.as_ref().map(StagingEvidence::json) },',
        {"a6_apply_retains_dry_run_staging"}),
}


def run():
    result = subprocess.run(COMMAND, cwd=ROOT, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            env={**os.environ, "CARGO_TERM_COLOR": "never"})
    print(result.stdout, end="", flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("control", choices=[*CONTROLS, "all"])
    args = parser.parse_args()
    if run().returncode:
        raise RuntimeError("passing-after prerequisite failed; no mutation applied")
    names = CONTROLS if args.control == "all" else [args.control]
    for name in names:
        file, old, new, expected = CONTROLS[name]
        path = TOOLS / file
        original = path.read_bytes()
        source = original.decode()
        if source.count(old) != 1:
            raise RuntimeError(f"{name}: control target must occur exactly once")
        print(f"A6 CONTROL: {name}", flush=True)
        try:
            path.write_text(source.replace(old, new, 1))
            result = run()
            failures = {m.split("::")[-1] for m in re.findall(
                r"^test (\S+) \.\.\. FAILED$", result.stdout, re.M)}
            if (result.returncode != 101 or not expected <= failures
                    or "could not compile" in result.stdout
                    or re.search(r"error\[E\d+\]", result.stdout)):
                raise RuntimeError(
                    f"{name}: expected assertion failures {sorted(expected)}, "
                    f"got exit={result.returncode}, failed={sorted(failures)}")
        finally:
            path.write_bytes(original)
        if path.read_bytes() != original:
            raise RuntimeError(f"{name}: restoration failed")
        print(f"REJECTED AS EXPECTED: {name}; exact source restored", flush=True)
    if run().returncode:
        raise RuntimeError("restored passing-after gate failed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
