"""codenextremote CDAP Python SDK — bridge framework for IoT and automation devices."""

from codenextremote_cdap.bridge import CDAPBridge
from codenextremote_cdap.widgets import (
    Widget,
    gauge,
    toggle,
    button,
    text_widget,
    led,
    slider,
    select,
    chart,
    table,
)

__version__ = "1.0.0"
__all__ = [
    "CDAPBridge",
    "Widget",
    "gauge",
    "toggle",
    "button",
    "text_widget",
    "led",
    "slider",
    "select",
    "chart",
    "table",
]
