"""Authenticated KiCad 10 native-operation bridge for Konnect.

This module is loaded only by the legacy ``pcbnew.ActionPlugin``.  It exposes
deliberately narrow operations to the external Rust server: native Specctra
export and an independently opt-in, version-pinned PCB Editor close. It is not
a general Python execution service or the KiCad 11 integration path.
"""

import json
import os
import secrets
import shutil
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


PROTOCOL_VERSION = 1
MAX_REQUEST_BYTES = 16 * 1024
MAX_DSN_BYTES = 512 * 1024 * 1024
UI_TIMEOUT_SECONDS = 30


def bridge_root():
    """Return the per-user registration directory shared with Konnect Rust."""
    override = os.environ.get("KONNECT_BRIDGE_DIR")
    if override:
        return os.path.abspath(os.path.expanduser(override))
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA", os.path.expanduser("~"))
    elif sys_platform() == "darwin":
        base = os.path.expanduser("~/Library/Application Support")
    else:
        base = os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share"))
    return os.path.join(base, "konnect", "native-bridge")


def sys_platform():
    # Kept behind a function so platform-path behavior is easy to test.
    import sys

    return sys.platform


def _canonical(path):
    return os.path.normcase(os.path.realpath(os.path.abspath(path)))


def _write_owner_only_json(path, value):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    temporary = f"{path}.{uuid.uuid4().hex}.tmp"
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            json.dump(value, stream, separators=(",", ":"))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        try:
            os.chmod(path, 0o600)
        except OSError:
            pass
    finally:
        try:
            os.remove(temporary)
        except OSError:
            pass


class _BridgeServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address, bridge):
        self.bridge = bridge
        super().__init__(address, _BridgeRequestHandler)


class _BridgeRequestHandler(BaseHTTPRequestHandler):
    server_version = "KonnectNativeBridge/1"

    def log_message(self, _format, *_args):
        # pcbnew has no reliable stderr and the bearer token must never enter logs.
        return

    def _authorized(self):
        expected = f"Bearer {self.server.bridge.token}"
        return secrets.compare_digest(self.headers.get("Authorization", ""), expected)

    def _json(self, status, value):
        payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(payload)

    def _require_auth(self):
        if self._authorized():
            return True
        self._json(401, {"success": False, "error": "unauthorized"})
        return False

    def do_GET(self):
        if not self._require_auth():
            return
        if self.path != "/v1/status":
            self._json(404, {"success": False, "error": "unknown operation"})
            return
        self._json(200, self.server.bridge.status())

    def do_POST(self):
        if not self._require_auth():
            return
        if self.path not in (
            "/v1/export-specctra-dsn", "/v1/editor-target", "/v1/shutdown-editor"
        ):
            self._json(404, {"success": False, "error": "unknown operation"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self._json(400, {"success": False, "error": "invalid content length"})
            return
        if length <= 0 or length > MAX_REQUEST_BYTES:
            self._json(413, {"success": False, "error": "request body is outside the supported size"})
            return
        try:
            request = json.loads(self.rfile.read(length).decode("utf-8"))
            expected_board = request["expected_board"]
            if not isinstance(expected_board, str) or not expected_board:
                raise ValueError("expected_board must be a non-empty string")
            if self.path == "/v1/export-specctra-dsn":
                result = self.server.bridge.export_specctra_dsn(expected_board)
            else:
                result = self.server.bridge.editor_operation(
                    expected_board, self.path == "/v1/shutdown-editor",
                    request.get("requester_pid"),
                )
        except (KeyError, ValueError, json.JSONDecodeError) as error:
            self._json(400, {"success": False, "error": str(error)})
            return
        except Exception as error:  # The response is bounded; traceback stays inside KiCad.
            self._json(409, {"success": False, "error": str(error)[:2000]})
            return
        self._json(200, result)


class NativeSpecctraBridge:
    """Own one authenticated loopback server for one PCB Editor process."""

    def __init__(self, pcbnew_module, wx_module, root=None,
                 allow_editor_shutdown=False, mcp_server_pid=None):
        self.pcbnew = pcbnew_module
        self.wx = wx_module
        self.root = root or bridge_root()
        self.token = secrets.token_urlsafe(32)
        self.server = None
        self.thread = None
        self.registration_path = os.path.join(
            self.root,
            f"bridge-{os.getpid()}-{secrets.token_hex(8)}.json",
        )
        self.session_dir = None
        self.started_at = None
        self.operation_lock = threading.Lock()
        self.allow_editor_shutdown = allow_editor_shutdown is True
        self.allow_specctra_export = True
        self.mcp_server_pid = mcp_server_pid or (lambda: None)

    def available(self):
        return callable(getattr(self.pcbnew, "ExportSpecctraDSN", None))

    def running(self):
        return self.server is not None and self.thread is not None and self.thread.is_alive()

    def start(self):
        if self.running():
            return True
        if not self.available():
            return False
        os.makedirs(self.root, exist_ok=True)
        self.session_dir = tempfile.mkdtemp(prefix=f"session-{os.getpid()}-", dir=self.root)
        self.server = _BridgeServer(("127.0.0.1", 0), self)
        self.started_at = time.time()
        port = self.server.server_address[1]
        registration = {
            "protocol_version": PROTOCOL_VERSION,
            "pid": os.getpid(),
            "address": f"http://127.0.0.1:{port}",
            "token": self.token,
            "started_at_unix": self.started_at,
        }
        try:
            _write_owner_only_json(self.registration_path, registration)
            self.thread = threading.Thread(
                target=self.server.serve_forever,
                name="konnect-native-bridge",
                daemon=True,
            )
            self.thread.start()
        except Exception:
            self.stop()
            raise
        return True

    def stop(self):
        server = self.server
        thread = self.thread
        self.server = None
        self.thread = None
        if server is not None:
            # shutdown() waits for serve_forever(). Calling it when start()
            # failed before the thread began would deadlock KiCad's UI thread.
            if thread is not None and thread.is_alive():
                server.shutdown()
            server.server_close()
        if thread is not None and thread is not threading.current_thread():
            thread.join(timeout=5)
        try:
            os.remove(self.registration_path)
        except OSError:
            pass
        if self.session_dir:
            shutil.rmtree(self.session_dir, ignore_errors=True)
        self.session_dir = None

    def status(self):
        return {
            "success": True,
            "protocol_version": PROTOCOL_VERSION,
            "pid": os.getpid(),
            "native_specctra_export": self.available() and self.allow_specctra_export,
        }

    def editor_operation(self, expected_board, close, requester_pid=None):
        """Validate and close on ONE UI turn, without yielding to other edits.

        KiCad 10.0.8 PCB_EDIT_FRAME::UpdateTitle/OnModify synchronously mirror
        IsContentModified as a leading '*'. There is no SWIG dirty-state query.
        Keep this compatibility path version-pinned and refuse unknown titles,
        ancillary frames and dialogs before invoking the vetoable native Close.
        """
        if not self.allow_editor_shutdown:
            return {"success": False, "status": "unsupported",
                    "reason": "native_editor_shutdown is disabled"}
        if not self.operation_lock.acquire(blocking=False):
            return {"success": False, "status": "refused", "reason": "bridge is busy"}
        completed = threading.Event()
        result = {}
        deadline = time.monotonic() + UI_TIMEOUT_SECONDS

        def run_on_ui_thread():
            try:
                # An expired queued callback must NEVER perform a late shutdown.
                if time.monotonic() >= deadline:
                    result.update(success=False, status="timed_out",
                                  reason="UI request expired before execution")
                    return
                if not self.allow_editor_shutdown or not self.running():
                    result.update(success=False, status="refused",
                                  reason="shutdown capability was disabled or bridge stopped")
                    return
                version = self.pcbnew.Version()
                if sys_platform() != "linux" or version != "10.0.8":
                    result.update(success=False, status="unsupported",
                                  reason=f"requires Linux KiCad 10.0.8; observed {sys_platform()} KiCad {version}",
                                  kicad_version=version)
                    return
                board = self.pcbnew.GetBoard()
                path = board.GetFileName() if board is not None else ""
                if not path or _canonical(path) != _canonical(expected_board):
                    result.update(success=False, status="wrong_target",
                                  reason="active board does not match expected_board")
                    return
                windows = [w for w in self.wx.GetTopLevelWindows()
                           if isinstance(w, (self.wx.Frame, self.wx.Dialog))]
                frames = [w for w in windows if w.GetName() == "PcbFrame"]
                if len(frames) != 1 or len(windows) != 1:
                    result.update(success=False, status="refused",
                                  reason="requires exactly one PCB frame and no ancillary frames/dialogs; "
                                         f"observed names: {[w.GetName() for w in windows]}")
                    return
                frame = frames[0]
                title = frame.GetTitle()
                if title.startswith("*") or not os.path.isfile(path):
                    result.update(success=False, status="save_required",
                                  reason="save the target board explicitly before shutdown")
                    return
                stem = os.path.splitext(os.path.basename(path))[0]
                if not title.startswith(stem + " \u2014 ") or not frame.IsEnabled():
                    result.update(success=False, status="refused",
                                  reason="native frame identity/title or enabled state is uncertain")
                    return
                result.update(success=True, status="ready", pid=os.getpid(),
                              board_path=_canonical(path), kicad_version=version,
                              mcp_server_pid=self.mcp_server_pid())
                if close:
                    if type(requester_pid) is not int or requester_pid <= 0:
                        result.update(success=False, status="refused",
                                      reason="requester PID is required")
                        return
                    if requester_pid == os.getpid() or self.mcp_server_pid() == requester_pid:
                        result.update(success=False, status="refused",
                                      reason="close would terminate the requesting MCP server")
                        return
                    # Non-forced wx close invokes KiCad's own canCloseWindow and
                    # doCloseWindow, not Destroy(), RunAction or input automation.
                    accepted = frame.Close(False)
                    result.update(success=bool(accepted),
                                  status="requested" if accepted else "refused",
                                  reason="" if accepted else "KiCad vetoed normal close")
            except Exception as error:
                result.update(success=False, status="uncertain", reason=str(error)[:2000])
            finally:
                completed.set()
                self.operation_lock.release()

        try:
            self.wx.CallAfter(run_on_ui_thread)
        except Exception:
            self.operation_lock.release()
            raise
        if not completed.wait(UI_TIMEOUT_SECONDS):
            return {"success": False, "status": "timed_out",
                    "reason": "UI thread did not complete; inspect before retrying"}
        return result

    def export_specctra_dsn(self, expected_board):
        if not self.allow_specctra_export:
            raise RuntimeError("native Specctra export is disabled")
        if not self.running() or not self.session_dir:
            raise RuntimeError("native bridge is not running")
        if not self.operation_lock.acquire(blocking=False):
            raise RuntimeError("native bridge is busy with another operation")
        completed = threading.Event()
        result = {}

        def run_on_ui_thread():
            try:
                board = self.pcbnew.GetBoard()
                board_path = board.GetFileName() if board is not None else ""
                if not board_path:
                    raise RuntimeError("PCB Editor has no saved active board")
                if _canonical(board_path) != _canonical(expected_board):
                    raise RuntimeError(
                        f"active KiCad board '{board_path}' does not match requested board '{expected_board}'"
                    )
                output = os.path.join(self.session_dir, f"native-{uuid.uuid4().hex}.dsn")
                try:
                    ok = self.pcbnew.ExportSpecctraDSN(output)
                except TypeError:
                    ok = self.pcbnew.ExportSpecctraDSN(board, output)
                if not ok or not os.path.isfile(output):
                    raise RuntimeError("KiCad native Specctra export failed")
                size = os.path.getsize(output)
                if size <= 0 or size > MAX_DSN_BYTES:
                    try:
                        os.remove(output)
                    except OSError:
                        pass
                    raise RuntimeError(f"KiCad native DSN size {size} is outside the supported range")
                result.update(
                    success=True,
                    protocol_version=PROTOCOL_VERSION,
                    pid=os.getpid(),
                    board_path=board_path,
                    dsn_path=output,
                    dsn_bytes=size,
                )
            except Exception as error:
                result["error"] = str(error)
            finally:
                completed.set()
                self.operation_lock.release()

        try:
            self.wx.CallAfter(run_on_ui_thread)
        except Exception:
            self.operation_lock.release()
            raise
        if not completed.wait(UI_TIMEOUT_SECONDS):
            # The queued callback still owns the operation lock and releases it
            # if/when KiCad's UI thread resumes. A second export must not race it.
            raise RuntimeError("KiCad UI thread did not complete native Specctra export")
        if "error" in result:
            raise RuntimeError(result["error"])
        return result
