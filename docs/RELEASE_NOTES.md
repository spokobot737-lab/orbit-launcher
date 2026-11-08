# Orbit 0.1.0 — Windows integration preview

Download `Orbit_0.1.0_x64-setup.exe` for installation, or `orbit-launcher.exe` for a standalone executable (requires WebView2). `SHA256SUMS.txt` contains file checksums. This build is unsigned.

## Tested

- Production TypeScript/Vite build and six Rust unit tests.
- Minecraft Java 1.21.1: official downloads, managed Java 21, file integrity checks, repair/resumption from a half-complete client JAR, and direct offline launch with renderer initialization.
- Fabric performance preset: Fabric Loader 0.20.1, Sodium 0.8.13, Lithium 0.15.4, FerriteCore 7.0.3; successful direct offline launch and renderer initialization. These versions are pinned. No FPS benchmark is claimed.
- Windows shortcut import with Unicode filename, arguments, working directory and icon extraction. Direct native process launch, SQLite persistence, favorites, collections, history and missing-executable errors.
- Live Roblox public Explore, search, details/creator metadata and thumbnail disk caching. A live-selected free experience was dispatched to its root place through the registered official runtime protocol. Successful server entry is not verified.
- Native Home/Programs layouts inspected and native file picker opened. User stopped further Computer Use with Escape; full UI/drag-and-drop/resize testing remains incomplete.
- NSIS installer built and silent installation returned exit code 0. The installed executable matched the build apart from Tauri's expected bundle-type marker. Interactive uninstall remains untested.

## External limitations

- **Roblox personalized Home is blocked:** its live endpoint returned HTTP 401/error 9002. No supported Open Cloud OAuth scope for the website Home feed was established. Public Explore is a separately labeled, explicit alternative, never passed off as personalized Home.
- **Microsoft sign-in needs publisher registration:** create your own public client ID and obtain Minecraft API permission. PKCE/browser authentication, entitlement/profile checks and credential refresh are implemented but not tested end to end with an approved application. No borrowed client ID, password field, cookie input or bundled secret is used.
- Other modern Minecraft releases are selectable but unverified. Legacy versions and snapshots are unsupported. Exiting Orbit leaves games running and stops process monitoring. Cache quota management and a durable/cancelable download queue are not included.

This preview does **not** represent completion of the requested personalized Roblox Home integration or verified Microsoft authentication. Read [setup and registration documentation](https://github.com/spokobot737-lab/orbit-launcher/blob/main/docs/INTEGRATIONS.md) and [the verification report](https://github.com/spokobot737-lab/orbit-launcher/blob/main/docs/VERIFICATION.md) before use.
