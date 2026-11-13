#!/usr/bin/env python3
"""A6a Linux/KiCad 10 native acceptance, coordinator-admitted remote use only.

Requires --disposable-worker and an existing compiled Konnect binary. Run under
the existing pcb_export_supervisor.py with a 60-second group deadline and Xvfb.
No package installation, mock CLI, fabricated IPC, or live project is used.
All CAD is authored/saved by pcbnew or public Konnect tools in a new directory.
"""
import argparse
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import subprocess
import sys
import tempfile
import time

import pcbnew

ROOT = Path(__file__).resolve().parents[3]


def fields(footprint):
    result = {
        field.GetName(): {
            "text": field.GetText(),
            "angle": field.GetTextAngle().AsDegrees() % 360,
            "thickness_nm": field.GetTextThickness(),
            "layer": field.GetLayerName(),
            "visible": field.IsVisible(),
            "upright": field.IsKeepUpright(),
            "position_nm": [field.GetPosition().x, field.GetPosition().y],
        }
        for field in footprint.GetFields()
    }
    assert {"Reference", "Value", "Datasheet", "Description"} <= result.keys(), result
    return result


def inspect(path):
    board = pcbnew.LoadBoard(str(path))
    return {fp.GetReference(): fields(fp) for fp in board.GetFootprints()}


def save_native(path, board):
    assert pcbnew.SaveBoard(str(path), board), f"native save failed: {path}"
    assert path.is_file() and path.stat().st_size > 0, path


def ordinary_texts(path):
    board = pcbnew.LoadBoard(str(path))
    return {
        fp.GetReference(): sorted(
            item.GetText() for item in fp.GraphicalItems()
            if isinstance(item, pcbnew.PCB_TEXT)
            and not isinstance(item, pcbnew.PCB_FIELD)
        )
        for fp in board.GetFootprints()
    }


def compare(actual, expected):
    assert actual.keys() == expected.keys(), (actual.keys(), expected.keys())
    for ref, wanted in expected.items():
        assert actual[ref].keys() == wanted.keys(), (ref, actual[ref], wanted)
        for name, field in wanted.items():
            kept = actual[ref][name]
            for key in ["text", "thickness_nm", "layer", "visible", "upright"]:
                assert kept[key] == field[key], (ref, name, key, kept, field)
            # Full turns only. An upside-down field is not considered equal.
            difference = (kept["angle"] - field["angle"] + 180) % 360 - 180
            assert abs(difference) < 0.000001, (ref, name, kept, field)
            assert all(abs(a - b) <= 1 for a, b in zip(
                kept["position_nm"], field["position_nm"])), (ref, name, kept, field)


class Api:
    def __init__(self, binary, config, environment, log):
        self.log = log.open("w")
        self.process = subprocess.Popen(
            [str(binary), "--config", str(config)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
            env=environment,
        )
        self.sequence = 0
        self.responses = []
        self.buffer = b""

    def call(self, name, arguments):
        self.sequence += 1
        request = {"jsonrpc": "2.0", "id": self.sequence, "method": "tools/call",
                   "params": {"name": name, "arguments": arguments}}
        self.process.stdin.write((json.dumps(request) + "\n").encode())
        self.process.stdin.flush()
        deadline = time.monotonic() + 12
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while True:
                while b"\n" not in self.buffer:
                    if not selector.select(max(0, deadline - time.monotonic())):
                        raise TimeoutError(f"native served call exceeded deadline: {name}")
                    chunk = os.read(self.process.stdout.fileno(), 65536)
                    if not chunk:
                        raise RuntimeError(f"Konnect exited during {name}")
                    self.buffer += chunk
                line, self.buffer = self.buffer.split(b"\n", 1)
                response = json.loads(line)
                if response.get("id") != self.sequence:
                    assert "method" in response, response
                    continue
                self.responses.append({"request": request, "response": response})
                assert not response.get("error"), response
                result = response["result"]
                assert not result.get("isError"), result
                text = result["content"][0]["text"]
                if name == "save_project":
                    assert text == "Board saved successfully.", text
                    return text
                try:
                    return json.loads(text)
                except json.JSONDecodeError:
                    return text

    def close(self):
        try:
            self.process.stdin.close()
        except BrokenPipeError:
            pass
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.log.close()


def stop_editor(editor):
    if editor is not None and editor.poll() is None:
        # Only this test's disposable editor. save_project already verified its
        # native save; failure cleanup must not leave a modal editor running.
        editor.terminate()
        try:
            editor.wait(timeout=3)
        except subprocess.TimeoutExpired:
            editor.kill()
            editor.wait()


def open_editor(board, environment, api, log):
    socket = Path(environment["TMPDIR"]) / "kicad/api.sock"
    # Previous editor, if any, was stopped and reaped by this helper. Only this
    # disposable control directory can contain the stale endpoint.
    socket.unlink(missing_ok=True)
    output = log.open("w")
    editor = subprocess.Popen(["pcbnew", str(board)], env=environment,
                              stdout=output, stderr=output)
    output.close()
    try:
        deadline = time.monotonic() + 15
        while True:
            assert editor.poll() is None, log.read_text()
            if time.monotonic() >= deadline:
                raise TimeoutError("owned PCB editor was not ready for the requested board")
            if socket.exists():
                result = api.call("open_project", {"path": str(board)})
                if result["ipc_available"] and result["requested_open"] is True:
                    return editor
            time.sleep(0.1)
    except BaseException:
        stop_editor(editor)
        raise


def native_footprint(library, name, board, ref, value, position, rotation=0, back=False):
    fp = pcbnew.FootprintLoad(str(library), name)
    assert fp is not None, name
    fp.SetFPID(pcbnew.LIB_ID("A6", name))
    fp.SetReference(ref)
    fp.SetValue(value)
    board.Add(fp)
    fp.SetPosition(position)
    if back:
        fp.Flip(position, False)
    # A whole-footprint editor rotation, not the lower-level orientation setter.
    # FOOTPRINT::Rotate invokes KeepUpright on fields after SetOrientation;
    # SetOrientationDegrees alone deliberately leaves their raw angles alone.
    delta = rotation - fp.GetOrientationDegrees()
    fp.Rotate(position, pcbnew.EDA_ANGLE(delta, pcbnew.DEGREES_T))
    assert abs((fp.GetOrientationDegrees() - rotation + 180) % 360 - 180) < 0.000001
    return fp


def native_operation_control(library, out):
    """Capture Rotate vs raw setter before testing any production field result."""
    board = pcbnew.BOARD()
    origin = pcbnew.VECTOR2I(pcbnew.FromMM(40), pcbnew.FromMM(40))
    native_footprint(library, "Upright0", board, "R920", "native-rotate",
                     origin, rotation=180)
    setter = native_footprint(library, "Upright0", board, "R921", "native-setter",
                              origin)
    # Deliberately different native operation, retained as a negative control.
    # Its raw 180° fields must not become the whole-footprint Rotate oracle.
    setter.SetOrientationDegrees(180)
    native_footprint(library, "Raw0", board, "R922", "native-raw-rotate",
                     origin, rotation=180)
    native_footprint(library, "Raw180", board, "R923", "native-raw-local",
                     origin)
    path = out / "native-operation-control.kicad_pcb"
    save_native(path, board)
    observed = inspect(path)
    (out / "native-operation-control.json").write_text(json.dumps(observed, indent=2))
    for ref in ["R920", "R921", "R922", "R923"]:
        assert {"KiLib_Generator", "AssemblyVendor"} <= observed[ref].keys(), observed
    for name in observed["R920"]:
        assert observed["R920"][name]["upright"]
        assert observed["R920"][name]["angle"] == 0, ("native Rotate contract", name, observed)
        assert observed["R921"][name]["angle"] == 180, ("raw setter control", name, observed)
        for ref in ["R922", "R923"]:
            assert not observed[ref][name]["upright"]
            assert observed[ref][name]["angle"] == 180, ("native non-upright control", ref, name, observed)
    return observed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disposable-worker", action="store_true", required=True)
    parser.add_argument("--konnect", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args()
    assert sys.platform == "linux" and os.environ.get("DISPLAY")
    assert str(pcbnew.Version()).startswith("10."), pcbnew.Version()
    binary = args.konnect.resolve(strict=True)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = {"kicad": pcbnew.Version(), "source_revision": args.source_revision,
              "status": "incomplete",
              "scope": "field-only sync additions and supported scratch refresh",
              "rotation_operation": "FOOTPRINT::Rotate about its own anchor after optional Flip(anchor, False)"}
    api = None
    editor = None
    try:
        with tempfile.TemporaryDirectory(prefix="ka6-") as temporary:
            control = Path(temporary)
            home = control / "home"
            config_home = control / "config"
            versioned = config_home / "10.0"
            versioned.mkdir(parents=True)
            home.mkdir()
            (versioned / "kicad_common.json").write_text(json.dumps({
                "meta": {"version": 6}, "api": {"enable_server": True},
                "graphics": {"canvas_type": 2},
                "do_not_show_again": {"data_collection_prompt": True,
                                      "update_check_prompt": True},
            }))
            for name in ["sym-lib-table", "fp-lib-table"]:
                shutil.copy(Path("/usr/share/kicad/template") / name, versioned / name)
            shutil.copy(Path("/usr/share/kicad/template/design-block-lib-table"),
                        versioned / "design-block-lib-table")
            environment = {**os.environ, "HOME": str(home),
                           "XDG_CONFIG_HOME": str(home / ".config"),
                           "XDG_DATA_HOME": str(home / ".local/share"),
                           "KICAD_CONFIG_HOME": str(config_home), "TMPDIR": temporary,
                           "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8",
                           "LIBGL_ALWAYS_SOFTWARE": "1"}
            config = control / "server.json"
            config.write_text(json.dumps({
                "kicad_cli": shutil.which("kicad-cli"),
                "ipc_address": f"ipc://{temporary}/kicad/api.sock",
                "eager_toolsets": True, "log_level": "warn",
            }))
            assert json.loads(config.read_text())["kicad_cli"]
            api = Api(binary, config, environment, out / "konnect.log")
            project = out / "project"
            api.call("create_project", {"path": str(project), "name": "a6"})
            library = project / "A6.pretty"
            library.mkdir()
            # Use an existing pcbnew-authored footprint. Changes below go
            # through KiCad's model and FootprintSave, never S-expression edits.
            fixture_dir = ROOT / "crates/konnect-core/tests/fixtures"
            for name, angle, upright in [
                ("Raw180", 180, False), ("Raw0", 0, False),
                ("Upright0", 0, True), ("FallbackDuplicate", 0, True),
            ]:
                fp = pcbnew.FootprintLoad(str(fixture_dir), "socket_kicad10")
                assert fp is not None
                fp.SetFPID(pcbnew.LIB_ID("A6", name))
                fp.SetReference("REF**")
                fp.SetValue(name)
                for field in fp.GetFields():
                    field.SetKeepUpright(upright)
                    field.SetTextAngle(pcbnew.EDA_ANGLE(angle, pcbnew.DEGREES_T))
                    field.SetTextThickness(pcbnew.FromMM(0.19))
                    if field.GetName() == "AssemblyVendor":
                        field.SetVisible(True)
                        if name == "FallbackDuplicate":
                            # PCB_FIELD locks are not persisted by the field writer.
                            # Duplicate custom names are: it emits each field using
                            # its canonical name. Rename through KiCad's model only.
                            field.SetName("KiLib_Generator")
                if name == "FallbackDuplicate":
                    assert sum(f.GetName() == "KiLib_Generator" for f in fp.GetFields()) == 2
                else:
                    assert any(f.GetName() == "AssemblyVendor" for f in fp.GetFields())
                pcbnew.FootprintSave(str(library), fp)
                saved_library = library / f"{name}.kicad_mod"
                assert saved_library.is_file()
                if name == "FallbackDuplicate":
                    # Assert the actual saved precondition, not just model state.
                    # Never rewrite this native artifact to manufacture the case.
                    occurrences = len(re.findall(
                        r'\(\s*property\s+"KiLib_Generator"(?=\s|\))',
                        saved_library.read_text(),
                    ))
                    assert occurrences == 2, ("native duplicate-name precondition", occurrences)
                    precondition = {
                        "footprint_id": "A6:FallbackDuplicate",
                        "property_name": "KiLib_Generator",
                        "saved_property_occurrences": occurrences,
                    }
                    report["fallback_precondition"] = precondition
                    (out / "fallback-native-precondition.json").write_text(
                        json.dumps(precondition, indent=2)
                    )
            report["native_operation_control"] = native_operation_control(library, out)
            api.call("register_footprint_library", {
                "library_path": str(library), "nickname": "A6",
                "project": str(project / "a6.kicad_pro"),
            })
            schematic = project / "a6.kicad_sch"
            api.call("batch_place_components", {"schematic": str(schematic), "components": [
                {"lib_id": "Device:R", "reference": ref, "x": x, "y": 50.8,
                 "footprint": f"A6:{name}", "value": "native-a6"}
                for ref, name, x in [
                    ("R901", "Raw180", 50.8), ("R902", "Upright0", 76.2),
                    ("R903", "FallbackDuplicate", 101.6),
                ]
            ]})
            board = project / "a6.kicad_pcb"
            save_native(board, pcbnew.BOARD())
            editor = open_editor(board, environment, api, out / "sync-editor.log")
            plan = api.call("update_pcb_from_schematic", {
                "schematic": str(schematic), "board": str(board)})
            assert plan["status"] == "ready", plan
            applied = api.call("update_pcb_from_schematic", {
                "schematic": str(schematic), "board": str(board),
                "dry_run": False, "expected_plan_revision": plan["plan_revision"]})
            assert applied["status"] == "applied", applied
            assert len(applied["diagnostics"]) == 1, applied
            diagnostic = applied["diagnostics"][0]
            assert diagnostic["code"] == "library_fields_not_carried", applied
            assert diagnostic["references"] == ["R903"], applied
            assert diagnostic["footprint_id"] == "A6:FallbackDuplicate", applied
            assert "KiLib_Generator" in diagnostic["message"], applied
            assert "appears more than once" in diagnostic["message"], applied
            api.call("save_project", {})
            stop_editor(editor)
            editor = None
            shutil.copy(board, out / "sync-native-saved.kicad_pcb")
            synced = pcbnew.LoadBoard(str(board))
            placed = {fp.GetReference(): fp for fp in synced.GetFootprints()}
            assert placed.keys() == {"R901", "R902", "R903"}, placed.keys()
            oracle = pcbnew.BOARD()
            for ref, name in [("R901", "Raw180"), ("R902", "Upright0")]:
                native_footprint(library, name, oracle, ref, "native-a6",
                                 placed[ref].GetPosition())
            save_native(out / "sync-native-control.kicad_pcb", oracle)
            compare({ref: field for ref, field in inspect(board).items() if ref != "R903"},
                    inspect(out / "sync-native-control.kicad_pcb"))
            assert {ref: text for ref, text in ordinary_texts(board).items() if ref != "R903"} == ordinary_texts(out / "sync-native-control.kicad_pcb")
            fallback = fields(placed["R903"])
            assert fallback["Reference"]["text"] == "R903", fallback
            assert fallback["Value"]["text"] == "native-a6", fallback
            assert "KiLib_Generator" not in fallback, fallback
            (out / "fallback-native-fields.json").write_text(json.dumps(fallback, indent=2))
            # Native placement, rotation and flipping are the refresh oracle.
            refresh = pcbnew.BOARD()
            refs = []
            for index, (name, rotation, back) in enumerate([
                ("Raw180", 0, False), ("Raw0", 180, False),
                ("Raw0", 0, True), ("Raw180", 180, True),
                ("Upright0", 180, False), ("Upright0", 180, True),
            ]):
                ref = f"R{910 + index}"
                refs.append(ref)
                native_footprint(library, name, refresh, ref, "native-control",
                                 pcbnew.VECTOR2I(pcbnew.FromMM(30 + index * 10),
                                                pcbnew.FromMM(40)), rotation, back)
            save_native(out / "refresh-native-control.kicad_pcb", refresh)
            expected = inspect(out / "refresh-native-control.kicad_pcb")
            for fp in refresh.GetFootprints():
                for field in fp.GetFields():
                    if field.GetName() == "AssemblyVendor":
                        field.SetText("damaged-before-refresh")
            save_native(board, refresh)
            shutil.copy(board, out / "refresh-native-before.kicad_pcb")
            editor = open_editor(board, environment, api, out / "refresh-editor.log")
            plan = api.call("update_footprints_from_library", {"board": str(board), "references": refs})
            assert plan["status"] == "ready", plan
            applied = api.call("update_footprints_from_library", {
                "board": str(board), "references": refs, "dry_run": False,
                "expected_plan_revision": plan["plan_revision"]})
            assert applied["status"] == "applied", applied
            api.call("save_project", {})
            stop_editor(editor)
            editor = None
            shutil.copy(board, out / "refresh-native-saved.kicad_pcb")
            actual = inspect(board)
            compare(actual, expected)
            assert ordinary_texts(board) == ordinary_texts(out / "refresh-native-control.kicad_pcb")
            report.update(status="passed", sync_fields=inspect(out / "sync-native-saved.kicad_pcb"),
                          refresh_fields=actual, refresh_expected=expected)
    except BaseException as error:
        report.update(status="failed", error=repr(error))
        raise
    finally:
        stop_editor(editor)
        if api is not None:
            (out / "served-responses.json").write_text(json.dumps(api.responses, indent=2))
            api.close()
        (out / "native-fields.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
