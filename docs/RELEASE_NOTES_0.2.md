# Orbit 0.2.0 — social prototype (prerelease)

Download **Orbit_0.2.0_x64-setup.exe** for installation, or **orbit-launcher.exe** for standalone use (requires Edge WebView2). SHA-256 checksums accompany both. This Windows x64 build is unsigned.

## Added

- Local profiles, optional separate EVM wallets, peer-verified SIWE, protected keys and encrypted export/restore.
- Signed invitations, accepted friendships, encrypted NIP-17 conversations, durable outbox, queued/sent/delivered/failed states, reconnect, blocking and profile updates.
- Separately encrypted photos/GIF/video/files, multiple attachments, optional JPEG compression, inline viewers, original download, progress/cancel, integrity checks, range-resume and local cache clearing.
- Connections UI with publisher-owned OAuth configuration, honest manual-link/unavailable states, official Bluesky OAuth implementation and public-post reading.
- Separate game/music privacy controls, expiring explicit activity sharing, local process matching and Windows media-session fallback. No fabricated Join actions or financial transfers.
- Automatically loaded **Public Roblox Explore**, separately labeled from restricted personalized Home. Official Home can open in the system browser without extracting its session.

## Verification

- Final suite: **10 passed, 0 failed**, plus three opt-in live tests. Real Windows Credential Manager, SIWE rejection/replay, actual two-client Nostr cryptography/delivery/receipts/reconnect/blocking, friendship persistence and encrypted recovery.
- A separate **1 GiB synthetic attachment** passed encryption, upload, resumed retrieval and original SHA-256 verification against an isolated local service. This is **not** a free public-hosting guarantee.
- 6 MP original photo, animated GIF and H.264 MP4 transfer/integrity/decoding passed; cancellation, failed upload retry, corrupt-cache recovery and JPEG compression passed.
- Live Roblox public catalog/details/cache and Windows shortcut/program/persistence regressions passed. Native local profile creation, Connections layout and Windows media-session metadata/artwork were inspected in the installed build.
- Minecraft installation/repair/managed Java/renderer initialization succeeded in the current regression, but the game exited cleanly before the 35-second duration assertion, so that assertion failed. Earlier 0.1 Vanilla/Fabric 35-second launch checks remain documented separately.
- Optimized EXE/NSIS build and isolated silent installation passed. Full interactive media playback, drag-and-drop/resize and installer/uninstaller coverage remain incomplete.

## External limits

- Personalized Roblox Home/friends require an official website session; no documented supported OAuth Home authorization was established. Explore is not presented as personalized Home.
- Microsoft/Minecraft, Spotify, Discord and Roblox identity need Orbit publisher registrations/access. No registered IDs or secrets were borrowed. Ordinary users never need developer keys. X API access is billable and disabled; Instagram personal accounts use manual links.
- Bluesky OAuth is implemented using the official protocol through Atrium, with a labeled localhost development mode until production metadata is configured. Real-account OAuth/refresh flows remain unverified.
- Choose compatible existing relays and a Blossom provider explicitly. No public operator retention/SLA or free encrypted GB-file service has been established. nostr.build advertises 20 MiB media-only free uploads; encrypted binary compatibility is not assumed. No paid services or transactions were provisioned.
- This is a prototype without an independent security audit, multi-device synchronization or group/voice/financial functionality.

Details: [setup/recovery](https://github.com/spokobot737-lab/orbit-launcher/blob/main/docs/SOCIAL_SETUP.md), [verification](https://github.com/spokobot737-lab/orbit-launcher/blob/main/docs/SOCIAL_VERIFICATION.md), [publisher configuration](https://github.com/spokobot737-lab/orbit-launcher/blob/main/docs/PUBLISHER_REGISTRATION.md), [architecture comparison](https://github.com/spokobot737-lab/orbit-launcher/blob/main/docs/SOCIAL_ARCHITECTURE.md).
