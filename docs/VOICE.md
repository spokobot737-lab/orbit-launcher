# Voice in Orbit 0.5.0

Open **Conversations** and select an accepted friend. Use the microphone button beside the composer to record up to two minutes, then stop and preview the clip. **Send voice message** starts the encrypted upload. Use **Retry voice message** after an upload error, or discard the clip. The recipient can download and play the attachment inline while its storage provider retains it.

Use **Call** to place a one-to-one call. The recipient can answer or decline. The call bar remains visible as you navigate Orbit. It includes the microphone selector, mute, volume, and hang up. When your after-launch setting is **Exit Orbit**, launching a game or program during a call minimizes Orbit. Closing the app during a call asks whether to end it.

## Connectivity and data

- Orbit uses audio-only WebRTC and Cloudflare’s public STUN service at `stun.cloudflare.com:3478`. Cloudflare documents this STUN endpoint as free and unlimited. [Cloudflare documentation](https://developers.cloudflare.com/realtime/turn/faq/)
- The relay carries short-lived, NIP-44 encrypted signaling. Nostr relays can see sender and recipient public keys and event timing. Peers exchange network candidates to establish the direct audio path. Call signaling is kept out of the local chat database and durable outbox.
- Some NAT or firewall combinations require TURN. This version uses STUN only. Send a voice message when a direct connection fails.
- Uploaded voice files use Orbit’s existing encrypted attachment providers and inherit their retention and access limits. The microphone is requested when you choose to record, call, answer, or change the active microphone.

## Existing installations

Use the same Windows installation and data directory to keep your profile and conversations. Historical server cards show **Server hosting is unavailable**. Worlds and backups remain under the existing Orbit data directory at `minecraft/servers`. The separate Minecraft client launcher remains available.

## Verification on Windows

- Built the x64 NSIS installer and ran its packaged Orbit executable with two isolated data directories and WebView2 157.0.4341.46. The installer itself was built. The desktop test launched the packaged EXE directly.
- Used a local Nostr relay and Blossom fixture. WebView2 captured synthetic audio in WebM/Opus, played the preview, and released capture on stop. The recipient downloaded, decrypted and played the clip after the sender process exited.
- Both WebRTC peers reported inbound and outbound audio RTP packets. Mute toggled the audio track. Navigation to Minecraft preserved the call. Launching a Windows program with **Exit Orbit** selected kept the process and call alive. An actual game launch and restoring the window from the taskbar were outside this test.
- Exercised Orbit going offline and reconnecting. Simulated microphone permission denial, a track ending, and direct connection failure. Verified cleanup and the voice-message fallback. The native test rejected expired, pre-reconnect and duplicate signals and verified that they created no chat or outbox rows.
- Opened a preexisting encrypted server card, confirmed the **Server hosting is unavailable** state, and verified that the old server start command returned an error. Synthetic world and backup files retained their original contents.
- Both desktop instances ran on one Windows PC. Physical microphone capture, hardware unplugging, actual Windows permission prompts, public relay support for ephemeral call events, and calls across independent networks still need field verification. The direct-connection test establishes local behavior only.
