# Orbit 0.3 verification and operator review

2026-11-08, Windows x64, Rust MSVC, Node 24. Only synthetic identities/files were used; no user wallet, private files, payments or server provisioning. The [0.2 report](SOCIAL_VERIFICATION.md) remains the historical baseline.

## Checks

| Check | Result and boundary |
|---|---|
| Native regression suite | 14 passed, 6 opt-in checks ignored. Covers real Windows Credential Manager, SQLite reopen, encryption, backup/restore, mutual friendship, delivery receipts, SIWE, activity expiry and program imports/launch/error paths. |
| Upgrade migration | Absent legacy configuration gets automatic defaults. Explicit custom endpoints, disabled configuration (including ambiguous persisted empty/disabled legacy rows), existing identity keys, wallet and stored data remain intact. No identity regeneration or reset. |
| Public messaging | Real signed NIP-17 publication, recipient subscriptions/decryption and retained offline retrieval passed separately on auth.nostr1.com and relay.primal.net, with SDK NIP-42 automatic authentication enabled. nos.lol publication timed out; relay.damus.io delivery timed out, so neither was selected. A socket alone was not counted as success. |
| Full automatic flow | `live_automatic_orbit_social`: two independent default-config profiles, signed invitation, mutual acceptance, accepted friend before any messages, bidirectional messages/receipts, offline recipient, sender closed, updated signed name/bio/avatar/banner retrieved, encrypted attachment retrieved and plaintext SHA-256 matched. No URLs entered. Test blobs deleted with signed requests. |
| Public storage | Both nostr.download and files.sovbit.host accepted an actual encrypted 25 MiB original (`application/octet-stream`, ciphertext 26,221,098 bytes). Complete GET/hash/size checks and signed DELETE passed. nostr.download returned HTTP 201 / DELETE 204; files.sovbit.host HTTP 200 / DELETE 200. Returned URLs remained on the issuing origin. |
| Storage fallback | Controlled HTTP 413 provider then compatible storage: fallback upload, download and hash verification passed twice; permanent rejection endpoint was contacted only once. Size cooldown uses original size, avoiding variable encryption-overhead retries. Type rejection and transient cooldown are bounded; Retry-After accepts seconds or HTTP date. |
| Complete profiles | Signed schema/revision proof, changed avatar and banner, visible links, newer update after owner shutdown, locally cached reopen, duplicate/out-of-order rejection and forged-author rejection passed. Encryption keys are excluded from ordinary contact fields and frontend-visible message rows. |
| Images and drafts | Native Windows picker for avatar and banner, immediate crop/preview, Cancel and Save were checked in the actual application. Both images and bio survived navigation and restarting the installed build with the same synthetic identity. Draft text/images survived background refresh. Native profile tests also check updated images at the peer. |
| Network failure | Custom/offline preservation, bounded retry, cancellation, same-origin URL validation, clock-specific error and no clock adjustment tested. Offline pauses connections and cancels active transfers. |
| Interface | Correct sidebar order, combined Conversations, invitation/QR without endpoint entry, profile and conversation layouts around 870 px width, visible keyboard Tab focus and Enter activation of Cancel checked in the native app. Exhaustive accessibility and every populated small-window chat state remain unverified. |
| Build | TypeScript/Vite production build and optimized Rust/NSIS packaging passed. Silent isolated installer exited 0; installed EXE reopened the existing synthetic profile and images. Full installer/uninstaller matrix remains unverified. |

Existing launch functionality was preserved without modifying Minecraft/Roblox/program service implementations. See the baseline [launcher verification](VERIFICATION.md) for previously tested direct launches and external restrictions. The 0.2 Minecraft repeat reached renderer initialization but exited before the 35-second duration assertion; that repeat was not a passing game-duration test. No authenticated Microsoft launch is claimed.

## Selected operators and terms review

- Messaging: [auth.nostr1.com](https://auth.nostr1.com/) and [Primal](https://primal.net/). auth.nostr1 describes a free DM inbox relay running alpha. Its terms section and Primal's [terms page](https://primal.net/terms) did not expose complete readable terms in this environment. Neither point-in-time tests nor that description establish unlimited use or retention.
- Files: [nostr.download](https://nostr.download/) and [files.sovbit.host](https://files.sovbit.host/). nostr.download exposes [Route96 API documentation](https://nostr.download/docs.md). Candidate `/terms` and `/terms-of-service` routes there were unavailable; files.sovbit.host's root/terms routes did not expose usable terms. A full affirmative redistribution/scale-use policy or retention promise could not be verified. These are disclosed experimental defaults in an integration preview, not a production hosting commitment. Re-review policies/capacity with operators before broad distribution.
- [FRAME transport](https://github.com/spokobot737-lab/stylized-crypto-wallet/blob/main/electron/chat/transport.ts) was reviewed for workflow/protocol reference. Candidate endpoints were tested independently; no private branding/assets were copied.

The automatic 25 MiB cap is the tested local ceiling, not an advertised perpetual free quota. Public GB uploads, indefinite message/blob retention, operator uptime, future pricing and production-scale usage remain unverified. No paid plans or hosting were provisioned. Primal's relay is used only for messages; its incompatible encrypted-file storage is not selected. Files are not disguised or split to bypass limits.

## Availability and security limits

Profiles are friends-only signed snapshots delivered through NIP-17, not public NIP-0 profiles or public contact lists. Media bytes are age-encrypted; retrieval keys stay in encrypted local snapshots/messages. A cached profile is persistent information, not presence. Activity expires independently after 90 seconds. Relays still see recipient keys, IP and timing; storage sees encrypted blob sizes and request metadata. Playback caches contain plaintext on the current Windows account's disk.

History recovery runs at startup and on observed reconnection, with bounded per-relay pagination (32 pages, 256 events per page, expanding to 2048 for equal-second boundaries). It is not an unlimited archive-recovery guarantee. Delivery depends on retained remote events. Simultaneously running restored installations with the same keys is not a supported multi-device synchronization scheme.

The local clock was approximately 90 seconds ahead of HTTP Date. Both selected storage operators accepted the real signed uploads; Windows time and signature timestamps were not adjusted. Rejections indicating invalid time produce a readable clock diagnostic. A third-party outage or changed policy can still stop messaging or attachments without disabling the launcher.

Microsoft/Minecraft and optional external account OAuth still require the publisher's own registrations/access. No borrowed IDs, application secrets, session-cookie entry or invented authenticated identities are introduced. The prototype has not received an independent security audit.

## Reproduce opt-in public checks

These contact real operators with temporary synthetic identities/files. Credential Manager access and outbound networking must be available. Tests delete their own uploaded blobs where supported.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml live_default_relay_candidates -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml live_default_storage_candidates -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml live_automatic_orbit_social -- --ignored --nocapture
```

Candidate probes print each individual PASS/FAIL; an exit code alone is not proof that every candidate worked. Do not repeatedly run the 25 MiB probe against public services without a reason.
