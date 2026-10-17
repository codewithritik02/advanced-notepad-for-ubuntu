# Linux Platform Layer & Ubuntu Distribution

This directory isolates Linux-specific behaviors, desktop configurations, packaging scripts, and system integrations for **Personal Notepad** on Ubuntu and Debian-based Linux distributions.

---

## Supported Releases & Architecture
- **Target Distribution**: Ubuntu 24.04 LTS (Noble Numbat) and Ubuntu 26.04 LTS
- **Architecture**: `x86_64` (Debian `amd64`)
- **Deliverable Package**: `personal-notepad_0.1.0_amd64.deb` (`4.47 MiB`)

---

## Building the Debian Package

### 1. Install Build Dependencies
```bash
sudo apt-get update && sudo apt-get install -y \
  build-essential \
  curl \
  wget \
  file \
  pkg-config \
  libglib2.0-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libsoup-3.0-dev \
  libcairo2-dev \
  libgdk-pixbuf-2.0-dev \
  libdbus-1-dev \
  xdg-utils
```

### 2. Compile and Package
```bash
# Clean install of pinned node dependencies
npm ci

# Build release bundle (.deb)
npm run package:linux
```

The package output is generated at:
`src-tauri/target/release/bundle/deb/personal-notepad_0.1.0_amd64.deb`

---

## Installation & Launch

### Install via APT (Recommended - automatically resolves dependencies)
```bash
sudo apt install ./personal-notepad_0.1.0_amd64.deb
```

### Install via DPKG
```bash
sudo dpkg -i personal-notepad_0.1.0_amd64.deb
# If any dependencies are missing:
sudo apt-get install -f
```

### Launch Application
- From the desktop application menu: click **Personal Notepad**.
- From the terminal:
  ```bash
  personal-notepad
  ```

---

## Filesystem Layout & Separation

| Path | Purpose | Ownership / Mode |
| :--- | :--- | :--- |
| `/usr/bin/personal-notepad` | Application executable | `root:root` (`0755`) |
| `/usr/share/applications/Personal Notepad.desktop` | Desktop launcher | `root:root` (`0644`) |
| `/usr/share/icons/hicolor/*/apps/personal-notepad.png` | FreeDesktop icons | `root:root` (`0644`) |
| `~/.local/share/com.personalnotepad.app/` | User notes, database, attachments, backups | `$USER:$USER` (`0755`) |

---

## User Data Preservation & Uninstall Policy

- **Package Removal**:
  ```bash
  sudo apt remove personal-notepad
  ```
  Removes exclusively `/usr/bin/personal-notepad`, `/usr/share/applications/Personal Notepad.desktop`, and `/usr/share/icons/`.
- **Zero Data Loss Guarantee**: All user databases (`database.sqlite`), note bodies, attachments, backups, and encryption keys strictly reside in `$XDG_DATA_HOME/com.personalnotepad.app/` (`~/.local/share/com.personalnotepad.app/`). They are **never** deleted or overwritten during package installation, upgrade, or removal.
- **Manual Data Deletion**: If a user wishes to permanently purge all notes and configuration:
  ```bash
  rm -rf ~/.local/share/com.personalnotepad.app
  ```
