"""Tests for the narrowly-scoped KiCad 10 native Specctra bridge."""

import importlib.util
import json
import os
import tempfile
import types
import unittest
import urllib.error
import urllib.request


_PLUGIN_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_SPEC = importlib.util.spec_from_file_location(
    "konnect_native_bridge_under_test",
    os.path.join(_PLUGIN_DIR, "native_bridge.py"),
)
native_bridge = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(native_bridge)


class _Board:
    def __init__(self, path):
        self.path = path

    def GetFileName(self):
        return self.path


class _Pcbnew:
    def __init__(self, board_path):
        self.board = _Board(board_path)

    def GetBoard(self):
        return self.board

    @staticmethod
    def ExportSpecctraDSN(output):
        with open(output, "w", encoding="utf-8") as stream:
            stream.write("(pcb native-test)\n")
        return True


class _Wx:
    @staticmethod
    def CallAfter(callback):
        callback()


def _request(registration, path, body=None, token=None):
    headers = {"Authorization": f"Bearer {token or registration['token']}"}
    data = None
    method = "GET"
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
        method = "POST"
    request = urllib.request.Request(
        registration["address"] + path,
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        return response.status, json.load(response)


class NativeBridgeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.board = os.path.join(self.temp.name, "board.kicad_pcb")
        with open(self.board, "w", encoding="utf-8") as stream:
            stream.write("(kicad_pcb)\n")
        self.bridge = native_bridge.NativeSpecctraBridge(
            _Pcbnew(self.board),
            _Wx(),
            root=os.path.join(self.temp.name, "bridge"),
        )
        self.assertTrue(self.bridge.start())
        self.addCleanup(self.bridge.stop)
        with open(self.bridge.registration_path, encoding="utf-8") as stream:
            self.registration = json.load(stream)

    def test_status_and_export_require_the_registration_token(self):
        status, payload = _request(self.registration, "/v1/status")
        self.assertEqual(status, 200)
        self.assertTrue(payload["native_specctra_export"])

        with self.assertRaises(urllib.error.HTTPError) as caught:
            _request(self.registration, "/v1/status", token="wrong")
        self.assertEqual(caught.exception.code, 401)

    def test_export_uses_a_bridge_owned_path_and_reports_the_active_board(self):
        status, payload = _request(
            self.registration,
            "/v1/export-specctra-dsn",
            {"expected_board": self.board},
        )
        self.assertEqual(status, 200)
        self.assertTrue(payload["success"])
        self.assertEqual(os.path.realpath(payload["board_path"]), os.path.realpath(self.board))
        self.assertTrue(os.path.isfile(payload["dsn_path"]))
        self.assertEqual(
            os.path.commonpath([payload["dsn_path"], self.bridge.session_dir]),
            self.bridge.session_dir,
        )
        self.assertGreater(payload["dsn_bytes"], 0)

    def test_export_refuses_a_different_board(self):
        with self.assertRaises(urllib.error.HTTPError) as caught:
            _request(
                self.registration,
                "/v1/export-specctra-dsn",
                {"expected_board": os.path.join(self.temp.name, "other.kicad_pcb")},
            )
        self.assertEqual(caught.exception.code, 409)

    def test_shutdown_endpoint_is_authenticated_and_disabled_by_default(self):
        body = {"expected_board": self.board, "requester_pid": os.getpid()}
        with self.assertRaises(urllib.error.HTTPError) as caught:
            _request(self.registration, "/v1/shutdown-editor", body, token="wrong")
        self.assertEqual(caught.exception.code, 401)
        _, payload = _request(self.registration, "/v1/shutdown-editor", body)
        self.assertEqual(payload["status"], "unsupported")

    def test_stop_removes_registration_and_session_artifacts(self):
        registration = self.bridge.registration_path
        session = self.bridge.session_dir
        self.bridge.stop()
        self.assertFalse(os.path.exists(registration))
        self.assertFalse(os.path.exists(session))


class _Frame:
    def __init__(self, title="board \u2014 PCB Editor", accepted=True):
        self.title = title
        self.accepted = accepted
        self.closes = []

    def GetName(self):
        return "PcbFrame"

    def GetTitle(self):
        return self.title

    def IsEnabled(self):
        return True

    def Close(self, force):
        self.closes.append(force)
        return self.accepted


class EditorShutdownTests(unittest.TestCase):
    def setUp(self):
        from unittest.mock import patch
        platform = patch.object(native_bridge, "sys_platform", return_value="linux")
        platform.start()
        self.addCleanup(platform.stop)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = os.path.join(self.temp.name, "board.kicad_pcb")
        with open(self.path, "w", encoding="utf-8") as stream:
            stream.write("(kicad_pcb)\n")
        self.frame = _Frame()
        self.windows = [self.frame]
        self.pcbnew = _Pcbnew(self.path)
        self.pcbnew.Version = lambda: "10.0.8"
        self.wx = types.SimpleNamespace(
            Frame=_Frame, Dialog=type("Dialog", (), {}),
            GetTopLevelWindows=lambda: self.windows, CallAfter=lambda f: f(),
        )
        self.bridge = native_bridge.NativeSpecctraBridge(
            self.pcbnew, self.wx, root=self.temp.name, allow_editor_shutdown=True,
        )
        self.bridge.running = lambda: True

    def close(self, path=None):
        return self.bridge.editor_operation(path or self.path, True, 999999)

    def test_clean_close_is_native_and_vetoable(self):
        self.assertEqual(self.close()["status"], "requested")
        self.assertEqual(self.frame.closes, [False])

    def test_dirty_board_never_enters_the_native_close_handler(self):
        self.frame.title = "*board \u2014 PCB Editor"
        self.assertEqual(self.close()["status"], "save_required")
        self.assertEqual(self.frame.closes, [])

    def test_wrong_board_and_ancillary_windows_remain_untouched(self):
        self.assertEqual(self.close(self.path + ".other")["status"], "wrong_target")
        self.windows.append(_Frame())
        self.assertEqual(self.close()["status"], "refused")
        self.assertEqual(self.frame.closes, [])

    def test_veto_and_unknown_title_are_refusals(self):
        self.frame.accepted = False
        self.assertEqual(self.close()["status"], "refused")
        self.frame.title = "unknown"
        self.assertEqual(self.close()["status"], "refused")
        self.assertEqual(len(self.frame.closes), 1)

    def test_disabled_unsupported_and_plugin_child_server_are_refused(self):
        self.bridge.allow_editor_shutdown = False
        self.assertEqual(self.close()["status"], "unsupported")
        self.bridge.allow_editor_shutdown = True
        self.pcbnew.Version = lambda: "11.0.0"
        self.assertEqual(self.close()["status"], "unsupported")
        self.pcbnew.Version = lambda: "10.0.8"
        self.bridge.mcp_server_pid = lambda: 999999
        self.assertEqual(self.close()["status"], "refused")
        self.bridge.mcp_server_pid = lambda: None
        self.assertEqual(
            self.bridge.editor_operation(self.path, True, os.getpid())["status"], "refused"
        )
        self.assertEqual(self.frame.closes, [])

    def test_expired_queued_close_never_runs_later(self):
        from unittest.mock import patch
        queued = []
        self.wx.CallAfter = queued.append
        with patch.object(native_bridge, "UI_TIMEOUT_SECONDS", 0):
            self.assertEqual(self.close()["status"], "timed_out")
            queued.pop()()
        self.assertEqual(self.frame.closes, [])

    def test_disabled_or_stopped_bridge_cannot_execute_a_queued_close(self):
        from unittest.mock import patch
        queued = []
        self.wx.CallAfter = queued.append
        with patch.object(native_bridge.threading, "Event") as event:
            event.return_value.wait.return_value = False
            self.assertEqual(self.close()["status"], "timed_out")
            self.bridge.running = lambda: False
            queued.pop()()
        self.assertEqual(self.frame.closes, [])


if __name__ == "__main__":
    unittest.main()
