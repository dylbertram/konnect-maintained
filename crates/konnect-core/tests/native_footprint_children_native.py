#!/usr/bin/env python3
"""Coordinator-only A8 native gate. Requires a fresh disposable KiCad 10.0.8 worker.

Use the unchanged pcb_export_supervisor.py at 60 seconds plus an outer
coordinator deadline. This helper is source only until admitted and executed.
It never uses global Konnect, a shared editor or text-authored CAD.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import traceback

import library_field_native as lifecycle

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "crates/konnect-core/tests/fixtures"
U1 = "c7901245-d0c4-476f-8215-85f1183d04de"
R1 = "20a44d6b-d6bb-4ec1-ac0c-f1d3d7e20db2"
MUTATIONS = {"BeginCommit", "EndCommit", "CreateItems", "UpdateItems", "DeleteItems",
             "SaveDocument", "RevertDocument"}


class OwnedProxy:
    """NativeRun closes the editor first, then these owned leaders, then audits."""
    def __init__(self, api, proxy):
        self.api, self.proxy = api, proxy

    @property
    def process(self):
        return self.api.process

    def close(self):
        try:
            self.api.close()
        finally:
            lifecycle.stop_editor(self.proxy)  # signals only our registered leader


def isolate(control):
    home = control / "home"
    versioned = home / ".config/kicad/10.0"
    versioned.mkdir(parents=True)
    (versioned / "kicad_common.json").write_text(json.dumps({
        "meta": {"version": 6}, "api": {"enable_server": True},
        "graphics": {"canvas_type": 2},
        "do_not_show_again": {"data_collection_prompt": True, "update_check_prompt": True},
    }))
    for name in ["sym-lib-table", "fp-lib-table"]:
        shutil.copy(Path("/usr/share/kicad/template") / name, versioned / name)
    template = Path("/usr/share/kicad/template/design-block-lib-table")
    if template.is_file():
        shutil.copy(template, versioned / template.name)
    else:
        (versioned / template.name).write_text("(design_block_lib_table\n  (version 7)\n)\n")
    runtime = control / "runtime"
    runtime.mkdir(mode=0o700)
    unavailable_bus = runtime / "no-session-bus"
    assert not unavailable_bus.exists()
    environment = {**os.environ, "HOME": str(home),
                   "XDG_CONFIG_HOME": str(home / ".config"),
                   "XDG_DATA_HOME": str(home / ".local/share"),
                   "XDG_CACHE_HOME": str(home / ".cache"),
                   "XDG_RUNTIME_DIR": str(runtime),
                   "KICAD_CONFIG_HOME": str(home / ".config/kicad"), "TMPDIR": str(control),
                   "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8",
                   "GIO_USE_VFS": "local", "GSETTINGS_BACKEND": "memory",
                   "GTK_USE_PORTAL": "0", "NO_AT_BRIDGE": "1",
                   "DBUS_SESSION_BUS_ADDRESS": f"unix:path={unavailable_bus}",
                   "LIBGL_ALWAYS_SOFTWARE": "1"}
    for key in ["DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE",
                "KICAD_API_TOKEN", "KICAD_API_SOCKET"]:
        environment.pop(key, None)
        os.environ.pop(key, None)
    os.environ.update(environment)
    return environment


def capture(binary, socket, board, out, environment):
    subprocess.run([str(binary), "capture", socket, str(board), str(out)],
                   env=environment, check=True, timeout=15)
    return json.loads((out / "capture.json").read_text())


def native_owned(capture_result, kiid):
    return next(item for item in capture_result["native_children"] if item["kiid"] == kiid)


def raw_item(capture_result, kiid):
    return next(item for item in capture_result["raw_items"] if item["kiid"] == kiid)


def assert_owned(result):
    owned = native_owned(result, U1)
    assert (owned["group"], owned["point"], owned["variant"]) == (1, 1, 1), owned
    assert owned["variant_names"] == ["NoTimer"], owned
    safe = native_owned(result, R1)
    assert (safe["group"], safe["point"], safe["variant"]) == (0, 0, 0), safe


def rpc_requests(path):
    return [row for row in map(json.loads, path.read_text().splitlines())
            if row["phase"] == "request"]


def assert_raw_equal(before, after, before_dir, after_dir):
    assert (before_dir / "native-board.txt").read_bytes() == (after_dir / "native-board.txt").read_bytes()
    assert before["native_children"] == after["native_children"]
    for item in before["raw_items"]:
        other = raw_item(after, item["kiid"])
        assert (before_dir / f"item-{item['index']}.any.value").read_bytes() == (
            after_dir / f"item-{other['index']}.any.value").read_bytes()


def assert_untouched_raw(before, after, before_dir, after_dir, kiid):
    old, new = raw_item(before, kiid), raw_item(after, kiid)
    assert (before_dir / f"item-{old['index']}.any.value").read_bytes() == (
        after_dir / f"item-{new['index']}.any.value").read_bytes()


def control_build(args, binary):
    """Require the isolated build runner's restored-positive provenance."""
    evidence = json.loads(args.control_build_evidence.read_text())
    assert evidence["status"] == "passed", evidence
    assert evidence["source_revision"] == args.source_revision, evidence
    source = (ROOT / "crates/konnect-core/src/tools/pcb_sync.rs").read_bytes()
    assert hashlib.sha256(source).hexdigest() == evidence["original_source_sha256"], evidence
    old = b"&targets, // A8 sync inventory targets"
    new = b"&[], // A8 sync inventory targets"
    assert source.count(old) == 1
    assert hashlib.sha256(source.replace(old, new, 1)).hexdigest() == evidence["control_source_sha256"]
    control = args.control_konnect.resolve(strict=True)
    assert control != binary
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == evidence["restored_binary_sha256"]
    assert hashlib.sha256(control.read_bytes()).hexdigest() == evidence["control_binary_sha256"]
    assert evidence["exact_restoration"] and evidence["restored_positive_gate"], evidence
    return control, evidence


def sync_loss_control(api, pcbnew, raw_binary, socket, board, schematic,
                      out, environment, before, before_dir, proxy_log, report):
    """Actual maintained sync only. Never inject or re-encode a replacement."""
    report["unsafe_loss_control"] = {
        "status": "incomplete", "actual_replacement_proven": False,
        "zero_mutation_claim": False,
    }
    evidence = report["unsafe_loss_control"]
    # Leave the native-prepared board untouched. Only U1 belongs to this public
    # scratch schematic; R1/C1 remain board-only, not replacement targets.
    for ref in ["R1", "C1"]:
        api.call("delete_schematic_component", {"schematic": str(schematic), "reference": ref})
    saved_before = pcbnew.LoadBoard(str(board))
    originals = [fp for fp in saved_before.GetFootprints() if fp.m_Uuid.AsString() == U1]
    assert len(originals) == 1 and originals[0].GetReference() == "U1"
    old_value = originals[0].GetValue()
    desired = "a8 desired value"
    assert old_value != desired, old_value
    evidence["disk_value_before"] = old_value
    start = len(rpc_requests(proxy_log / "rpc.jsonl"))
    try:
        arguments = {"schematic": str(schematic), "board": str(board)}
        plan = api.call("update_pcb_from_schematic", {**arguments, "dry_run": True})
        evidence["plan"] = plan
        assert plan["status"] == "ready", plan
        assert len(plan["changes"]) == 1, plan
        change = plan["changes"][0]
        assert (change["kind"], change["reference"], change["kiid"], change["value"]) == (
            "update", "U1", U1, desired), change
        assert plan["coverage"]["footprints_updated"] == {"planned": 1, "applied": 0}, plan
        assert plan["coverage"]["footprints_added"]["planned"] == 0, plan
        applied = api.call("update_pcb_from_schematic", {
            **arguments, "dry_run": False, "expected_plan_revision": plan["plan_revision"]})
        evidence["applied"] = applied
        assert applied["status"] == "applied", applied
        assert applied["changes"] == plan["changes"], applied
        assert applied["coverage"]["footprints_updated"] == {"planned": 1, "applied": 1}, applied
        assert applied["coverage"]["footprints_added"]["applied"] == 0, applied
        api.call("save_project", {})
        after_dir = out / "after-control-sync"
        after = capture(raw_binary, socket, board, after_dir, environment)
        shutil.copyfile(board, out / "after-control-sync.kicad_pcb")
        saved = pcbnew.LoadBoard(str(board))
        actual = [fp for fp in saved.GetFootprints() if fp.m_Uuid.AsString() == U1]
        assert len(actual) == 1 and actual[0].GetReference() == "U1"
        evidence["disk_value_after"] = actual[0].GetValue()
        assert actual[0].GetValue() == desired, evidence
        old_raw, new_raw = raw_item(before, U1), raw_item(after, U1)
        assert (before_dir / f"item-{old_raw['index']}.any.value").read_bytes() != (
            after_dir / f"item-{new_raw['index']}.any.value").read_bytes()
        old, new = native_owned(before, U1), native_owned(after, U1)
        evidence["native_before"] = old
        evidence["native_after"] = new
        evidence["raw_before"] = old_raw
        evidence["raw_after"] = new_raw
        requests = rpc_requests(proxy_log / "rpc.jsonl")[start:]
        counts = {}
        for row in requests:
            name = row["command"].rsplit(".", 1)[-1]
            counts[name] = counts.get(name, 0) + 1
            assert (proxy_log / f"{row['index']:05}.response.pb").is_file(), row
        evidence["rpc_counts"] = counts
        assert counts.get("UpdateItems") == 1, counts
        assert counts.get("BeginCommit") == counts.get("EndCommit") == 1, counts
        assert counts.get("SaveDocument", 0) >= 1 and not counts.get("RevertDocument"), counts
        assert not counts.get("CreateItems") and not counts.get("DeleteItems"), counts
        evidence["actual_replacement_proven"] = True
        evidence["kinds"] = {
            kind: {"before": old[kind], "after": new[kind], "lost": new[kind] < old[kind]}
            for kind in ["group", "point", "variant"]
        }
        evidence["variant_names_before"] = old["variant_names"]
        evidence["variant_names_after"] = new["variant_names"]
        evidence["kinds"]["variant"]["lost"] = (
            new["variant"] < old["variant"] or "NoTimer" not in new["variant_names"])
        assert evidence["kinds"]["group"]["lost"] or evidence["kinds"]["variant"]["lost"], (
            "Actual U1 replacement produced no group or variant loss; reproduction failed", evidence)
        evidence["status"] = "reproduced"
    except BaseException as error:
        evidence.update(status="failed", error=repr(error))
        raise
    finally:
        requests = rpc_requests(proxy_log / "rpc.jsonl")[start:]
        evidence["rpc_counts"] = {}
        for row in requests:
            name = row["command"].rsplit(".", 1)[-1]
            evidence["rpc_counts"][name] = evidence["rpc_counts"].get(name, 0) + 1
        (out / "control-rpcs.json").write_text(json.dumps(requests, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disposable-worker", action="store_true", required=True)
    parser.add_argument("--konnect", type=Path, required=True)
    parser.add_argument("--capture-helper", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--mode", choices=["refusal", "sync-loss-control"], default="refusal")
    parser.add_argument("--control-konnect", type=Path)
    parser.add_argument("--control-build-evidence", type=Path)
    args = parser.parse_args()
    if args.mode == "sync-loss-control":
        if not args.control_konnect or not args.control_build_evidence:
            parser.error("sync-loss-control requires --control-konnect and --control-build-evidence")
    elif args.control_konnect or args.control_build_evidence:
        parser.error("control arguments require --mode sync-loss-control")
    assert sys.platform == "linux" and os.environ.get("DISPLAY")
    binary = args.konnect.resolve(strict=True)
    raw_binary = args.capture_helper.resolve(strict=True)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {"source_revision": args.source_revision, "status": "incomplete",
              "scope": "bounded native-owned child refusal, not universal metadata preservation",
              "unsafe_loss_control": "not run; no native data-loss assertion is made"}
    report["mode"] = args.mode
    served_binary = binary
    api = None
    primary = None
    try:
        if args.mode == "sync-loss-control":
            served_binary, report["control_build"] = control_build(args, binary)
            report["scope"] = "actual native U1 loss control, not refusal or zero-mutation proof"
        fixture = FIXTURES / "footprint_children_kicad10.kicad_pcb"
        assert hashlib.sha256(fixture.read_bytes()).hexdigest() == (
            "44a89de8509f6aade944ee7c4f100960672b6a34c5cfb1aef1997ad9989b73a3")
        with lifecycle.NativeRun(out, report) as run:
            control = Path(run.temporary)
            environment = isolate(control)
            report["desktop_isolation"] = "set before pcbnew import and before owned editor launch"
            import pcbnew
            assert str(pcbnew.Version()).startswith("10.0.8"), pcbnew.Version()
            cli = shutil.which("kicad-cli")
            assert cli
            cli_version = subprocess.check_output([cli, "version"], env=environment,
                                                  text=True, timeout=3).strip()
            assert cli_version.startswith("10.0.8"), cli_version
            report.update(pcbnew=pcbnew.Version(), cli=cli_version)
            upstream = f"ipc://{control}/kicad/api.sock"
            config = control / "server.json"
            config.write_text(json.dumps({"kicad_cli": cli, "ipc_address": upstream,
                                          "eager_toolsets": True, "log_level": "warn"}))
            api = lifecycle.Api(binary, config, environment, out / "setup-konnect.log")
            run.api = api
            project = control / "project"
            api.call("create_project", {"path": str(project), "name": "a8"})
            schematic = project / "a8.kicad_sch"
            board = project / "a8.kicad_pcb"
            # Import the original bytes; all accepted-copy changes below use
            # pcbnew's native model and serializer, never text edits.
            shutil.copyfile(fixture, board)
            shutil.copyfile(board, out / "original-import.kicad_pcb")
            writer = pcbnew.PCB_IO_KICAD_SEXPR()
            library = project / "A8.pretty"
            writer.CreateLibrary(str(library))
            library_controls = []
            for name, directory, entry in [
                ("SafeDIP", "Package_DIP.pretty", "DIP-8_W7.62mm"),
                ("SafeR", "Resistor_SMD.pretty", "R_0603_1608Metric"),
            ]:
                fp = pcbnew.FootprintLoad(str(Path("/usr/share/kicad/footprints") / directory), entry)
                assert fp is not None, entry
                removed = []
                # The stock DIP library itself owns a point. Remove it in a
                # native library copy so library-input policy cannot mask the
                # placed-instance gate. Never remove children from placed U1.
                for drawing in list(fp.GraphicalItems()):
                    if "POINT" in drawing.GetClass().upper():
                        removed.append(drawing.GetClass())
                        fp.Remove(drawing)
                fp.SetFPID(pcbnew.LIB_ID("A8", name))
                fp.SetDescription("a8 native library replacement")
                writer.FootprintSave(str(library), fp)
                library_controls.append({"name": name, "removed_native_library_points": removed})
            report["native_library_controls"] = library_controls
            api.call("register_footprint_library", {
                "library_path": str(library), "nickname": "A8",
                "project": str(project / "a8.kicad_pro"),
            })
            api.call("batch_place_components", {"schematic": str(schematic), "components": [
                {"lib_id": symbol, "reference": ref, "x": x, "y": 50.8,
                 "footprint": f"A8:{name}", "value": "a8 desired value"}
                for symbol, ref, name, x in [
                    ("Device:R", "R1", "SafeR", 50.8),
                    ("Device:R", "C1", "SafeR", 76.2),
                    ("Timer:NE555P", "U1", "SafeDIP", 101.6),
                ]
            ]})
            native = pcbnew.LoadBoard(str(board))
            for fp in native.GetFootprints():
                fp.SetFPID(pcbnew.LIB_ID("A8", "SafeDIP" if fp.GetReference() == "U1" else "SafeR"))
                # An explicitly unlinked native instance allows the sync's
                # existing reference fallback to link the public new schematic.
                fp.SetPath(pcbnew.KIID_PATH())
            # This local import happened after desktop isolation. The reused
            # lifecycle module intentionally has no module-global pcbnew when
            # imported, so do not call its standalone save_native helper.
            assert pcbnew.SaveBoard(str(board), native), f"native save failed: {board}"
            assert board.is_file() and board.stat().st_size > 0, board
            shutil.copyfile(board, out / "native-prepared.kicad_pcb")
            lifecycle.open_editor(board, environment, api, out / "editor.log", run)

            # Setup is over. Every served guard call and capture below uses
            # the instrumented proxy to the SAME owned editor/socket/document.
            proxy_socket = f"ipc://{control}/a8-proxy.sock"
            proxy_log = out / "ipc"
            with (out / "proxy.log").open("w") as log:
                proxy = subprocess.Popen([str(raw_binary), "proxy", proxy_socket, upstream,
                                          str(proxy_log)], env=environment, stdout=log, stderr=log)
            owner = OwnedProxy(api, proxy)
            run.api = owner
            deadline = time.monotonic() + 3
            while not (proxy_log / "ready").exists():
                assert proxy.poll() is None, (out / "proxy.log").read_text()
                if time.monotonic() > deadline:
                    raise TimeoutError("owned raw IPC proxy did not start")
                time.sleep(0.05)
            api.close()
            (out / "setup-served-responses.json").write_text(json.dumps(api.responses, indent=2))
            config.write_text(json.dumps({"kicad_cli": cli, "ipc_address": proxy_socket,
                                          "eager_toolsets": True, "log_level": "warn"}))
            api = lifecycle.Api(served_binary, config, environment, out / "guard-konnect.log")
            owner.api = api
            run.checkpoint("instrumented_native_gate")
            before_dir = out / ("before-control" if args.mode == "sync-loss-control" else "before-refusal")
            after_dir = out / "after-refusal"
            before = capture(raw_binary, proxy_socket, board, before_dir, environment)
            assert_owned(before)
            u1raw = raw_item(before, U1)
            # Report separate observations, never infer all loss from one.
            assert u1raw["group_any_count"] == 0, u1raw
            assert len(u1raw["native_group_kiid_raw_presence"]) == 1, u1raw
            assert not u1raw["native_group_kiid_raw_presence"][0]["utf8_in_raw_value"], u1raw
            assert u1raw["point_any_count"] > 0, u1raw
            if args.mode == "sync-loss-control":
                assert any(url.rsplit("/", 1)[-1] == "kiapi.board.types.ReferencePoint"
                           for url in u1raw["definition_child_type_urls"]), u1raw
            assert u1raw["variant_field_20_occurrences"] > 0, u1raw
            assert len(u1raw["raw_variants"]) == u1raw["variant_field_20_occurrences"], u1raw
            assert [variant["decoded"]["name"] for variant in u1raw["raw_variants"]] == ["NoTimer"], u1raw
            assert u1raw["raw_variants"][0]["decoded"]["do_not_populate"] is True, u1raw
            report["serialization_observations"] = u1raw
            if args.mode == "sync-loss-control":
                sync_loss_control(api, pcbnew, raw_binary, proxy_socket, board, schematic,
                                  out, environment, before, before_dir, proxy_log, report)
                # NativeRun still closes/audits everything and the outer finally
                # persists evidence. The default acceptance gate below is unchanged.
                report["status"] = "passed"
                return
            disk_before = board.read_bytes()
            start = len(rpc_requests(proxy_log / "rpc.jsonl"))
            for tool in ["update_pcb_from_schematic", "update_footprints_from_library"]:
                arguments = {"board": str(board)}
                if tool == "update_pcb_from_schematic":
                    arguments["schematic"] = str(schematic)
                for dry_run in [True, False]:
                    response = api.call(tool, {
                        **arguments, "dry_run": dry_run,
                        **({} if dry_run else {"expected_plan_revision": "native-unsafe"}),
                    })
                    assert response["status"] == "conflict", response
                    diagnostic = next(d for d in response["diagnostics"]
                                      if d["code"] == "footprint_children_not_carried")
                    assert diagnostic["reference"] == "U1" and diagnostic["kiid"] == U1, diagnostic
                    assert diagnostic["library_id"] == "A8:SafeDIP", diagnostic
                    assert set(diagnostic["kinds"]) == {"group", "point", "variant"}, diagnostic
                    assert [diagnostic["native_children"][kind] for kind in
                            ["groups", "points", "variants"]] == [1, 1, 1], diagnostic
                    assert diagnostic["native_children"]["variant_names"] == ["NoTimer"], diagnostic
                    assert response["changes"] == [], response
            guarded = rpc_requests(proxy_log / "rpc.jsonl")[start:]
            assert guarded and not [
                row for row in guarded if row["command"].rsplit(".", 1)[-1] in MUTATIONS
            ], guarded
            (out / "guard-rpcs.json").write_text(json.dumps(guarded, indent=2))
            after = capture(raw_binary, proxy_socket, board, after_dir, environment)
            assert_raw_equal(before, after, before_dir, after_dir)
            assert board.read_bytes() == disk_before
            report["refusal_and_zero_mutation_proof"] = "passed"

            # Actual safe R1 refresh with native readback, despite board group
            # membership and unsafe U1 elsewhere on the board.
            plan = api.call("update_footprints_from_library", {"board": str(board), "references": ["R1"]})
            assert plan["status"] == "ready", plan
            applied = api.call("update_footprints_from_library", {
                "board": str(board), "references": ["R1"], "dry_run": False,
                "expected_plan_revision": plan["plan_revision"]})
            assert applied["status"] == "applied", applied
            positive = capture(raw_binary, proxy_socket, board, out / "after-safe-refresh", environment)
            assert_owned(positive)
            assert (out / "after-safe-refresh/native-board.txt").read_bytes() != (
                after_dir / "native-board.txt").read_bytes()
            assert native_owned(positive, U1) == native_owned(before, U1)
            assert_untouched_raw(before, positive, before_dir, out / "after-safe-refresh", U1)
            # Make a safe-only schematic with public edits, not text edits.
            for ref in ["U1", "C1"]:
                api.call("delete_schematic_component", {"schematic": str(schematic), "reference": ref})
            plan = api.call("update_pcb_from_schematic", {"schematic": str(schematic), "board": str(board)})
            assert plan["status"] == "ready", plan
            applied = api.call("update_pcb_from_schematic", {
                "schematic": str(schematic), "board": str(board), "dry_run": False,
                "expected_plan_revision": plan["plan_revision"]})
            assert applied["status"] == "applied", applied
            api.call("save_project", {})
            saved = pcbnew.LoadBoard(str(board))
            actual = {fp.GetReference(): fp for fp in saved.GetFootprints()}
            assert actual["R1"].GetValue() == "a8 desired value"
            assert actual["R1"].GetDescription() == "a8 native library replacement"
            shutil.copyfile(board, out / "after-safe-sync.kicad_pcb")
            final = capture(raw_binary, proxy_socket, board, out / "after-safe-sync", environment)
            assert_owned(final)
            assert_untouched_raw(before, final, before_dir, out / "after-safe-sync", U1)
            report["safe_native_readbacks"] = "passed"
        report["status"] = "passed"
    except BaseException as error:
        primary = error
        report.update(status="failed", error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        artifact_errors = []
        if api is not None:
            try:
                (out / "served-responses.json").write_text(json.dumps(api.responses, indent=2))
            except BaseException as error:
                artifact_errors.append({"artifact": "served-responses.json", "error": repr(error)})
        if artifact_errors:
            report.update(status="failed", artifact_errors=artifact_errors)
        try:
            (out / "native-footprint-children.json").write_text(json.dumps(report, indent=2))
        except BaseException as error:
            artifact_errors.append({"artifact": "native-footprint-children.json", "error": repr(error)})
        if artifact_errors:
            print(f"native evidence write errors: {artifact_errors}", file=sys.stderr)
            if primary is None:
                raise RuntimeError(f"native evidence write errors: {artifact_errors}")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
