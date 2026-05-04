#!/usr/bin/env python3
"""Yomie CDAP — Modbus TCP/RTU Bridge.

Polls Modbus registers/coils at a configurable interval and pushes values
to Yomie via CDAP.  Incoming commands (toggle, slider set) are written
back to the Modbus target.

Usage:
    pip install yomie-cdap pymodbus
    python bridge_modbus.py --config config.json
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import struct
import sys
from pathlib import Path
from typing import Any

from yomie_cdap import CDAPBridge, gauge, toggle, slider, textWidget

logger = logging.getLogger("bridge_modbus")

# ── Modbus helpers ────────────────────────────────────────────────────

DATA_FORMATS: dict[str, str] = {
    "int16": ">h",
    "uint16": ">H",
    "int32": ">i",
    "uint32": ">I",
    "float32": ">f",
}


def decode_registers(regs: list[int], data_type: str, scale: float, offset: float) -> float:
    """Convert raw register values to a scaled float."""
    fmt = DATA_FORMATS.get(data_type, ">H")
    size = struct.calcsize(fmt) // 2  # number of 16-bit registers
    raw_bytes = b""
    for r in regs[:size]:
        raw_bytes += struct.pack(">H", r)
    value = struct.unpack(fmt, raw_bytes)[0]
    return round(value * scale + offset, 4)


def encode_value(value: float, data_type: str, scale: float, offset: float) -> list[int]:
    """Convert a scaled float back to raw register values."""
    raw = (value - offset) / scale if scale else value
    fmt = DATA_FORMATS.get(data_type, ">H")
    packed = struct.pack(fmt, int(raw))
    regs = []
    for i in range(0, len(packed), 2):
        regs.append(struct.unpack(">H", packed[i : i + 2])[0])
    return regs


# ── Bridge ────────────────────────────────────────────────────────────


class ModbusBridge:
    """Modbus ↔ CDAP bridge."""

    def __init__(self, cfg: dict[str, Any]):
        self.cfg = cfg
        self.mcfg = cfg["modbus"]
        self.registers: list[dict] = cfg.get("registers", [])
        self.client = None  # pymodbus client instance

        # Build CDAP bridge
        cdap = cfg["cdap"]
        self.bridge = CDAPBridge(
            server=cdap["server"],
            auth_method="api_key",
            api_key=cdap.get("api_key", ""),
            device_name=cdap.get("device_name", "Modbus Bridge"),
            device_type=cdap.get("device_type", "bridge"),
            bridge_name=cdap.get("bridge_name", "modbus"),
            bridge_version="1.0.0",
            heartbeat_sec=cdap.get("heartbeat_sec", 15),
        )

        self._build_widgets()
        self._register_handlers()

    # ── Widget construction ───────────────────────────────────────

    def _build_widgets(self) -> None:
        for reg in self.registers:
            wtype = reg.get("widget", "text")
            wid = reg["widget_id"]
            label = reg.get("label", wid)
            ro = reg.get("readonly", True)

            if wtype == "gauge":
                self.bridge.add_widget(gauge(wid, label, unit=reg.get("unit", ""), min_val=reg.get("min", 0), max_val=reg.get("max", 100)))
            elif wtype == "toggle":
                self.bridge.add_widget(toggle(wid, label))
            elif wtype == "slider":
                self.bridge.add_widget(slider(wid, label, min_val=reg.get("min", 0), max_val=reg.get("max", 100), step=reg.get("step", 1), unit=reg.get("unit", "")))
            else:
                self.bridge.add_widget(textWidget(wid, label, readonly=ro))

    def _register_handlers(self) -> None:
        for reg in self.registers:
            if reg.get("readonly", True):
                continue
            wid = reg["widget_id"]

            @self.bridge.on_command(wid)
            async def _handler(action: str, value: Any, *, _reg: dict = reg, **kw: Any) -> Any:
                return await self._write_register(_reg, value)

    # ── Modbus I/O ────────────────────────────────────────────────

    async def _connect_modbus(self) -> None:
        transport = self.mcfg.get("transport", "tcp")
        if transport == "tcp":
            from pymodbus.client import AsyncModbusTcpClient

            self.client = AsyncModbusTcpClient(
                host=self.mcfg["host"],
                port=self.mcfg.get("port", 502),
                timeout=self.mcfg.get("timeout_sec", 5),
            )
        else:
            from pymodbus.client import AsyncModbusSerialClient

            self.client = AsyncModbusSerialClient(
                port=self.mcfg.get("serial_port", "/dev/ttyUSB0"),
                baudrate=self.mcfg.get("serial_baudrate", 9600),
                parity=self.mcfg.get("serial_parity", "N"),
                stopbits=self.mcfg.get("serial_stopbits", 1),
                timeout=self.mcfg.get("timeout_sec", 5),
            )
        await self.client.connect()

    async def _read_register(self, reg: dict) -> Any:
        unit = self.mcfg.get("unit_id", 1)
        rtype = reg.get("type", "holding")
        addr = reg["address"]
        count = reg.get("count", 1)

        if rtype == "coil":
            result = await self.client.read_coils(addr, count, slave=unit)
            return bool(result.bits[0]) if not result.isError() else None
        elif rtype == "discrete":
            result = await self.client.read_discrete_inputs(addr, count, slave=unit)
            return bool(result.bits[0]) if not result.isError() else None
        elif rtype == "input":
            result = await self.client.read_input_registers(addr, count, slave=unit)
        else:
            result = await self.client.read_holding_registers(addr, count, slave=unit)

        if result.isError():
            return None

        return decode_registers(
            result.registers,
            reg.get("data_type", "uint16"),
            reg.get("scale", 1.0),
            reg.get("offset", 0.0),
        )

    async def _write_register(self, reg: dict, value: Any) -> Any:
        unit = self.mcfg.get("unit_id", 1)
        rtype = reg.get("type", "holding")
        addr = reg["address"]

        if rtype == "coil":
            result = await self.client.write_coil(addr, bool(value), slave=unit)
            return bool(value) if not result.isError() else None
        else:
            regs = encode_value(
                float(value),
                reg.get("data_type", "uint16"),
                reg.get("scale", 1.0),
                reg.get("offset", 0.0),
            )
            result = await self.client.write_registers(addr, regs, slave=unit)
            return float(value) if not result.isError() else None

    # ── Poll loop ─────────────────────────────────────────────────

    async def _poll_loop(self) -> None:
        interval = self.mcfg.get("poll_interval_sec", 3)
        await self._connect_modbus()
        logger.info("Modbus connected to %s:%s", self.mcfg.get("host", "serial"), self.mcfg.get("port", ""))

        while True:
            updates: dict[str, Any] = {}
            for reg in self.registers:
                try:
                    val = await self._read_register(reg)
                    if val is not None:
                        updates[reg["widget_id"]] = val
                except Exception as exc:
                    logger.warning("Read %s failed: %s", reg["widget_id"], exc)

            if updates:
                self.bridge.bulk_update(updates)

            await asyncio.sleep(interval)

    # ── Entry point ───────────────────────────────────────────────

    def run(self) -> None:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)

        # Start poll loop alongside CDAP bridge
        async def _main() -> None:
            poll_task = asyncio.create_task(self._poll_loop())
            cdap_task = asyncio.create_task(self.bridge._run_forever())
            done, pending = await asyncio.wait(
                [poll_task, cdap_task], return_when=asyncio.FIRST_EXCEPTION
            )
            for t in done:
                if t.exception():
                    logger.error("Task failed: %s", t.exception())
            for t in pending:
                t.cancel()

        try:
            loop.run_until_complete(_main())
        except KeyboardInterrupt:
            logger.info("Shutting down")
        finally:
            loop.close()


# ── CLI ───────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Yomie CDAP Modbus Bridge")
    parser.add_argument("--config", "-c", default="config.json", help="Path to config file")
    parser.add_argument("--log-level", "-l", default="INFO", choices=["DEBUG", "INFO", "WARNING", "ERROR"])
    args = parser.parse_args()

    logging.basicConfig(level=getattr(logging, args.log_level), format="%(asctime)s [%(name)s] %(levelname)s %(message)s")

    path = Path(args.config)
    if not path.is_file():
        logger.error("Config file not found: %s", path)
        sys.exit(1)

    with open(path, encoding="utf-8") as f:
        cfg = json.load(f)

    bridge = ModbusBridge(cfg)
    bridge.run()


if __name__ == "__main__":
    main()
