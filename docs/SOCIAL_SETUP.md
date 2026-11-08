# Social prototype: setup, privacy and recovery

Orbit 0.3 is a communication prototype, not a production messenger or a promise of free hosting. The launcher remains usable without an Orbit profile or wallet.

## Use two installations

1. Create an Orbit profile under Profile. Its stable ID is a public communication key. An independent private key and local-data encryption key are protected by Windows Credential Manager.
2. Networking starts automatically when opening social features. No endpoint or API key is required. Defaults are **auth.nostr1.com** and **relay.primal.net** for encrypted messages, **nostr.download** and **files.sovbit.host** for encrypted files. NIP-42 relay authentication uses the communication identity automatically. Relay operators can reject writes, require payment, remove messages or disappear; no paid access is implemented or purchased.
3. Create an invitation under Conversations and send its code privately. QR is available when the code fits. Invitations expire in seven days and admit one identity. The recipient imports the code; the inviter accepts the incoming request. Pending requests remain separate. Accepted friends appear immediately, even without messages.
4. Open Conversations and send a message. **Queued** means local durable outbox; **sent** means a recipient relay acknowledged the event; **delivered** means the recipient Orbit installation sent an encrypted receipt. It is not a read receipt. Offline delivery requires the relay to retain the event until reconnection. Failed sends stop after five attempts and offer explicit retry.
5. Select files in Conversations and Send. Automatic storage tries the disclosed compatible services, with a tested local limit of **25 MiB per original file**. Failed uploads preserve the message draft. Advanced custom configuration permits a local bound up to 2 GiB, but that does not prove provider capacity or quota.

No registration, financial wallet or Orbit backend is needed for social protocol keys. Settings → Network provides Offline, Automatic and Custom modes. Endpoint forms, restore defaults, diagnostics, reconnect and cache controls are under Advanced. Switching to Automatic or restoring defaults preserves intentional Offline. Existing custom configurations and disabled networking are not silently replaced. An absent legacy configuration receives defaults; a persisted disabled legacy row is conservatively treated as intentional offline because the old schema cannot establish user intent.

Edit the profile card in place, choose avatar/banner and Save. Save writes locally and automatically queues a complete signed update for accepted friends; Cancel discards the draft. Feedback distinguishes local waiting, relay-acknowledged sync and failure with Retry. Friends apply only newer signed revisions and retrieve retained updates after reconnecting. Avatar/banner bytes are encrypted separately; protected retrieval metadata travels inside encrypted messages. Previously received profiles remain cached while the owner is closed. Availability depends on retained copies and never implies online presence. Receiving a link never independently proves ownership of the external account.

## Attachments

Files are streamed through age X25519 encryption before upload. Each attachment has a separate key. Retrieval metadata and keys travel inside NIP-17 encrypted messages; local message/media records are encrypted with an OS-protected data key. Original local file paths are not sent to friends.

- Select or drop up to ten files in Conversations. Selecting a file only stages a local encrypted copy. **Send** uploads it using the configured storage mode.
- Original quality is the default. Optional photo compression creates a separate JPEG (maximum 2560 px, quality 88); GIFs are preserved. Compression inputs are bounded to 32 MiB and decoder allocations to 128 MiB. The original file remains unchanged.
- BUD-06 preflight can reject unsupported media, quota or size. BUD-02 uploads are complete PUTs; retry starts over. No resumable upload claim is made.
- Downloads retain `.part` files and use verified byte ranges where supported. Both ciphertext and plaintext size/SHA-256 are checked. Corrupt partial data is removed so retry can recover.
- Images and animated GIFs display inline after verified retrieval. MP4/WebM use WebView2's available HTML5 codecs; unsupported codecs need an external player. Playback begins after the full verified download, not streaming decryption/seeking over a remote encrypted blob.
- Download originals to a new filename. Clear local media cache under Settings → Network → Advanced. Clearing cache also removes staged copies; select unsent originals again. It does not delete provider copies or message history.
- At most two transfer tasks run concurrently, with 256 KiB streaming buffers. Photo decoding has a separate bounded allocation. Saved profile images and user-requested transfers may upload in the background; there is no continuous disk scanning. Offline cancels active transfers and pauses pending social work.

Images may contain EXIF/location data when sent at original quality. Encryption hides contents from storage, but not ciphertext sizes, request IP/timing, recipient keys at relays, or the existence of blobs. Downloaded plaintext media is cached on this Windows account's disk for playback. Use Windows disk/account protection for that cache. Deleting a local copy does not revoke a recipient's existing copy.

### Actual provider limits

Revalidated 2026-11-08: both automatic storage operators accepted a synthetic encrypted **25 MiB** original as `application/octet-stream`; complete downloads matched ciphertext SHA-256 and signed deletion succeeded. This establishes point-in-time compatibility, not a permanent free quota, SLA, retention guarantee or working GB uploads. No paid plan was purchased. No unrestricted encrypted GB-file public service is claimed. Primal storage and nostr.build are not default encrypted-file providers; the Primal **relay** is a separate messaging service.

A configurable compatible provider can permit larger blobs, but hosting, retention and bandwidth remain its responsibility. Permanent size/type rejection is remembered to avoid repeating doomed attempts; transient errors use cooldown/backoff. Returned retrieval URLs must remain on the provider origin, redirects are disabled, and hashes/sizes are verified. Clock errors are reported without changing Windows time or backdating signatures. See [operator review and verification](SOCIAL_VERIFICATION_0.3.md), including unavailable terms/retention information.

## Wallets and SIWE

A wallet is optional. Create a local EVM key with zero balance or connect an existing EOA through a browser extension. There are no balance queries, transfers, approvals, swaps, contracts or transaction requests.

Local wallet keys and Nostr communication keys are separate. The browser receives only a fixed SIWE flow, never private keys. The actual verifying Orbit service or peer issues an unpredictable three-minute nonce bound to its domain/URI and both communication identities. Signature, audience, chain, exact message, expiration and one-time consumption are checked. Connecting an address is not global proof; friend proofs are labeled as verification by this device and expire. EIP-1271 contract-wallet signatures and external-extension signing of peer challenges are not implemented; peer approval currently uses the separately stored local wallet.

## Backups and migration

Export under Settings → Backup and recovery to a new `.orbit` file using a passphrase of at least 16 characters. The age-encrypted backup includes profile, communication/data keys, optional local wallet, contacts, messages, queued outbox, signed snapshots and attachment references. It excludes media bytes, Microsoft credentials and external OAuth sessions. Keep the backup and passphrase separately and safely; the backup contains valuable keys. Older 0.2 backups remain accepted.

Restore only into a fresh Orbit profile. Network connections and media upload consent remain off until reviewed. Reconnect external accounts. Do not run the old and restored installation simultaneously with the same keys; this preview has no independent-device key delegation or automatic multi-device history synchronization. Sent self-copies are not used to reconstruct history automatically. Media recovery depends on a provider copy or separately preserved originals/cache. **Recovering an Ethereum wallet alone does not recover Orbit contacts, messages or communication identity.**

Disconnect removes credentials from this device; revoke external grants in the provider's account settings as well. An external wallet's key stays in its extension. Disconnecting a local wallet destroys this device's saved key; export a backup first.

## Activity

Game and music visibility have separate controls under Settings → Privacy. Both compact cards live on Profile and friend details. Activity sharing is an explicit action and expires after 90 seconds, independently of persistent profile information. It does not imply online presence or joinability.

- Minecraft and opted-in Programs launched through Orbit track their actual parent process. External detection is user-triggered and matches opted-in program executable paths exactly. Hand-off processes and Roblox's game/session state cannot be reliably inferred from protocol dispatch.
- Friend game cards can launch a uniquely matching local Minecraft profile/version/loader or an opted-in program of the same name. Otherwise choose a local entry yourself. No Join action is offered without real session-join data.
- Windows media sessions read only this device's current title, artist, artwork and playing/paused state. Spotify account playback requires the publisher registration and API access. A Windows session alone cannot see another device's music. No link is invented when metadata does not supply one.
- This preview has no persistent automatic cross-device presence/heartbeat or automatic Spotify polling. Refresh/share explicitly; stale cards expire.

## Protocol and implementation boundaries

NIP-17/NIP-44/NIP-59 use the Rust Nostr SDK; SIWE uses the `siwe` library; encryption/backups use age. Do not change cryptographic formats casually. See [architecture comparison](SOCIAL_ARCHITECTURE.md). Future groups, reactions, replies, read receipts, voice and financial features are not implemented. This prototype has not had an independent security audit.
