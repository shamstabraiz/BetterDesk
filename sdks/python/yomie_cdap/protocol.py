"""CDAP protocol constants and WebSocket transport."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass, field
from typing import Any


# ── Message envelope ──────────────────────────────────────────────────

@dataclass
class Message:
    """CDAP protocol message envelope."""

    type: str
    payload: dict[str, Any] = field(default_factory=dict)
    id: str | None = None
    timestamp: str | None = None

    def to_json(self) -> str:
        obj: dict[str, Any] = {"type": self.type, "payload": self.payload}
        if self.id:
            obj["id"] = self.id
        obj["timestamp"] = self.timestamp or time.strftime(
            "%Y-%m-%dT%H:%M:%SZ", time.gmtime()
        )
        return json.dumps(obj, separators=(",", ":"))

    @classmethod
    def from_json(cls, raw: str | bytes) -> "Message":
        data = json.loads(raw)
        return cls(
            type=data.get("type", ""),
            payload=data.get("payload", {}),
            id=data.get("id"),
            timestamp=data.get("timestamp"),
        )


# ── Auth payloads ─────────────────────────────────────────────────────

def auth_payload_api_key(key: str, device_id: str = "", client_version: str = "1.0.0") -> dict:
    return {
        "method": "api_key",
        "key": key,
        "device_id": device_id,
        "client_version": client_version,
    }


def auth_payload_device_token(token: str, device_id: str = "", client_version: str = "1.0.0") -> dict:
    return {
        "method": "device_token",
        "token": token,
        "device_id": device_id,
        "client_version": client_version,
    }


def auth_payload_user_password(
    username: str, password: str, device_id: str = "", client_version: str = "1.0.0"
) -> dict:
    return {
        "method": "user_password",
        "username": username,
        "password": password,
        "device_id": device_id,
        "client_version": client_version,
    }


# ── Command actions ───────────────────────────────────────────────────

ACTION_SET = "set"
ACTION_TRIGGER = "trigger"
ACTION_EXECUTE = "execute"
ACTION_RESET = "reset"
ACTION_QUERY = "query"

# ── Command response statuses ────────────────────────────────────────

STATUS_OK = "ok"
STATUS_ERROR = "error"
STATUS_TIMEOUT = "timeout"
STATUS_REJECTED = "rejected"
STATUS_QUEUED = "queued"

# ── Known capabilities ───────────────────────────────────────────────

CAPABILITIES = [
    "telemetry",
    "commands",
    "alerts",
    "logs",
    "remote_desktop",
    "video_stream",
    "audio",
    "clipboard",
    "file_transfer",
    "input_control",
]

# ── Widget types ─────────────────────────────────────────────────────

WIDGET_TOGGLE = "toggle"
WIDGET_GAUGE = "gauge"
WIDGET_BUTTON = "button"
WIDGET_LED = "led"
WIDGET_CHART = "chart"
WIDGET_SELECT = "select"
WIDGET_SLIDER = "slider"
WIDGET_TEXT = "text"
WIDGET_TABLE = "table"
WIDGET_TERMINAL = "terminal"
WIDGET_DESKTOP = "desktop"
WIDGET_VIDEO = "video_stream"
WIDGET_FILE_BROWSER = "file_browser"

# ── Alert severities ─────────────────────────────────────────────────

SEVERITY_CRITICAL = "critical"
SEVERITY_WARNING = "warning"
SEVERITY_INFO = "info"
