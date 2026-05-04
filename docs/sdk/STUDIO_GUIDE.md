# CDAP SDK Studio Guide

> **Status:** The SDK Studio visual builder is planned for Phase 14 of the Yomie 3.0 roadmap.
> This document describes the intended feature set and workflow.

## Overview

CDAP SDK Studio is a browser-based visual tool for creating CDAP agents and bridges
without writing code. It generates ready-to-deploy Python or Node.js projects.

## Planned Features

### Widget Designer
- Drag-and-drop widget palette (8 widget types)
- Visual property editor (label, group, unit, thresholds)
- Live preview of widget rendering
- Widget grouping and layout

### Capability Builder
- Toggle available capabilities (telemetry, commands, terminal, file_transfer, clipboard, audio)
- Per-capability configuration forms
- Dependency warnings (e.g., terminal requires shell access)

### Command Editor
- Define custom commands with parameters
- Set confirmation requirements and cooldown periods
- Map commands to widget interactions (button → command)

### Code Generator
- Generates complete project structure (Python or Node.js)
- Includes: main script, config.json, requirements.txt / package.json, systemd unit / NSSM script
- Download as ZIP or push directly to Git repository
- Code is fully editable after generation

### Test Sandbox
- Connect generated agent to a test CDAP gateway
- Simulate widget state updates
- Send test commands and verify responses
- View message log in real time

## Workflow

1. **Create** — Open Studio, name the agent, pick language
2. **Design** — Add widgets, configure capabilities, define commands
3. **Preview** — See live widget rendering in browser
4. **Generate** — Download project ZIP
5. **Deploy** — Run agent on target machine, verify in panel

## File Structure (Generated Python Project)

```
my-agent/
├── main.py            # Agent entry point
├── config.json        # Server URL, device ID, API key
├── requirements.txt   # yomie-cdap + dependencies
└── install.sh         # systemd service installer
```

## File Structure (Generated Node.js Project)

```
my-agent/
├── index.js           # Agent entry point
├── config.json        # Server URL, device ID, API key
├── package.json       # yomie-cdap + dependencies
└── install.ps1        # NSSM service installer (Windows)
```
