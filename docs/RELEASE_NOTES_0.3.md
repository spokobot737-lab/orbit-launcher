# Orbit 0.3.0 — automatic social networking (prerelease)

Windows x64: install **Orbit_0.3.0_x64-setup.exe**, or run **orbit-launcher.exe** with Edge WebView2 installed. SHA-256 checksums accompany both. This build is unsigned.

## Improvements

- Create a profile, invite a friend and exchange encrypted messages without entering server URLs. Automatic defaults, bounded retries/fallback, simple connection status and explicit Offline; advanced custom configuration remains available.
- Complete friends-only signed profiles: name, bio, avatar, banner and visible links, automatically queued on Save. Ordered revisions reject stale/forged updates. Retained remote copies and local caches allow viewing while the owner is closed.
- Edit the actual profile card in place. Native avatar/banner pickers, immediate previews, Save/Cancel and stable drafts during background refresh. Compact game/music activity remains separate from persistent profile information and expires normally.
- Conversations combines accepted friends, search, invitations, incoming/outgoing requests and chats. Accepted friends appear before messages. Sidebar order is Home, Minecraft, Roblox, Programs, Profile, Conversations, Collections, Downloads, Settings.
- Wallet, backup/recovery, privacy, network and cache controls moved into Settings. Existing keys, wallets, contacts, messages, media references, launcher data, custom endpoints and intentional offline mode are preserved.

## Verified

- Native suite: **14 passed, 0 failed**, six opt-in tests excluded from the ordinary run. Actual Windows Credential Manager, migration/persistence, real two-client protocol cryptography, SIWE, receipts, blocking, backup/restore, media integrity/resume/failure paths and activity expiry.
- Public default-operator end-to-end test passed: two synthetic profiles, invitation/mutual acceptance, friend before messages, two-way delivery/receipts, offline retrieval of a complete updated profile and encrypted attachment, original SHA-256 comparison, and test-upload deletion.
- Both selected storage providers passed actual encrypted **25 MiB** octet-stream upload, complete download/hash and signed deletion. Controlled permanent rejection/fallback passed without repeated doomed requests.
- Production frontend and optimized Windows EXE/NSIS build passed. Isolated installer exited 0; installed app retained the same identity, both images and bio after restart. Native picker/preview/cancel/save, background draft stability, narrow layouts and keyboard Cancel checked.

## Operators and limits

Messages use **auth.nostr1.com** and **relay.primal.net**; encrypted files use **nostr.download** and **files.sovbit.host**. NIP-42 authentication uses the communication identity. No account/API key, paid plan or Orbit backend is required for these tested flows. Requests go directly to the disclosed independent operators.

The automatic cap is **25 MiB per original file**, a tested local ceiling, not a permanent free quota, uptime/retention SLA or GB-upload promise. Full operator scale-use/retention terms were not available from the reviewed pages; defaults remain experimental. Revalidate operator policies before broad production distribution. No payments, file disguising or limit-evasion splitting were introduced. See [detailed checks and operator review](SOCIAL_VERIFICATION_0.3.md).

The existing computer clock was accepted by both storage services; Orbit did not change Windows time or backdate signatures. Clock-related rejections receive a readable diagnostic.

Microsoft/Minecraft and optional provider OAuth still need Orbit publisher registrations/access; authenticated flows remain unverified without them. Restricted personalized Roblox Home is still distinguished from working Public Explore. This release does not claim those external restrictions are resolved. Existing launcher behavior is preserved; no new full-duration Minecraft rendering test was claimed. No independent security audit or supported simultaneous multi-device key sharing is provided.
