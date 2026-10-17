# Project Static Assets

Contains global static assets, branding resources, and icons used by Personal Notepad.

## Assets Structure
- `assets/app-icon.png`: Authoritative master 1024x1024 RGBA icon with transparent background.
- `public/app-icon.png`: Webview/favicon asset referenced by `index.html`.
- `src-tauri/icons/`: Multi-resolution platform icon suite (16x16 through 512x512 PNG, `.ico`, `.icns`) generated from `assets/app-icon.png`.
- `scripts/generate-icons.py`: Utility script for master icon extraction and FreeDesktop sizing.

