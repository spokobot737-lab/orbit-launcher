# Minecraft hosting verification

## Orbit 0.4.1 verification - 2026-11-08

- Native suite: **18 passed, 0 failed, 10 opt-in checks ignored**. The new check covers nonpremium properties, deterministic exact-case nickname UUIDs without Mojang lookup, approval/removal, malformed-mode rejection, confirmation and separate allowlist preservation when switching modes. Encrypted-card tests now check nonpremium client filtering/creation and legacy premium behavior.
- Real Minecraft Java **1.21.1 client and dedicated server**, managed Java 21, no Microsoft session: unapproved `OrbitGuest` was refused, then the approved offline UUID entered the world. The first local lifecycle passed in about 224 seconds including fresh client downloads. Client/server were stopped and all dimensions saved.
- A second real-client run passed in about **66 seconds** with the approved client connecting through the public playit hostname. The client's own log confirms the playit destination and the server logs actual world entry. This is gameplay admission through the tunnel **from the host PC**, not an independent-network player test. Both processes were stopped afterward.
- Real free allocation: the self-managed Agent-Key created a **global Minecraft Java tunnel** with loopback origin port 25881; the returned tunnel matched the restricted selector. Only that newly created test tunnel was deleted afterward. This passed in about 8 seconds. The newer official SDK `/v1/tunnels/create` schema was rejected with a parse-body validation error; production uses the working official `/tunnels/create` method.
- Source includes pending-allocation reuse, serialized creation and a five-minute cooldown after unconfirmed responses; no purchase/premium allocation or account API key is requested. Provider quotas, verification requirements, rate limits and outages remain external limits.
- Normal installations need their own one-time browser authorization; the isolated test credential is not copied into user data. No existing user world, account, server authentication setting or installed executable was silently changed by these tests.
- Not verified: two simultaneous real players, a second machine/network, every provider failure/quota scenario, authenticated Premium client entry or a complete installer/interactive UI matrix. Nonpremium nickname impersonation remains inherent to unauthenticated Vanilla; an allowlist does not verify identity. Modpacks and custom game authentication plugins are outside this release.

Reproduce the additional checks in an explicitly authorized isolated test installation:

```powershell
$env:ORBIT_SERVER_EULA = 'accepted'
$env:ORBIT_SERVER_TEST_ROOT = "$PWD\.verification\hosting"
cargo test --manifest-path src-tauri/Cargo.toml live_nonpremium_java_client_joins_hosted_server -- --ignored --nocapture
# With an already-authorized test playit agent and a matching tunnel:
$env:ORBIT_TEST_PUBLIC_JOIN = '1'
cargo test --manifest-path src-tauri/Cargo.toml live_nonpremium_java_client_joins_hosted_server -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml live_free_self_managed_tunnel_creation -- --ignored --nocapture
```

## Historical 0.4.0 verification

2026-11-08, Windows x64, Rust MSVC, Node 24. All worlds/files were synthetic and isolated under ignored verification directories. The user explicitly accepted Minecraft EULA for the tests and authorized use of their playit account for browser setup. No private world, Minecraft account, wallet, paid plan or remote game server was used or provisioned.

## Verified

| Area | Evidence and limit |
|---|---|
| Existing functionality | Full native suite: **17 passed, 0 failed, 8 opt-in tests ignored**. Includes previous credential, identity/wallet preservation, friendship, encryption/receipts, offline delivery, media integrity/resume/recovery, SIWE, expiring activity and program persistence checks. |
| Dedicated Vanilla and Fabric | Real Minecraft **1.21.1 Vanilla and bare Fabric Loader 0.16.14** started through Orbit's backend using managed Java 21. Both returned protocol **767** status, saved all dimensions on graceful Stop, restarted, and retained world data. Final combined live test passed in about 135 seconds. |
| Installation repair | Mojang metadata/server digests and Fabric Maven digests verified. An intentionally corrupted synthetic server JAR and an 8 MiB partial were recovered; the repaired installation restarted successfully. This establishes recovery, not a claim that every upstream always supports byte-range resumption. |
| EULA and port handling | Missing acceptance and a forged acceptance timestamp are rejected; an occupied TCP port is rejected before Java startup. Authentication, allowlist enforcement, disabled RCON/query/JMX and loopback binding for playit are asserted. Stop also cancels a server waiting behind another installation lock. |
| World safety | Real stopped-world backup/restore passed for both loaders; Vanilla world import preserved the original. A separate test rejects an active source lock, another Orbit management lock, modded metadata and wrong DataVersion without changing the existing target. Active-server raw backups are refused. Restoration retains a safety backup/previous directory. |
| Encrypted invitations | Two independent synthetic Orbit profiles completed mutual friendship over the local fixture using real NIP-17 cryptography. A structured server card appeared in messages and the shared-server index; wrong-author and older updates were rejected, expired targets could not be joined. Offline and unverified-mod client profiles were excluded; a clean authenticated profile could be created. This is protocol verification, not gameplay. |
| Browser authorization | The user's real browser claim completed through the official API. The host secret was stored in installation-scoped Windows Credential Manager and never printed. The original interrupted claim was resumed. |
| Public tunnel | A real playit Minecraft Java tunnel mapped exactly one TCP origin to `127.0.0.1:25879`. The official embedded agent authenticated, and a status request through the public SRV hostname returned the matching unique Minecraft session marker. The repeat against the corrected production lifecycle passed too. **These requests originated on the host, not an independent network.** |
| Tunnel recovery | A controlled Reconnect canceled/replaced the managed agent and regained a matching public response while retaining the same Minecraft PID and session. The live check passed in about 87 seconds including installation/start/stop. This is agent-replacement recovery, not a real ISP-outage or remote-player continuity test. |
| Cleanup | Each completed live test sent Minecraft's graceful stop and canceled the managed agent. A process check after the earlier lifecycle test found no remaining test JVM. No test server was intentionally left publicly running. The user-created provider agent/tunnel remains in their account, inactive between tests. |
| Frontend and packaging | Production TypeScript/Vite build and optimized Windows EXE/NSIS packaging passed. The final EXE stayed running and initialized SQLite in a fresh isolated startup smoke test; no server was started by that check. The native UI interaction test was explicitly stopped by the user with Escape; full interactive hosting, resize, keyboard, installer and close-dialog verification was not completed. |

## Integration findings and limits

The first Fabric attempts exposed an upstream `+` Maven-version syntax and the need to use Fabric's dedicated server launcher to set the verified game-JAR path. Those issues were corrected; headless JVM mode prevents a loader error from leaving an invisible graphical dialog waiting forever. Subsequent real Fabric startup/repair/backup/restart checks passed.

The initial onboarding started the forwarding agent only after a tunnel existed. Playit's browser wizard instead waited for the agent. This circular dependency was corrected: setup connects the official agent with no forwarding origins, and hosting connects while waiting for the exact Minecraft mapping. Unrelated ports are never inserted into the forwarding lookup.

Free Minecraft Java support is documented by [playit](https://playit.gg/download); the official core is pinned to released v1.2.0 and its [BSD notice](third-party/PLAYIT-LICENSE.txt) is included in source and embedded in the executable's Settings license view. No payment or premium configuration was performed. Current operator quotas, availability and routing govern; no unlimited bandwidth, fixed capacity, region or latency promise is made. See [setup and official references](HOSTING.md).

**Not verified:** two authenticated players entering the world, a player on a genuinely different network, authenticated host-client/server gameplay together, actual allowlist admission/rejection of owned accounts, real ISP outage/CGNAT conditions, every loader/mod combination, or the complete interactive installer/uninstaller/UI matrix. Server-side authentication/allowlist configuration and raw status availability are not substitutes for these checks.

Integrated authenticated client launch still requires Orbit's own approved Microsoft/Minecraft registration. No borrowed client ID, entitlement bypass or offline-mode shortcut is used. Quick Play support and the guarded Join workflow are implemented, but actual authenticated entry remains blocked/unverified without that registration and player accounts. The existing launcher integration limitations remain documented in [integrations](INTEGRATIONS.md).

The playit authorization used an isolated test installation. Normal installations use their own credential namespace and need the one-time browser setup from Minecraft → Servers. A provider-side test resource is not silently copied into the user's existing Orbit identity or credentials.

## Reproduce

Only run live checks with an explicit isolated directory and accepted EULA. Public checks use the account you authorize and may require manually configuring the free Minecraft Java tunnel. Test resources are stopped afterward; provider-side account resources are not silently deleted.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1

$env:ORBIT_SERVER_EULA = 'accepted'
$env:ORBIT_SERVER_TEST_ROOT = "$PWD\.verification\hosting"
cargo test --manifest-path src-tauri/Cargo.toml live_server_install_lifecycle_backups_and_fabric -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml live_playit_browser_claim_and_public_roundtrip -- --ignored --nocapture

# Optional controlled agent replacement, using an already authorized host:
$env:ORBIT_TUNNEL_RECOVERY_TEST = '1'
cargo test --manifest-path src-tauri/Cargo.toml live_playit_browser_claim_and_public_roundtrip -- --ignored --nocapture
```
