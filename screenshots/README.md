# Screenshots Placeholder

This directory contains screenshots of Yomie Console for documentation purposes.

## Required Screenshots

For the README.md, please add the following screenshots:

1. **dashboard.png** - Main dashboard view showing:
   - Navigation bar with logo and stats
   - Four statistics cards (Total, Active, Inactive, With Notes)
   - Device list table
   - Search functionality

2. **devices-list.png** - Device management view showing:
   - Horizontal folder chips bar (scrollable pills)
   - Unified toolbar with search, segmented filters, column toggle
   - Slim table with status dot, device ID, hostname, platform
   - Kebab context menu (⋮) per row

3. **device-details.png** - Device details modal showing:
   - Device ID
   - UUID
   - Public Key
   - Status badge
   - Note
   - Created timestamp
   - Additional metadata

4. **mobile-view.png** - Mobile responsive view (≤600px) showing:
   - Card-style device rows (CSS grid 2-column layout)
   - Bottom sheet kebab menu with backdrop overlay
   - Compact folder chips (icon-only at ≤400px)
   - Touch-friendly controls with larger tap targets

## How to Create Screenshots

### Using the Demo App

```bash
cd web
python3 app_demo.py
```

Then open `http://localhost:5001` in your browser.

### Taking Screenshots

**Recommended Settings**:
- Browser: Chrome/Firefox (latest version)
- Window Size: 1920x1080 (for desktop) / 375x667 (for mobile)
- Zoom: 100%
- Theme: Dark mode enabled

**Tools**:
- Built-in browser screenshot (F12 → ... → Screenshot)
- macOS: Cmd+Shift+4
- Windows: Win+Shift+S
- Linux: Flameshot, GNOME Screenshot

### File Naming Convention

Use lowercase with hyphens:
- `dashboard.png`
- `devices-list.png`
- `device-details.png`
- `mobile-view.png`

### Image Requirements

- Format: PNG
- Max size: 2MB per image
- Compression: Optimized (use TinyPNG or similar)
- DPI: 72 (web)

## Privacy Note

The demo app (`app_demo.py`) generates fake data to protect real device information in screenshots.
