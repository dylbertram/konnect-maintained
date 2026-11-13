#!/usr/bin/env python3
"""Coordinator-only Linux/KiCad 10.0.8 rename acceptance, source only until run.

Use the UNCHANGED pcb_export_supervisor.py with a 60-second child budget and
Xvfb. No installs, Git, shared editors, authored CAD or native save/upgrade.
Schematic and PCB fixtures are independent: no sch/PCB parity claim is made.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import time
import traceback
import xml.etree.ElementTree as ET

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "crates/konnect-core/tests/fixtures"
OLD = "multichannel_mixer"
NEW = "guarded_rename"
CHILD = "multichannel_channel_strip.kicad_sch"
FILES = {
    f"{OLD}.kicad_pro": f"{OLD}.kicad_pro",
    f"{OLD}.kicad_sch": f"{OLD}.kicad_sch",
    CHILD: CHILD,
    f"{OLD}.kicad_pcb": "specctra_two_resistors.kicad_pcb",
}
MAX_FILE = 16 * 1024 * 1024
MAX_TREE = 64 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def source_manifest():
    pins = json.loads((ROOT / "crates/konnect-core/tests/project_rename_source_pins.json").read_text())
    result = {}
    for name, digest in pins.items():
        actual = sha((ROOT / name).read_bytes())
        require(actual == digest, f"reviewed source pin mismatch: {name}")
        result[name] = actual
    for name in [
        "crates/konnect-core/tests/project_rename_source_pins.json",
        "crates/konnect-core/tests/project_rename_native.py",
        "crates/konnect-core/tests/library_field_native.py",
        "crates/konnect-core/tests/native_footprint_children_native.py",
        "crates/konnect-core/tests/support/pcb_export_supervisor.py",
        "crates/konnect-ipc/examples/native_footprint_children_capture.rs",
    ]:
        result[name] = sha((ROOT / name).read_bytes())
    return result


def tree_manifest(root):
    """Bounded, no-follow inventory. File timestamps are not a byte oracle."""
    result = {}
    total = 0
    require(root.is_dir() and not root.is_symlink(), f"not an owned directory: {root}")
    entries = []
    for directory, children, files in os.walk(root, followlinks=False):
        for name in children + files:
            path = Path(directory) / name
            require(not path.is_symlink(), f"refusing to follow project symlink: {path}")
            entries.append(path)
            require(len(entries) <= 128, "unexpectedly large disposable project tree")
    for path in sorted(entries):
        mode = path.lstat().st_mode
        name = path.relative_to(root).as_posix()
        require(not stat.S_ISLNK(mode), f"refusing to follow project symlink: {path}")
        if stat.S_ISDIR(mode):
            result[name] = {"kind": "directory"}
        else:
            require(stat.S_ISREG(mode), f"nonregular project entry: {path}")
            require(path.stat().st_size <= MAX_FILE, f"project file exceeds bound: {path}")
            data = path.read_bytes()
            total += len(data)
            require(total <= MAX_TREE, "project tree exceeds bound")
            result[name] = {"kind": "file", "bytes": len(data), "sha256": sha(data)}
    return result


def archive_tree(source, destination):
    manifest = tree_manifest(source)
    destination.mkdir(parents=True, exist_ok=False)
    for name, entry in manifest.items():
        target = destination / name
        if entry["kind"] == "directory":
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / name, target)
    require(tree_manifest(destination) == manifest, f"artifact copy differs: {destination}")
    write_json(destination.with_name(destination.name + ".manifest.json"), manifest)
    return manifest


def project_copy(parent):
    project = parent / OLD
    project.mkdir(parents=True)
    for target, source in FILES.items():
        shutil.copyfile(FIXTURES / source, project / target)
        require((project / target).read_bytes() == (FIXTURES / source).read_bytes(),
                f"fixture copy differs: {target}")
    return project


def cli_run(command, environment, out, label):
    """Every CLI leader has a 12-second budget; outer supervisor owns its group."""
    audit = {"command": [str(value) for value in command], "timeout_seconds": 12,
             "timed_out": False}
    try:
        result = subprocess.run(command, env=environment, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=12)
        (out / f"{label}.stdout").write_bytes(result.stdout)
        (out / f"{label}.stderr").write_bytes(result.stderr)
        audit["status"] = result.returncode
        require(result.returncode == 0, f"native CLI failed: {label}, status {result.returncode}")
        return result.stdout.decode("utf-8", errors="strict").strip()
    except subprocess.TimeoutExpired as error:
        audit["timed_out"] = True
        (out / f"{label}.stdout").write_bytes(error.stdout or b"")
        (out / f"{label}.stderr").write_bytes(error.stderr or b"")
        raise
    finally:
        write_json(out / f"{label}.audit.json", audit)


def canonical_netlist(path):
    require(path.is_file() and 0 < path.stat().st_size <= MAX_FILE, "missing/oversize XML netlist")
    data = path.read_bytes()
    require(b"<!DOCTYPE" not in data and b"<!ENTITY" not in data, "unexpected XML declarations")
    root = ET.fromstring(data)
    require(root.tag == "export", "native output is not a KiCad XML netlist")
    components = {}
    for item in root.findall("./components/comp"):
        reference = item.get("ref")
        stamps = (item.findtext("tstamps") or "").split()
        library = item.find("libsource")
        sheet = item.find("sheetpath")
        require(reference and "?" not in reference and reference not in components,
                f"missing, duplicate or unannotated native component: {reference}")
        require(stamps and library is not None and sheet is not None,
                f"missing native component identity: {reference}")
        components[reference] = {
            "uuids": sorted(stamps), "sheet_uuids": sheet.get("tstamps"),
            "value": item.findtext("value") or "",
            "footprint": item.findtext("footprint") or "",
            "library": library.get("lib"), "part": library.get("part"),
        }
    require(bool(components), "native netlist contains zero components")
    nets = []
    for net in root.findall("./nets/net"):
        name = net.get("name")
        require(name is not None, "native net has no name")
        nodes = []
        for node in net.findall("node"):
            reference, pin = node.get("ref"), node.get("pin")
            require(reference in components and pin, "native net node has no known component/pin")
            nodes.append([reference, pin, node.get("pinfunction", ""), node.get("pintype", "")])
        nets.append({"name": name, "nodes": sorted(nodes)})
    require(bool(nets), "native netlist contains zero nets")
    # Codes, design source paths, generator dates and export ordering are not
    # electrical identity. References, UUIDs, named nets and pin memberships are.
    return {"components": components,
            "nets": sorted(nets, key=lambda net: (net["name"], net["nodes"]))}


def export_netlist(cli, project, schematic, environment, out, label):
    # Export only retained read copies, not a live editor's project.
    before = tree_manifest(project)
    output = out / f"{label}.xml"
    cli_run([cli, "sch", "export", "netlist", "--format", "kicadxml",
             "--output", str(output), str(schematic)], environment, out, label)
    require(tree_manifest(project) == before, f"CLI changed its input read copy: {label}")
    canonical = canonical_netlist(output)
    write_json(out / f"{label}.canonical.json", canonical)
    return canonical


def board_identity(pcbnew, path):
    board = pcbnew.LoadBoard(str(path))
    require(board is not None, f"native PCB load failed: {path}")
    footprints = []
    for footprint in board.GetFootprints():
        pads = []
        for pad in footprint.Pads():
            pads.append({
                "uuid": pad.m_Uuid.AsString(), "number": pad.GetNumber(),
                "net": pad.GetNetname(),
                "position_nm": [pad.GetPosition().x, pad.GetPosition().y],
                "size_nm": [pad.GetSize().x, pad.GetSize().y],
                "rotation": pad.GetOrientation().AsDegrees(),
            })
        footprints.append({
            "uuid": footprint.m_Uuid.AsString(), "reference": footprint.GetReference(),
            "value": footprint.GetValue(), "path": footprint.GetPath().AsString(),
            "position_nm": [footprint.GetPosition().x, footprint.GetPosition().y],
            "rotation": footprint.GetOrientationDegrees(), "layer": footprint.GetLayer(),
            "pads": sorted(pads, key=lambda item: item["uuid"]),
        })
    require(len(footprints) == 2 and all(len(item["pads"]) == 2 for item in footprints),
            "unexpected native two-resistor fixture inventory")
    uuids = [item["uuid"] for item in footprints]
    uuids += [pad["uuid"] for item in footprints for pad in item["pads"]]
    require(all(uuids) and len(uuids) == len(set(uuids)), "native PCB UUIDs missing/duplicate")
    return {"footprints": sorted(footprints, key=lambda item: item["uuid"])}


def rename_call(api, project, dry_run, expect_error):
    """Reuse Api unchanged, including its deliberate assertion on MCP errors."""
    before = len(api.responses)
    arguments = {"project": str(project / f"{OLD}.kicad_pro"), "new_name": NEW,
                 "rename_directory": True, "dry_run": dry_run}
    try:
        api.call("rename_project", arguments)
    except AssertionError as error:
        # Do not accept a transport assertion, JSON-RPC error or unrelated panic.
        require(len(api.responses) == before + 1, "API failure has no complete retained response")
        response = api.responses[-1]["response"]
        result = response.get("result")
        if (not expect_error or response.get("error") or not isinstance(result, dict)
                or result.get("isError") is not True or error.args != (result,)):
            raise
    require(len(api.responses) == before + 1, "missing exact rename response")
    entry = api.responses[-1]
    require(entry["request"]["params"] == {"name": "rename_project", "arguments": arguments},
            "retained response does not belong to this rename request")
    result = entry["response"]["result"]
    require(bool(result.get("isError")) == expect_error, f"unexpected rename result: {result}")
    content = result.get("content")
    require(content and content[0].get("type") == "text", "rename has no text body")
    body = json.loads(content[0]["text"])
    require(isinstance(body, dict), "rename body is not an object")
    return body


def start_api(lifecycle, binary, config, environment, out, sessions, name):
    api = lifecycle.Api(binary, config, environment, out / f"{name}.stderr.log")
    sessions.append((name, api))
    return api


def recognize_ownership_refusal(body, project, opened):
    """Fail closed against the current project.rs/pcb_board.rs error contracts."""
    pro = str(project / f"{OLD}.kicad_pro")
    board = str(project / f"{OLD}.kicad_pcb")
    require(opened.get("ipc_available") is True and opened.get("requested_open") is True
            and opened.get("requested_path") == board and opened.get("requested_board") == board
            and opened.get("requested_check_error") is None,
            "ownership refusal is not bound to this case's exact old-name board")
    outcome = body.get("outcome")
    require(isinstance(outcome, dict) and outcome.get("target") == pro,
            "ownership refusal targets a different or missing old-name project")
    error, message = body.get("error"), body.get("message")
    require(isinstance(error, dict) and isinstance(message, str) and bool(message),
            "ownership refusal requires structured error fields and an actual message")
    kind, reason = error.get("kind"), error.get("reason")
    require(isinstance(kind, str), "ownership refusal is missing its error kind")

    if kind == "unsafe_file_fallback":
        require(set(error) == {"kind", "path", "reason"},
                "unexpected ownership error schema")
        require(isinstance(reason, str) and bool(reason), "ownership refusal has no machine reason")
        if reason in {"kicad_project_lock_present", "kicad_project_lock_unreadable"}:
            require(error["path"] == pro, "project lock refusal belongs to another project")
            lock = str(project / f"~{OLD}.kicad_pro.lck")
            tail = (
                " pcbnew and Eeschema rewrite the project file from their own copy when "
                "they save, so a project rename written to it now would be reverted. "
                "This ownership check refused further work. Any earlier completed rename "
                "steps remain applied as reported. Close the project in KiCad, or make "
                "the change there, and retry. If no KiCad program has this project open, "
                "the lock was left behind by a crash: delete it and retry."
            )
            if reason == "kicad_project_lock_present":
                situation = (
                    f"KiCad's project lock '{lock}' is present, so a KiCad program "
                    "holds this project."
                )
                require(message == situation + tail, "project lock message does not match its reason")
            else:
                prefix = f"KiCad's project lock '{lock}' could not be inspected ("
                suffix = "), so a KiCad program holding this project cannot be ruled out." + tail
                require(re.fullmatch(re.escape(prefix) + r"(.+)" + re.escape(suffix), message),
                        "unreadable project lock message has no matching inspection detail")
            return {"route": "project-lock", "guard_path": pro, "lock_path": lock,
                    "reason": reason, "reason_source": "error.reason",
                    "message_contract": "project_lock_refusal_from + rename_outcome"}
        if reason in {"kicad_lock_present", "kicad_lock_unreadable"}:
            require(error["path"] == board, "board lock refusal belongs to another design")
            lock = str(project / f"~{OLD}.kicad_pcb.lck")
            rename_message = (
                f"KiCad ownership prevents renaming or rewriting '{board}'. "
                "Close or recover the editor first."
            )
            if message != rename_message:
                tail = (
                    " The saved board file may be older than unsaved editor state, so "
                    "Konnect did not modify it. Reopen or recover the board in KiCad, "
                    "reconcile it, and save the authoritative state. If KiCad was "
                    "deliberately closed cleanly, restart Konnect only after confirming "
                    "that the saved file is authoritative."
                )
                if reason == "kicad_lock_present":
                    situation = (
                        f"KiCad sibling lock '{lock}' is present; its contents cannot "
                        "prove whether an editor still owns newer in-memory state."
                    )
                    require(message == situation + tail, "board lock message does not match its reason")
                else:
                    prefix = f"KiCad sibling lock '{lock}' could not be inspected ("
                    suffix = "); its absence cannot be established." + tail
                    require(re.fullmatch(re.escape(prefix) + r"(.+)" + re.escape(suffix), message),
                            "unreadable board lock message has no matching inspection detail")
            return {"route": "design-lock", "guard_path": board, "lock_path": lock,
                    "reason": reason, "reason_source": "error.reason",
                    "message_contract": "rename_lock_error or board_lock_refusal_from"}

    if kind == "conflict":
        paths = error.get("paths")
        allowed = {str(project / f"{OLD}.kicad_sch"), str(project / CHILD)}
        require(set(error) == {"kind", "paths"} and isinstance(paths, list)
                and len(paths) == 1 and isinstance(paths[0], str) and paths[0] in allowed,
                "schematic lock refusal has no exact owned old-name design path")
        # Current schematic Conflict has no error.reason field. Its exact
        # source-defined lock message, not a generic conflict, supplies cause.
        cause = "KiCad editor lock is present or cannot be inspected"
        require(message == f"Project rename refused at '{paths[0]}': {cause}",
                "generic/non-ownership schematic conflict is not acceptance")
        return {"route": "design-lock", "guard_path": paths[0],
                "lock_path": str(project / ("~" + Path(paths[0]).name + ".lck")),
                "reason": cause, "reason_source": "exact rename_lock_error message",
                "message_contract": "rename_lock_error + rename_conflict"}

    live_message = (
        "KiCAD currently holds this board open, and a project rename written to the file "
        "would be discarded by KiCAD's next save. Close the board in KiCAD (or make "
        "the edit there) and retry — this tool has no IPC path for a live board yet."
    )
    require(kind == "handler_error" and set(error) == {"kind", "reason"}
            and reason == live_message and message == live_message,
            "missing/unclassified ownership reason is not native refusal evidence")
    # The current plain live-board helper has no error.path. Bind it through
    # the exact preceding open_project response and rename outcome target,
    # already checked above; never fabricate a structured error.path.
    return {"route": "exact-live-board", "guard_path": board,
            "reason": reason, "reason_source": "error.reason + exact message",
            "path_basis": "open_project requested_board + rename outcome target",
            "message_contract": "refuse_if_board_open_in_kicad + rename_outcome"}


def refusal(lifecycle, a8, run, binary, capture_helper, config, environment,
            project, upstream, out, sessions, report):
    config.write_text(json.dumps({"kicad_cli": environment["RENAME_KICAD_CLI"],
                                  "ipc_address": upstream,
                                  "eager_toolsets": True, "log_level": "warn"}))
    api = start_api(lifecycle, binary, config, environment, out, sessions, "editor-readiness")
    run.api = api
    editor = lifecycle.open_editor(project / f"{OLD}.kicad_pcb", environment, api,
                                   out / "editor.log", run)
    report["owned_editor_pid"] = editor.pid
    proxy_log = None
    if capture_helper is not None:
        listen = f"ipc://{run.temporary}/rename-proxy.sock"
        proxy_log = out / "ipc"
        with (out / "proxy.log").open("w") as log:
            proxy = subprocess.Popen([str(capture_helper), "proxy", listen, upstream,
                                      str(proxy_log)], env=environment, stdout=log, stderr=log)
        owner = a8.OwnedProxy(api, proxy)
        run.api = owner
        report["owned_proxy_pid"] = proxy.pid
        deadline = time.monotonic() + 3
        while not (proxy_log / "ready").exists():
            require(proxy.poll() is None, "owned native proxy exited before readiness")
            require(time.monotonic() < deadline, "owned native proxy readiness exceeded 3 seconds")
            time.sleep(0.05)
        api.close()
        config.write_text(json.dumps({"kicad_cli": environment["RENAME_KICAD_CLI"],
                                      "ipc_address": listen,
                                      "eager_toolsets": True, "log_level": "warn"}))
        api = start_api(lifecycle, binary, config, environment, out, sessions, "refusal")
        owner.api = api
    # This binds the actual editor/document through the proxy when requested.
    opened = api.call("open_project", {"path": str(project / f"{OLD}.kicad_pcb")})
    require(opened["ipc_available"] and opened["requested_open"] is True,
            "owned editor is not holding the exact refusal board")
    report["owned_editor_open"] = opened
    run.checkpoint("native_refusal_bound")
    before = archive_tree(project, out / "refusal-before")
    start = len(a8.rpc_requests(proxy_log / "rpc.jsonl")) if proxy_log else None
    body = rename_call(api, project, False, True)
    report["refusal"] = {"response": body, "reason": body.get("error"),
                         "message": body.get("message")}
    write_json(out / "native-project-rename.json", report)
    recognized = recognize_ownership_refusal(body, project, opened)
    report["recognized_route"] = recognized["route"]
    report["refusal"]["recognized_route"] = recognized
    write_json(out / "native-project-rename.json", report)
    require(body.get("outcome", {}).get("status") == "failed", "native refusal lacks failed outcome")
    require(body["outcome"]["requested"] == 1 and body["outcome"]["completed"] == 0
            and body["outcome"]["failed"] == 1,
            "native refusal claims completion")
    applied = body.get("applied", {})
    require(applied.get("moved_files") == [] and applied.get("verified_rewrites") == [],
            "native refusal claims applied work")
    require(applied.get("backup_move") is None and applied.get("directory_move") is None,
            "native refusal claims optional moves")
    require(applied.get("unverified_move_attempt") is None, "native refusal claims a move attempt")
    require(len(applied.get("current_paths", [])) >= 4, "native refusal omitted current paths")
    require(all(item["source"] == item["current_path"] for item in applied["current_paths"]),
            "native refusal relocated paths")
    after = archive_tree(project, out / "refusal-after")
    require(before == after, "files changed during the native refusal")
    require(not project.with_name(NEW).exists(), "native refusal moved its containing directory")
    report["refusal"]["files_unchanged"] = True
    report["refusal"]["before"] = before
    report["refusal"]["after"] = after
    report["refusal"]["rpc_observation"] = "not requested"
    report["refusal"]["exact_ipc_branch_covered"] = False
    if proxy_log:
        requests = a8.rpc_requests(proxy_log / "rpc.jsonl")[start:]
        write_json(out / "refusal-rpcs.json", requests)
        names = [row["command"].rsplit(".", 1)[-1] for row in requests]
        require(not set(names) & a8.MUTATIONS, f"mutation RPC during refusal: {names}")
        require(all(name.startswith("Get") for name in names),
                f"unclassified/non-read RPC during refusal: {names}")
        for row in requests:
            require((proxy_log / f"{row['index']:05}.request.pb").is_file()
                    and (proxy_log / f"{row['index']:05}.response.pb").is_file(),
                    "native proxy omitted raw request/response bytes")
        report["refusal"].update(
            rpc_observation={"commands": names, "mutation_count": 0},
            exact_ipc_branch_covered=(
                recognized["route"] == "exact-live-board"
                and "GetOpenDocuments" in names),
        )
    # Never remove authentic lock files to reach another authority branch.
    lifecycle.stop_editor(editor)
    run.api.close()  # OwnedProxy, when present, also stops only its owned proxy.
    run.checkpoint("refusal_editor_and_transport_stopped")
    report["refusal"]["after_owned_stop"] = archive_tree(project, out / "refusal-after-stop")


def positive(lifecycle, run, binary, cli, pcbnew, config, environment,
             project, upstream, out, sessions, report):
    before = archive_tree(project, out / "positive-before")
    board_before = board_identity(pcbnew, project / f"{OLD}.kicad_pcb")
    write_json(out / "pcb-before.json", board_before)
    require(tree_manifest(project) == before, "native saved-PCB load changed the positive input")
    netlist_before = export_netlist(cli, out / "positive-before",
        out / "positive-before" / f"{OLD}.kicad_sch", environment, out, "netlist-before")
    config.write_text(json.dumps({"kicad_cli": cli, "ipc_address": upstream,
                                  "eager_toolsets": True, "log_level": "warn"}))
    api = start_api(lifecycle, binary, config, environment, out, sessions, "positive")
    run.api = api
    run.checkpoint("closed_positive_copy")
    preview = rename_call(api, project, True, False)
    report["dry_run"] = preview
    write_json(out / "native-project-rename.json", report)
    require(preview.get("dry_run") is True, "dry-run response omitted its flag")
    require({item["file"]: item["references_updated"]
             for item in preview.get("content_rewrites", [])} == {
                 f"{NEW}.kicad_pro": 1, f"{NEW}.kicad_sch": 29,
                 f"{NEW}.kicad_pcb": 0, CHILD: 36,
             }, "dry-run reference preview differs from the independent fixture counts")
    require(tree_manifest(project) == before, "dry run changed files or tree entries")
    require(not project.with_name(NEW).exists(), "dry run moved its directory")
    body = rename_call(api, project, False, False)
    report["apply"] = body
    write_json(out / "native-project-rename.json", report)
    renamed = project.with_name(NEW)
    require(not project.exists() and renamed.is_dir(), "rename_directory was not applied")
    require(body.get("directory") == str(renamed), "success response has the wrong final directory")
    require(body.get("outcome", {}).get("status") == "complete", "rename outcome is not complete")
    require(body["outcome"]["requested"] == 1 and body["outcome"]["completed"] == 1
            and body["outcome"]["failed"] == 0,
            "rename whole-request accounting differs")
    after = archive_tree(renamed, out / "positive-after")
    old_token = f'(project "{OLD}"'.encode()
    new_token = f'(project "{NEW}"'.encode()
    expected = {}
    for old_name, count in [(f"{OLD}.kicad_sch", 29), (CHILD, 36)]:
        original = (out / "positive-before" / old_name).read_bytes()
        new_name = f"{NEW}.kicad_sch" if old_name != CHILD else CHILD
        actual = (renamed / new_name).read_bytes()
        require(original.count(old_token) == count, "native fixture project-key count changed")
        candidate = original.replace(old_token, new_token)
        require(actual == candidate and actual.count(new_token) == count
                and old_token not in actual, f"unexpected schematic bytes: {new_name}")
        expected[new_name] = {"sha256": sha(candidate), "references_updated": count}
    original = (out / "positive-before" / f"{OLD}.kicad_pro").read_bytes()
    from_name = f'"{OLD}.kicad_pro"'.encode()
    require(original.count(from_name) == 1 and f'"{OLD}"'.encode() not in original,
            "documented project-file substitution anchors changed")
    candidate = original.replace(from_name, f'"{NEW}.kicad_pro"'.encode())
    require((renamed / f"{NEW}.kicad_pro").read_bytes() == candidate,
            "project metadata differs beyond its documented filename substitution")
    expected[f"{NEW}.kicad_pro"] = {"sha256": sha(candidate), "references_updated": 1}
    original_board = (out / "positive-before" / f"{OLD}.kicad_pcb").read_bytes()
    require(old_token not in original_board, "native board token coverage assumption changed")
    require((renamed / f"{NEW}.kicad_pcb").read_bytes() == original_board,
            "native PCB bytes changed during its filename move")
    require(set(after) == {f"{NEW}.kicad_pro", f"{NEW}.kicad_sch", f"{NEW}.kicad_pcb", CHILD},
            "positive rename added or dropped project entries")
    applied = body.get("applied", {})
    require(len(applied.get("moved_files", [])) == 3, "wrong completed filename-move count")
    require(len(applied.get("verified_rewrites", [])) == 3, "wrong verified-rewrite count")
    actual_rewrites = {item["current_path"]: item["references_updated"]
                       for item in applied.get("verified_rewrites", [])}
    require(actual_rewrites == {str(renamed / name): item["references_updated"]
                               for name, item in expected.items()},
            "verified-rewrite accounting differs from independent byte edits")
    wanted_moves = {
        (str(project / f"{OLD}.{extension}"), str(project / f"{NEW}.{extension}"),
         str(renamed / f"{NEW}.{extension}"))
        for extension in ["kicad_pro", "kicad_sch", "kicad_pcb"]
    }
    require({(item["from"], item["to"], item["current_path"])
             for item in applied["moved_files"]} == wanted_moves,
            "completed move paths differ from the native filesystem observations")
    wanted_paths = {
        str(project / name): str(renamed / (
            f"{NEW}.{name.rsplit('.', 1)[-1]}" if name != CHILD else CHILD))
        for name in FILES
    }
    require({item["source"]: item["current_path"] for item in applied.get("current_paths", [])}
            == wanted_paths, "final current-path accounting differs")
    require(len(applied["current_paths"]) == len(wanted_paths)
            and applied.get("unverified_move_attempt") is None,
            "clean rename contains duplicate paths or an unverified move")
    require(applied.get("directory_move") == {"from": str(project), "to": str(renamed)}
            and applied.get("backup_move") is None and body.get("backups_folder") is None,
            "optional-move accounting differs")
    netlist_after = export_netlist(cli, out / "positive-after",
        out / "positive-after" / f"{NEW}.kicad_sch", environment, out, "netlist-after")
    require(netlist_before == netlist_after, "native schematic component/pin-net identity changed")
    board_after = board_identity(pcbnew, renamed / f"{NEW}.kicad_pcb")
    write_json(out / "pcb-after.json", board_after)
    require(board_before == board_after, "native saved-PCB footprint/pad-net/UUID identity changed")
    require(tree_manifest(renamed) == after, "native readback modified the positive project")
    report["positive"] = {
        "byte_oracles": expected, "pcb_sha256": sha(original_board),
        "before": before, "after": after, "schematic_netlist_identity_equal": True,
        "pcb_saved_identity_equal": True, "schematic_pcb_parity": "NOT CHECKED: independent fixtures",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disposable-worker", action="store_true", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--konnect", type=Path, required=True)
    parser.add_argument("--capture-helper", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.source_revision):
        parser.error("--source-revision requires the externally supplied full source-archive SHA")
    out = args.out.resolve()
    require(not out.is_relative_to(ROOT), "evidence directory must be outside the source archive")
    out.mkdir(parents=True, exist_ok=False)
    report = {
        "source_revision": args.source_revision.lower(), "status": "incomplete",
        "revision_authority": "external coordinator declaration; no Git invoked",
        "scope": "Linux KiCad 10.0.8 independent schematic and saved-PCB rename checks",
        "native_pcb_project_token_rewrite": "UNESTABLISHED: fixture has no project token",
        "schematic_pcb_parity": "NOT CHECKED: independent fixtures",
        "exact_ipc_branch_coverage": "report actual refusal; authentic locks are never deleted",
    }
    sessions = []
    primary = None
    sources = None
    try:
        require(sys.platform == "linux" and sys.version_info >= (3, 9),
                "requires a provisioned Linux/Python 3.9+ worker")
        require(bool(os.environ.get("DISPLAY")), "run under a private Xvfb display")
        sources = source_manifest()
        report["source_manifest_before"] = sources
        binary = args.konnect.resolve(strict=True)
        require(binary.is_file() and os.access(binary, os.X_OK), "Konnect binary is not executable")
        report["konnect_sha256"] = sha(binary.read_bytes())
        capture_helper = args.capture_helper.resolve(strict=True) if args.capture_helper else None
        if capture_helper:
            require(capture_helper.is_file() and os.access(capture_helper, os.X_OK),
                    "requested native capture helper is unavailable")
            report["capture_helper_sha256"] = sha(capture_helper.read_bytes())
        import library_field_native as lifecycle
        import native_footprint_children_native as a8
        with lifecycle.NativeRun(out, report) as run:
            control = Path(run.temporary).resolve()
            parents = []
            try:
                require(not control.is_relative_to(ROOT), "control root must be outside source")
                environment = a8.isolate(control)
                environment["PYTHONDONTWRITEBYTECODE"] = "1"
                report["desktop_isolation"] = "reused A8 private HOME/XDG/DBus, before pcbnew import"
                import pcbnew
                version = str(pcbnew.Version())
                require(re.match(r"^10\.0\.8(?:$|[-+.\s])", version), f"wrong pcbnew version: {version}")
                cli = shutil.which("kicad-cli", path=environment.get("PATH"))
                editor_program = shutil.which("pcbnew", path=environment.get("PATH"))
                require(cli and editor_program,
                        "native KiCad CLI/editor prerequisites are unavailable; no install attempted")
                cli_version = cli_run([cli, "version"], environment, out, "cli-version")
                require(re.match(r"^10\.0\.8(?:$|[-+.\s])", cli_version),
                        f"wrong native CLI version: {cli_version}")
                report.update(
                    pcbnew_version=version, cli_version=cli_version,
                    cli_program=str(Path(cli).resolve()),
                    cli_sha256=sha(Path(cli).read_bytes()),
                    editor_program=str(Path(editor_program).resolve()),
                    editor_sha256=sha(Path(editor_program).read_bytes()),
                )
                environment["RENAME_KICAD_CLI"] = cli
                upstream = f"ipc://{control}/kicad/api.sock"
                config = control / "server.json"
                # Separate pristine copies avoid stale-lock deletion and
                # editor-authored PRL/project state in the positive byte oracle.
                refusal_parent, positive_parent = control / "refusal", control / "positive"
                parents = [("refusal", refusal_parent), ("positive", positive_parent)]
                refusal_project = project_copy(refusal_parent)
                positive_project = project_copy(positive_parent)
                write_json(out / "native-project-rename.json", report)
                refusal(lifecycle, a8, run, binary, capture_helper, config, environment,
                        refusal_project, upstream, out, sessions, report)
                write_json(out / "native-project-rename.json", report)
                positive(lifecycle, run, binary, cli, pcbnew, config, environment,
                         positive_project, upstream, out, sessions, report)
                run.checkpoint("native_rename_verified")
            finally:
                artifact_errors = []
                for name, parent in parents:
                    if parent.is_dir():
                        try:
                            archive_tree(parent, out / f"final-{name}-tree")
                        except BaseException as error:
                            artifact_errors.append({"artifact": f"final-{name}-tree", "error": repr(error)})
                for name, api in sessions:
                    try:
                        write_json(out / f"{name}-responses.json", api.responses)
                    except BaseException as error:
                        artifact_errors.append({"artifact": f"{name}-responses.json", "error": repr(error)})
                if artifact_errors:
                    report["artifact_errors"] = artifact_errors
                    if sys.exc_info()[0] is None:
                        raise RuntimeError(f"native artifact retention failed: {artifact_errors}")
        require(report["lifecycle"]["temporary_directory_removed"] is True,
                "unchanged lifecycle did not verify control-root absence")
        report["status"] = "passed"
    except BaseException as error:
        primary = error
        report.update(status="failed", error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        errors = []
        try:
            after = source_manifest()
            report["source_manifest_after"] = after
            require(sources is not None and after == sources, "source/original fixture hashes changed")
            report["source_and_original_fixtures_unchanged"] = True
        except BaseException as error:
            errors.append({"stage": "source_hashes", "error": repr(error)})
        if errors:
            report.update(status="failed", evidence_errors=errors)
        write_json(out / "native-project-rename.json", report)
        if errors and primary is None:
            raise RuntimeError(f"native evidence verification failed: {errors}")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
