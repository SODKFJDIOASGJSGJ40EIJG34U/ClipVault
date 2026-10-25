# ClipVault

**ClipVault** is a local-first clipboard history manager for Windows, built with Tauri 2, React, TypeScript, Rust, and SQLite. The interface is Russian by default and can be switched to English.

## Current status

This repository is an early, working foundation, not a release-ready product. The Windows event listener captures text, bitmap images, and file-drop lists. Text, images, and file lists can be restored to the clipboard. History, search, filters, favorites, snippets, settings, source-process exclusions, Windows' `ExcludeClipboardContentFromMonitorProcessing` signal, the tray menu, startup option, and global shortcuts are implemented. Source application paths are abbreviated in cards; a separate action copies the full path. Release builds do not open a console window. Automatic paste, comprehensive migration management, signed release packages, and in-app runtime verification are still outstanding. Do not use the current version as a security boundary for secrets.

The application does not make network requests by design. All history is stored in a SQLite file in the app data directory. No account or telemetry is configured. The UI uses system fonts and works offline.

## Development

Prerequisites: Node.js 20+, Rust stable with the MSVC target, Microsoft C++ Build Tools, and the WebView2 runtime. On Windows:

```powershell
npm install
npm run tauri dev
```

Frontend production build:

```powershell
npm run build
```

Build Windows installers (NSIS and MSI):

```powershell
npm run tauri build
```

Tauri writes packages under `src-tauri/target/release/bundle/`. The locally built, unsigned installers are `src-tauri/target/release/bundle/nsis/ClipVault_0.1.0_x64-setup.exe` and `src-tauri/target/release/bundle/msi/ClipVault_0.1.0_x64_en-US.msi`. The GitHub Actions workflow builds both and attaches them to a published GitHub release.

## Data and privacy

The SQLite database is created under Tauri's per-user application data directory. No administrator privileges are requested. Clipboard contents can include sensitive material; pause monitoring or exit ClipVault before copying secrets. The app currently does not encrypt the database. The configured retention and history limits are applied when settings are saved.

## Keyboard shortcut

`Ctrl+Shift+V` shows the main history window. If that key combination is already registered by another application, ClipVault falls back to `Ctrl+Alt+V`; the tray menu always remains available. Selecting an entry restores content to the clipboard; it does not synthesize a paste keystroke.

## Screenshots

See [`docs/screenshots/README.md`](docs/screenshots/README.md) for the screenshot status and capture instructions. Checked-in UI screenshots will be added after a Windows release build is available.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Contributions should preserve offline behavior, Unicode correctness, and per-user operation without administrator privileges.

## License

MIT — see [LICENSE](LICENSE).
