# Verification report

Date: 2026-11-08. Target: Windows x64, Node 24, Rust stable MSVC, Tauri 2.

This is an integration preview. A complete production release satisfying every requested feature is blocked by personalized Roblox Home access and the missing publisher Microsoft registration.

## Completed checks

- `npm run build`: TypeScript check and production Vite build passed. Roblox UI is a separate lazy-loaded chunk. No network fonts, continuous animations, or mock library data.
- Native Rust compile check passed.
- Rust unit tests passed for Windows rule selection, deterministic offline identity, unsupported argument rejection, credential-line redaction, host/path restrictions, and Windows shortcut argument parsing.
- Live Roblox Home request: HTTP 401 / error 9002, authentication token missing. Public Explore request: HTTP 200, real service-provided experience sections. Live search returned actual paginated Game content with universe/root-place IDs.
- The installed official Roblox runtime's Windows protocol registration exists on the test machine.
- Live Minecraft Vanilla 1.21.1 installation: downloaded official metadata, game JAR, assets, libraries and Microsoft OpenJDK 21.0.7 from Mojang's runtime manifest. Managed Java executed successfully.
- Recovery test: deliberately corrupted the test client and wrote its first half to `.jar.part`. Reinstallation resumed/reverified it and restored the exact original bytes.
- Direct offline Vanilla launch: Java process remained running for the 35-second check and initialized LWJGL/rendering/texture atlases. No other Minecraft launcher was invoked. Offline Realms errors are expected and do not establish authenticated play.
- Fabric performance installation/repair/direct launch also passed the 35-second renderer check. Verified versions: Fabric Loader 0.20.1, Sodium 0.8.13+mc1.21.1, Lithium 0.15.4+mc1.21.1, FerriteCore 7.0.3. The preset now pins these exact tested versions in `performance-profile.json`; game logs confirmed all three mods loaded.
- Native Windows shortcut import passed with a Unicode filename, argument vector preservation, working directory, and extracted PNG icon. Direct process creation was checked using the harmless Windows `where.exe` utility.
- SQLite reopen/persistence checks passed for favorites, collection membership and launch history. Removal cascaded collection membership correctly. A missing executable produced the expected readable error before process creation.
- Real Roblox details/creator metadata and local thumbnail file caching passed. A free experience selected from the live Top Trending section was dispatched with its service-resolved root place ID. Protocol dispatch succeeded; runtime/server entry remains unverified.
- Visually inspected the native Home/Programs layouts and opened the Windows executable picker. Further UI automation was stopped by the user with Escape; full interactive UI/resize/drag-and-drop coverage is not claimed.
- Produced the optimized Windows x64 EXE and a 3.01 MiB NSIS installer. A silent installation into an isolated workspace directory exited with code 0 and created the executable/uninstaller. Binary comparison found only Tauri's expected three-byte `NSS` versus `UNK` bundle-type marker; the remainder exactly matched the standalone release. SHA-256 checksums accompany the downloads. Full interactive installer/uninstaller testing was not performed.

## Still requires external setup

- Microsoft registration, Minecraft API approval, and a Java-owning test account. Browser OAuth, account entitlement/profile checks, Credential Manager storage and refresh are implemented but have not been exercised end to end with an approved client ID.
- Personalized Roblox Home: unavailable through a supported documented third-party authorization path established here. Explore is a separate explicit option, not claimed as Home.

## Scope limits

- Only Windows x64 and Minecraft 1.21.1 are integration-tested. Other modern releases are discoverable, but not exhaustively verified. Legacy versions and snapshots are unsupported.
- No performance benchmark, FPS guarantee, world-generation soak test, multiplayer authentication test, or custom in-game HUD is claimed.
- The installer and executable are unsigned. No auto-updater is included.
- Metadata and thumbnail freshness are bounded; cache disk quota/eviction UI is not included.
- A launch record means process creation or protocol dispatch; it is not proof a Roblox experience admitted the player. Programs that immediately hand off to another process can outlive the tracked parent.
- Installations retry/repair from disk after interruption. The Downloads session list is not a durable job queue, and there is no cancellation control.
