"""Widget definition helpers for CDAP manifest construction."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class Widget:
    """Represents a CDAP widget for the device manifest."""

    type: str
    id: str
    label: str
    group: str = ""
    value: Any = None
    readonly: bool = False

    # Gauge / Slider
    unit: str = ""
    min: float = 0.0
    max: float = 0.0
    step: float = 0.0
    precision: int = 0
    warning_low: float = 0.0
    warning_high: float = 0.0

    # Button
    confirm: bool = False
    confirm_message: str = ""
    style: str = ""
    icon: str = ""
    cooldown: int = 0

    # Select
    options: list[dict[str, Any]] = field(default_factory=list)

    # Chart
    chart_type: str = ""
    points: int = 0
    series: list[dict[str, Any]] = field(default_factory=list)
    retention: str = ""

    # Table
    columns: list[dict[str, Any]] = field(default_factory=list)
    max_rows: int = 0
    sortable: bool = False

    # RBAC
    permissions: dict[str, str] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize widget to CDAP-compatible dict, omitting zero-value fields."""
        d: dict[str, Any] = {"type": self.type, "id": self.id, "label": self.label}
        _optional = {
            "group": self.group,
            "value": self.value,
            "readonly": self.readonly,
            "unit": self.unit,
            "min": self.min,
            "max": self.max,
            "step": self.step,
            "precision": self.precision,
            "warning_low": self.warning_low,
            "warning_high": self.warning_high,
            "confirm": self.confirm,
            "confirm_message": self.confirm_message,
            "style": self.style,
            "icon": self.icon,
            "cooldown": self.cooldown,
            "options": self.options,
            "chart_type": self.chart_type,
            "points": self.points,
            "series": self.series,
            "retention": self.retention,
            "columns": self.columns,
            "max_rows": self.max_rows,
            "sortable": self.sortable,
        }
        for k, v in _optional.items():
            if v:  # skip zero-values
                d[k] = v
        if self.permissions:
            d["permissions"] = self.permissions
        return d


# ── Factory helpers ───────────────────────────────────────────────────

def gauge(
    id: str,
    label: str,
    *,
    group: str = "",
    unit: str = "%",
    min_val: float = 0,
    max_val: float = 100,
    warning_high: float = 0,
    precision: int = 1,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="gauge", id=id, label=label, group=group,
        unit=unit, min=min_val, max=max_val,
        warning_high=warning_high, precision=precision,
        permissions=permissions or {"read": "viewer"},
    )


def toggle(
    id: str,
    label: str,
    *,
    group: str = "",
    value: bool = False,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="toggle", id=id, label=label, group=group,
        value=value,
        permissions=permissions or {"read": "viewer", "control": "operator"},
    )


def button(
    id: str,
    label: str,
    *,
    group: str = "",
    icon: str = "",
    confirm: bool = False,
    confirm_message: str = "",
    cooldown: int = 0,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="button", id=id, label=label, group=group,
        icon=icon, confirm=confirm, confirm_message=confirm_message,
        cooldown=cooldown,
        permissions=permissions or {"control": "operator"},
    )


def text_widget(
    id: str,
    label: str,
    *,
    group: str = "",
    readonly: bool = True,
    value: str = "",
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="text", id=id, label=label, group=group,
        readonly=readonly, value=value,
        permissions=permissions or {"read": "viewer"},
    )


def led(
    id: str,
    label: str,
    *,
    group: str = "",
    value: bool = False,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="led", id=id, label=label, group=group,
        value=value,
        permissions=permissions or {"read": "viewer"},
    )


def slider(
    id: str,
    label: str,
    *,
    group: str = "",
    unit: str = "",
    min_val: float = 0,
    max_val: float = 100,
    step: float = 1,
    value: float = 0,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="slider", id=id, label=label, group=group,
        unit=unit, min=min_val, max=max_val, step=step, value=value,
        permissions=permissions or {"read": "viewer", "control": "operator"},
    )


def select(
    id: str,
    label: str,
    *,
    group: str = "",
    options: list[dict[str, Any]] | None = None,
    value: Any = None,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="select", id=id, label=label, group=group,
        options=options or [], value=value,
        permissions=permissions or {"read": "viewer", "control": "operator"},
    )


def chart(
    id: str,
    label: str,
    *,
    group: str = "",
    chart_type: str = "line",
    points: int = 60,
    series: list[dict[str, Any]] | None = None,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="chart", id=id, label=label, group=group,
        chart_type=chart_type, points=points, series=series or [],
        permissions=permissions or {"read": "viewer"},
    )


def table(
    id: str,
    label: str,
    *,
    group: str = "",
    columns: list[dict[str, Any]] | None = None,
    max_rows: int = 100,
    sortable: bool = True,
    permissions: dict[str, str] | None = None,
) -> Widget:
    return Widget(
        type="table", id=id, label=label, group=group,
        columns=columns or [], max_rows=max_rows, sortable=sortable,
        permissions=permissions or {"read": "viewer"},
    )
