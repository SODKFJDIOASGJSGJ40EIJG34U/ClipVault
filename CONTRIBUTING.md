# Contributing to ClipVault

Thanks for helping improve ClipVault. Keep changes focused and document any behavior that affects clipboard privacy.

## Local setup

Install Node.js 20+, Rust stable with MSVC, Microsoft C++ Build Tools, and WebView2. Run `npm install`, then `npm run tauri dev`.

## Before submitting

- Run `npm run build` for frontend changes.
- Run `cargo test` and `cargo check` from `src-tauri` for Rust changes.
- Do not add network calls, telemetry, or administrator requirements without an explicit design discussion.
- Add tests for clipboard parsing, duplicate suppression, SQLite migrations, and Unicode handling when changing those areas.
- Never include real clipboard contents, personal paths, or credentials in fixtures, screenshots, or logs.

Use conventional commit messages where practical. By contributing, you agree that your contributions are licensed under the MIT License.
