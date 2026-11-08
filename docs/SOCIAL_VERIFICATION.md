# Orbit 0.2 verification

2026-11-08, Windows x64 / Rust MSVC / Node 24. This is a prototype release. A successful build is not proof of external OAuth, public hosting availability or game-server admission.

## Tested with real implementations

| Check | Evidence / boundary |
|---|---|
| SIWE | Real k256 signatures and `siwe` verification; wrong domain/URI, corrupt signatures, expired challenges and replay rejected. Actual remote-peer challenge/proof exchange tested over NIP-17; mapping is stored only by the verifying peer. |
| Credentials | Actual Windows Credential Manager; separate installation-scoped Nostr/data/wallet keys, reloaded credential persistence, chunked OAuth-sized values and disconnect. Sandbox-denied credential access was rerun outside the restricted sandbox. An intermittent deletion assertion prompted serialized vault operations plus revocation before deletion; hardened tests passed. |
| Friends / messages | Two independent AppStates, SQLite databases, keyring namespaces and real Nostr SDK clients communicating over a loopback WebSocket relay. Signed invitation, acceptance, reopen/restore, text, relay acknowledgment, encrypted delivery receipt, blocking and profile updates passed. A supplied profile cannot inject a verified-wallet claim. |
| Offline recipient | Recipient disconnected, sender received relay acknowledgment and remained **sent**, recipient reconnected and received the retained event. Fixture relay stores events in memory while its process runs; this does not prove retention across a relay restart or public operator availability. |
| Large file | **1,073,741,824 plaintext bytes (1 GiB)** generated synthetically, encrypted/uploaded, downloaded from a preexisting one-third ciphertext partial, decrypted and SHA-256 compared to the original. Ciphertext was 1,074,004,213 bytes. This extended unoptimized test took about 637 seconds including photo/GIF/video/recovery work; it is not a release-performance benchmark. |
| Rich media | Generated 3000×2000 PNG (18,003,438 bytes), animated multi-frame GIF and H.264 MP4 transferred through encryption/storage/DM/retrieval. Original hashes matched; PNG dimensions and multiple GIF frames decoded. Optional local FFmpeg decoded the received MP4 successfully. JPEG compression was checked separately and preserved the original. |
| Transfer failures | Cancellation releases transfer handles; rejected upload can be retried; corrupted cached ciphertext is removed/redownloaded; resumed ranges and ciphertext/plaintext integrity checks passed. BUD-02 upload resumption is not implemented or claimed. |
| Recovery | Passphrase-encrypted age export, wrong-passphrase rejection without creating a profile, restore of identity/contact/history and queued outbox records. Media bytes and external OAuth sessions are excluded. |
| Activity protocol | Game and music records delivered independently without overwriting one another; expired activity disappears. Peer proof and accepted friendship persist after reopening SQLite. Actual Spotify authorization and Windows player compatibility remain separate unverified integrations. |
| Existing programs | Windows shortcut/icon/argument/working-directory import, harmless `where.exe` process start, favorites, collections, history, reopen persistence and missing-path errors passed again. |
| Live Roblox | Home again returned HTTP 401; real public Explore, selected live details/creators and thumbnail cache passed. The test selected a current service-provided universe rather than a hardcoded game. Runtime dispatch was previously verified in 0.1; admission to a Roblox server remains unobservable/unverified. |
| Minecraft | Existing 0.1 Vanilla/Fabric performance tests passed installation, managed Java, repair/range recovery and 35-second renderer launches. In the 0.2 regression, install/repair/Java and renderer initialization succeeded, but the game exited cleanly with code 0 before the 35-second assertion; the duration assertion therefore failed. No crash cause or successful 35-second rerun is claimed. |

The 1 GiB media test passed while a separate old credential-deletion assertion failed in the same run. After hardening credential removal, SIWE/credential tests passed, and the complete smaller transfer suite passed. The report distinguishes these outcomes instead of describing that initial combined run as entirely green.

## Reproduce

Final native suite: **10 passed, 0 failed, 3 opt-in live tests ignored**. TypeScript/Vite production build passed. The optimized Windows x64 executable and NSIS installer built successfully; silent installation into an isolated workspace directory exited with code 0 and produced the executable/uninstaller. Native Home, local profile creation, Profile and Connections were inspected in the installed build. A global sidebar stylesheet was moved out of the lazy social bundle after that inspection caught an initial Home layout inconsistency. Broader interactive coverage remains limited as listed below.

The installed Activity screen successfully read the current Firefox/YouTube Windows media session, displaying title, creator, artwork and playing state while sharing stayed off. This proves a local media-session read on that player, not music classification, other-device detection, Spotify authorization or continuous track-event updates.

```powershell
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture --test-threads=1
```

Tests use synthetic identities and files. Windows Credential Manager must be accessible to the process; do not replace it with plaintext storage to work around sandbox denial.

Extended test (allow at least 6 GiB of temporary disk for the source, ciphertext/provider copy, partial and plaintext; test/build overhead needs additional space):

```powershell
$env:ORBIT_SOCIAL_TEST_ROOT = "$PWD\.verification\social"
$env:ORBIT_TEST_GIB = '1'
# Optional local decoder, not downloaded or executed automatically by the test:
$env:ORBIT_TEST_FFMPEG = '<absolute path to ffmpeg.exe>'
cargo test --manifest-path src-tauri/Cargo.toml two_clients_friendship -- --nocapture
```

Run two app instances with separate data/credential namespaces for manual checks:

```powershell
.\orbit-launcher.exe --data-dir C:\OrbitTest\Alice
.\orbit-launcher.exe --data-dir C:\OrbitTest\Bob
```

Never use these test paths to overwrite an existing profile. Choose actual compatible relays/storage explicitly; network use is disabled initially. There is no shipped mock catalog, mock account authentication or mock delivery state. The loopback service exists only in Rust tests and is not packaged as a free hosting service.

## Unverified or externally blocked

- Microsoft/Minecraft, Spotify, Discord, Roblox identity and X need publisher registrations/access and real browser authorization. None was borrowed or invented. X is billable and remains disabled. Instagram personal OAuth is unavailable; manual links are labeled.
- Official Bluesky OAuth is implemented through Atrium (PKCE/PAR/DPoP), with an explicit localhost development mode until production metadata is configured. It has not been exercised with a real account; refresh/provider interoperability and public-post reading still require live authorization/manual checks.
- No selected public relay/storage operator has a verified service-level/retention guarantee. No free public encrypted GB-file provider has been established. The measured 1 GiB result belongs to the isolated test service only.
- Native Windows media-session metadata/track events vary by player and do not cover another device. Account-based Spotify playback and live Windows session behavior need interactive provider/player checks. Activity sharing is explicit and expires; automatic continuous presence is not claimed.
- MP4 decode/integrity passed, but inline WebView2 playback/codec support, galleries, resize, picker/drag-and-drop and full interactive installer/uninstaller coverage are not inferred from those tests.
- External wallet-extension approval, contract wallets, cross-device key delegation and full multi-device history synchronization are not verified/implemented. No transaction, payment, server provisioning, credit purchase or private user-file upload was performed.
- Independent security audit, public-provider compatibility/abuse/retention testing and production code signing remain required before treating this as a production messenger.

See [publisher configuration](PUBLISHER_REGISTRATION.md), [social setup](SOCIAL_SETUP.md), and the [0.1 baseline report](VERIFICATION.md).
