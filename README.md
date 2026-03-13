# MobileHero Launcher 🎸

The PC companion app for **MobileHero**, turning your smartphone into a virtual guitar controller for Clone Hero and other rhythm games.

**WE NEED BETA TESTERS!** https://forms.gle/8qv6XRVjwfFsTohA6

![MobileHero Launcher Preview](https://i.imgur.com/6f9oFet.png)

## Features
- **Easy Connection**: Generates a QR code for instant pairing with the MobileHero app.
- **Low Latency**: Uses high-performance WebSockets for minimal input delay.
- **Secure**: PIN-based authentication to prevent unauthorized connections.
- **Configurable**: Custom keybindings and server settings.
- **Cross-Platform**: Supports Windows and Linux.

## Prerequisites

### Windows
- **Node.js** (v18 or newer)
- **Rust** (for building the backend)
- **WebView2** (pre-installed on most modern Windows systems)

### Linux
- **Node.js** (v18 or newer)
- **Rust** (for building the backend)
- The following system libraries:
  ```bash
  sudo apt install libwebkit2gtk-4.1-dev libxdo-dev \
    build-essential curl wget file libssl-dev \
    libayatana-appindicator3-dev librsvg2-dev
  ```
  > On Fedora/RHEL: `sudo dnf install webkit2gtk4.1-devel libxdo-devel openssl-devel`

## Installation

### From Binary (Recommended)
Download the latest release from the [Releases page](https://github.com/nothing2me/MobileHeroLauncher/releases):
- **Windows**: `MobileHeroLauncher_x64-setup.exe` or `.msi`
- **Linux**: `MobileHeroLauncher_amd64.AppImage` (universal) or `.deb` (Debian/Ubuntu)

For AppImage on Linux:
```bash
chmod +x MobileHeroLauncher_*.AppImage
./MobileHeroLauncher_*.AppImage
```

### From Source
1. Clone this repository:
    ```bash
    git clone https://github.com/nothing2me/MobileHeroLauncher.git
    cd MobileHeroLauncher
    ```

2. Install dependencies:
    ```bash
    npm install
    ```

3. Run in development mode:
    ```bash
    npm run tauri dev
    ```

## Building for Production

```bash
npm run tauri build
```

Output locations:
- **Windows**: `src-tauri/target/release/bundle/nsis/` (`.exe`) and `/msi/` (`.msi`)
- **Linux**: `src-tauri/target/release/bundle/appimage/` (`.AppImage`) and `/deb/` (`.deb`)

## Usage
1. Open **MobileHero Launcher** on your PC.
2. Click **Start Server**.
3. Open the **MobileHero App** on your phone.
4. Scan the **QR Code** displayed in the launcher.
5. Start rocking! 🤘

## Troubleshooting

**Windows Firewall**: Ensure "MobileHero Launcher" is allowed through your Windows Firewall on the WebSocket port (default: 8080).

**Linux Firewall (ufw)**:
```bash
sudo ufw allow 8080/tcp
```

**Network**: Both devices must be on the **same Wi-Fi network**.

**Linux / Wayland**: The launcher works on both X11 and Wayland (via XWayland). Clone Hero itself runs under XWayland, so keyboard input is routed correctly.

## License
[MIT](LICENSE)
