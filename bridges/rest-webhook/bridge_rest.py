#!/usr/bin/env python3
"""Yomie CDAP — REST / Webhook Bridge.

Polls HTTP endpoints and/or listens for incoming webhooks.  Pushes values
to Yomie via CDAP, and writes back to REST APIs on command.

Usage:
    pip install yomie-cdap aiohttp
    python bridge_rest.py --config config.json
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import sys
from pathlib import Path
from typing import Any

import aiohttp
from aiohttp import web

from betterdesk_cdap import CDAPBridge, gauge, toggle, textWidget

logger = logging.getLogger("bridge_rest")

# ── JMESPath-lite extraction ──────────────────────────────────────────


def extract_path(data: Any, path: str) -> Any:
    """Simple dot-notation path extractor (no full JMESPath dependency)."""
    parts = path.split(".")
    current = data
    for part in parts:
        if isinstance(current, dict):
            current = current.get(part)
        elif isinstance(current, (list, tuple)) and part.isdigit():
            idx = int(part)
            current = current[idx] if idx < len(current) else None
        else:
            return None
        if current is None:
            return None
    return current


# ── Bridge ────────────────────────────────────────────────────────────


class RESTBridge:
    """REST/Webhook ↔ CDAP bridge."""

    def __init__(self, cfg: dict[str, Any]):
        self.cfg = cfg
        self.hcfg = cfg.get("http", {})
        self.sources: list[dict] = cfg.get("sources", [])
        self.webhooks: list[dict] = cfg.get("webhooks", [])
        self._session: aiohttp.ClientSession | None = None

        cdap = cfg["cdap"]
        self.bridge = CDAPBridge(
            server=cdap["server"],
            auth_method="api_key",
            api_key=cdap.get("api_key", ""),
            device_name=cdap.get("device_name", "REST Bridge"),
            device_type=cdap.get("device_type", "bridge"),
            bridge_name=cdap.get("bridge_name", "rest"),
            bridge_version="1.0.0",
            heartbeat_sec=cdap.get("heartbeat_sec", 15),
        )

        self._build_widgets()
        self._register_handlers()

    # ── Widgets ───────────────────────────────────────────────────

    def _build_widgets(self) -> None:
        for src in self.sources:
            wtype = src.get("widget", "text")
            wid = src["widget_id"]
            label = src.get("label", wid)

            if wtype == "gauge":
                self.bridge.add_widget(gauge(wid, label, unit=src.get("unit", ""), min_val=src.get("min", 0), max_val=src.get("max", 100)))
            elif wtype == "toggle":
                self.bridge.add_widget(toggle(wid, label))
            else:
                self.bridge.add_widget(textWidget(wid, label))

        for wh in self.webhooks:
            wid = wh["widget_id"]
            self.bridge.add_widget(textWidget(wid, wh.get("label", wid)))

    def _register_handlers(self) -> None:
        for src in self.sources:
            if not src.get("write_url"):
                continue
            wid = src["widget_id"]

            @self.bridge.on_command(wid)
            async def _handler(action: str, value: Any, *, _src: dict = src, **kw: Any) -> Any:
                return await self._write_source(_src, action, value)

    # ── HTTP polling ──────────────────────────────────────────────

    async def _poll_source(self, src: dict) -> Any:
        method = src.get("method", "GET").upper()
        headers = src.get("headers", {})
        url = src["url"]

        async with self._session.request(method, url, headers=headers) as resp:
            if resp.status >= 400:
                logger.warning("HTTP %d from %s", resp.status, url)
                return None
            data = await resp.json()
            path = src.get("jmespath", "")
            return extract_path(data, path) if path else data

    async def _write_source(self, src: dict, action: str, value: Any) -> Any:
        """Write command back to REST API."""
        url = src["write_url"]

        # Resolve {action} placeholder for HA-style services
        if src.get("widget") == "toggle":
            act = src.get("on_action", "turn_on") if value else src.get("off_action", "turn_off")
            url = url.replace("{action}", act)

        method = src.get("write_method", "POST").upper()
        headers = src.get("write_headers", src.get("headers", {}))
        body = src.get("write_body", {})

        async with self._session.request(method, url, headers=headers, json=body) as resp:
            if resp.status >= 400:
                logger.warning("Write HTTP %d to %s", resp.status, url)
                return None
            return value

    async def _poll_loop(self) -> None:
        default_interval = self.hcfg.get("poll_interval_sec", 10)
        self._session = aiohttp.ClientSession()
        logger.info("REST polling started (%d sources)", len(self.sources))

        # Group sources by their poll interval
        intervals: dict[int, list[dict]] = {}
        for src in self.sources:
            iv = src.get("poll_interval_sec", default_interval)
            intervals.setdefault(iv, []).append(src)

        async def _poll_group(sources: list[dict], interval: int) -> None:
            while True:
                updates: dict[str, Any] = {}
                for src in sources:
                    try:
                        val = await self._poll_source(src)
                        if val is not None:
                            wtype = src.get("widget", "text")
                            if wtype == "toggle":
                                val = str(val).lower() in ("on", "true", "1")
                            elif wtype == "gauge":
                                try:
                                    val = float(val)
                                except (ValueError, TypeError):
                                    continue
                            else:
                                val = str(val)
                            updates[src["widget_id"]] = val
                    except Exception as exc:
                        logger.warning("Poll %s failed: %s", src["widget_id"], exc)
                if updates:
                    self.bridge.bulk_update(updates)
                await asyncio.sleep(interval)

        tasks = [asyncio.create_task(_poll_group(srcs, iv)) for iv, srcs in intervals.items()]
        await asyncio.gather(*tasks)

    # ── Webhook listener ──────────────────────────────────────────

    async def _start_webhook_server(self) -> None:
        if not self.webhooks:
            return

        port = self.hcfg.get("webhook_port", 8090)
        path = self.hcfg.get("webhook_path", "/webhook")

        async def handle_webhook(request: web.Request) -> web.Response:
            try:
                data = await request.json()
            except Exception:
                return web.Response(status=400, text="invalid json")

            for wh in self.webhooks:
                match_field = wh.get("match_field", "")
                match_value = wh.get("match_value", "")
                if match_field and str(extract_path(data, match_field)) != str(match_value):
                    continue

                value_field = wh.get("value_field", "")
                val = extract_path(data, value_field) if value_field else json.dumps(data)
                self.bridge.update_state(wh["widget_id"], str(val))

                if wh.get("alert_severity"):
                    self.bridge.fire_alert(
                        f"wh_{wh['widget_id']}",
                        wh["alert_severity"],
                        wh.get("alert_message", str(val)),
                    )

            return web.Response(status=200, text="ok")

        app = web.Application()
        app.router.add_post(path, handle_webhook)
        runner = web.AppRunner(app)
        await runner.setup()
        site = web.TCPSite(runner, "0.0.0.0", port)
        await site.start()
        logger.info("Webhook server listening on 0.0.0.0:%d%s", port, path)

    # ── Entry point ───────────────────────────────────────────────

    def run(self) -> None:
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)

        async def _main() -> None:
            await self._start_webhook_server()
            tasks = [
                asyncio.create_task(self._poll_loop()),
                asyncio.create_task(self.bridge._run_forever()),
            ]
            done, pending = await asyncio.wait(tasks, return_when=asyncio.FIRST_EXCEPTION)
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
            if self._session:
                loop.run_until_complete(self._session.close())
            loop.close()


# ── CLI ───────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Yomie CDAP REST/Webhook Bridge")
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

    bridge = RESTBridge(cfg)
    bridge.run()


if __name__ == "__main__":
    main()
