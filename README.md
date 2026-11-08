# Orbit Launcher

A lightweight, local-first Windows launcher built with **Rust, Tauri 2, React/TypeScript, and SQLite**. No Orbit account or hosted backend is required.

**Status: integration preview, not a feature-complete production release.** Roblox's personalized Home API is restricted, and Microsoft/Minecraft sign-in needs the publisher's own approved application registration. These limitations are visible in the app; no mock catalog or borrowed client ID is shipped.

## Download

Windows x64 downloads are published under [GitHub Releases](https://github.com/spokobot737-lab/orbit-launcher/releases). Use the `Orbit_0.1.0_x64-setup.exe` installer. A standalone `orbit-launcher.exe` is also provided; it needs Microsoft Edge WebView2 Runtime installed.

The build is unsigned. A production release should be code-signed. Roblox must already be installed and signed in through its official application.

## Features

- Home with local recent launches and pinned favorites; custom collections and unified library search.
- Live public Roblox Explore sections, search, experience details, creator/player/visit metadata, locally cached icons, and place-specific runtime dispatch. The app first requests Home and explains access restrictions; Explore is explicitly separate and opt-in.
- Direct modern Minecraft Java installation with managed Mojang Java runtimes, assets, libraries, natives, integrity checks, resumed partial downloads, separate profile game folders, and Vanilla/Fabric support.
- A Fabric performance preset for 1.21.1 with Sodium, Lithium, and FerriteCore, downloaded directly from Modrinth. No proprietary Lunar/Feather assets or code.
- System-browser Microsoft authorization-code authentication with PKCE, loopback callback/state validation, Xbox/XSTS exchange, Java entitlement/profile checks, and refresh on launch. Requires your own registered and permitted client ID; not yet verified with such an ID.
- Refresh credentials in Windows Credential Manager; access tokens remain in Rust memory. Neither is returned to the frontend or stored in SQLite.
- Explicit offline identities, independent of Microsoft accounts. Offline mode grants neither ownership nor authenticated multiplayer access.
- EXE/Windows shortcut import using the file picker or drag-and-drop, Windows shortcut argument parsing, extracted icons, image selection, editable arguments/working directory, and direct process launching.
- Installation progress, process exit status, bounded launch logs, repair, readable errors, and stay/minimize/exit behavior.

## Develop on Windows

1. Install Node.js 22.12+ (tested here with Node 24), Rust stable MSVC, Visual Studio 2022 Build Tools with **Desktop development with C++** and a Windows SDK, and Edge WebView2 Runtime. Restart the terminal after installing Rust.
2. Run:

```powershell
npm ci
npm run tauri -- dev
```

Frontend-only preview: `npm run dev`. It clearly indicates that native integrations are unavailable; it does not substitute mock data.

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build
```

Build outputs:

```text
src-tauri/target/release/orbit-launcher.exe
src-tauri/target/release/bundle/nsis/Orbit_0.1.0_x64-setup.exe
```

## Minecraft setup

Create a profile under Minecraft, select a version/loader, and save. Choose **Install / Repair**, wait for completion, then **Play**. An owned Java account is required for authenticated play; configure your own Microsoft client ID under Settings. See [external registrations and API limitations](docs/INTEGRATIONS.md).

The first validated version is **1.21.1 on Windows x64**. The version picker exposes modern stable releases from Mojang (1.20.5+); other releases are not all tested. Snapshots, legacy versions, ARM64, Forge/NeoForge, custom HUDs, and proprietary mod suites are outside this preview.

The performance preset keeps its version/loader fixed. Create a different profile for a different version. Adding arbitrary extra mods can affect compatibility. Reinstalling a profile does not delete worlds or user mods.

## Local data

Data lives in `%LOCALAPPDATA%\app.orbit.launcher\`:

```text
orbit.sqlite                  library, collections, history, settings, metadata cache
cache/images/                 cached Roblox thumbnails (7-day freshness)
minecraft/shared/             runtimes, libraries, versions, assets
minecraft/profiles/<uuid>/game separate worlds, mods, options for each profile
minecraft/profiles/<uuid>/orbit.log sanitized launch output (bounded)
```

Metadata TTLs are 5–10 minutes for Roblox search/discovery/details, one day for thumbnail metadata/runtime manifests, and one hour for version metadata. Public Explore can display stale cached discovery data when offline. Thumbnail files are refreshed lazily. Cache size management is not implemented; nothing scans the disk continuously.

Removing a library entry preserves installed game files and worlds. Launch history records successful process starts or Roblox protocol dispatch, not verified entry into a game server. Exiting Orbit leaves games running and ends Orbit's process monitoring.

## Verification

See [verification report](docs/VERIFICATION.md) for tested functionality and remaining gaps. Opt-in live checks download real game files into an explicitly chosen directory:

```powershell
$env:ORBIT_TEST_ROOT = "$PWD\.verification"
$env:ORBIT_TEST_LAUNCH = '1'
cargo test --manifest-path src-tauri/Cargo.toml live_minecraft_install_repair_and_launch -- --ignored --nocapture

$env:ORBIT_TEST_LOADER = 'fabric'
$env:ORBIT_TEST_PERFORMANCE = '1'
cargo test --manifest-path src-tauri/Cargo.toml live_minecraft_install_repair_and_launch -- --ignored --nocapture
```

The launch check starts its own offline test client, observes it for 35 seconds, checks renderer initialization, then stops only that test process. It also corrupts its own test client JAR and creates a half-complete `.part` to verify repair and resumption. This test folder is ignored by Git.

## Architecture

The frontend calls a bounded operation router (`api`) with operation-specific validation. Rust modules separate storage (`db`), HTTP/downloads (`net`), Roblox, Microsoft authentication, Minecraft, performance mods, and desktop programs. SQL statements are parameterized. Executables are launched directly with an argument vector, never a user-built shell command. PowerShell is used only for a fixed Windows COM/icon extraction script, with the selected path supplied as an environment variable.

Network operations use HTTPS host allowlists, connection/request timeouts, at most four concurrent file/API operations, and bounded API retry/backoff for 429/5xx. Downloads use `.part` staging, range validation and SHA-1 verification before promotion. Unsupported metadata, unavailable runtime components, missing files and service restrictions surface as errors. Tauri exposes dialog/core permissions only; no generic frontend shell, HTTP or filesystem plugin is granted. Roblox UI code and thumbnail work load on demand.

## Attribution

Orbit is independent of Roblox, Mojang, Microsoft, Fabric, Modrinth, Lunar, and Feather. Game metadata/art comes directly from Roblox. Minecraft/Java files and mods download from their respective upstream services at install time and are not bundled or rehosted in releases. Decorative interface art and the Orbit logo are original code/vector artwork. Icons use Lucide (ISC). Third-party dependencies retain their respective licenses.
