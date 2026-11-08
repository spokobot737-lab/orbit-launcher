# Orbit 0.4.0 — local Minecraft hosting (prerelease)

Windows x64: **Orbit_0.4.0_x64-setup.exe**, or **orbit-launcher.exe** with WebView2 installed. SHA-256 checksums accompany the files. This is an unsigned integration preview.

## Added

- Minecraft → Servers: independent dedicated worlds, Vanilla **1.21.1** and bare **Fabric 0.16.14**, verified downloads/repair and shared managed Java 21.
- Server settings, explicit EULA acceptance, small-group defaults, memory guidance, player counts, process metrics and bounded logs. Host Play targets localhost; clients and servers remain separate processes.
- The official playit core, protected host credentials, one-time browser authorization, an empty-map setup agent and restricted Minecraft forwarding. Create the free Minecraft Java tunnel once in playit's dashboard; no developer key or router setup is required for this mode.
- Accepted-friend encrypted server cards, expiring status announcements, compatible client selection/creation/install and guarded Quick Play. Minecraft account authentication and explicit host allowlist approval remain enforced.
- Compatible Vanilla world-copy import, stopped-world backups, restore with safety copy, explicit deletion, graceful Stop/Restart and confirmed force recovery. Closing while hosting offers stop-and-exit, minimize or cancel; Exit-after-launch cannot abandon hosting controls.

## Verification and boundaries

**17 native tests passed.** Real Vanilla and Fabric installation, repair, local startup/status, graceful saving, restart and backup/restore passed. Real browser authorization and a matching Minecraft response through a public playit tunnel passed. Controlled agent replacement regained the public response without restarting Minecraft. Test servers and agents were stopped.

The public tunnel probe ran **from the host**. Two real authenticated players, entry from a separate network and authenticated client gameplay are not claimed as verified. Integrated Microsoft/Minecraft login still requires Orbit publisher registration/access. Offline identities are not used to bypass it. Interactive UI testing was stopped by the user; full hosting/close-dialog/installer usability coverage remains incomplete.

The host PC must stay powered on, awake and connected. Free-tier availability, quotas and routing are controlled by playit; no guaranteed latency, player capacity, unlimited bandwidth or paid hosting is included. Initial Fabric support has no additional mods; arbitrary modpacks, world version changes and modded-world import are blocked.

Existing Orbit identities, wallets, contacts, conversations, Minecraft installations and launcher settings are preserved. See [hosting instructions](HOSTING.md), [detailed verification](HOSTING_VERIFICATION.md), and [playit license](third-party/PLAYIT-LICENSE.txt).
