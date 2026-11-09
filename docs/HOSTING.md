# Minecraft server hosting

Orbit hosts the dedicated Minecraft Java process and world on the host's Windows PC. Gameplay travels through the selected TCP tunnel or manually configured public connection, never through Nostr. No paid server or Orbit backend is provisioned.

## Supported scope

- Minecraft Java **1.21.1**, protocol 767, Java 21, Windows x64.
- Vanilla and **Fabric Loader 0.16.14 without additional mods**. Bare Fabric accepts an unmodified Vanilla client; the metadata contains no client-required, server-only or optional mods. No modpack installer or arbitrary executable/mod download is provided. Orbit refuses extra server mods and offers clean client profiles matching the selected premium/nonpremium mode. The existing performance client preset is preserved but is not silently treated as verified server compatibility.
- Multiple stored server configurations; **one running managed server at a time**.
- Import copies only a closed unmodified Vanilla Java world saved with DataVersion 3955 (1.21.1). Worlds marked as modded are rejected because a version number cannot establish mod compatibility. Other versions, silent upgrades/downgrades and changing a server's loader/version are blocked. Create a separate server for other experiments. Upgrading a world elsewhere can make it incompatible with older Minecraft versions.

## Host a world

1. Open Minecraft → Servers → Create server. Choose a name, loader, player authentication (new servers default to Nonpremium), new world or existing compatible world copy, heap memory, player slots, difficulty, game mode and optional seed. View and simulation distances are bounded.
2. Review [Minecraft EULA](https://www.minecraft.net/eula) and explicitly accept it for that server. Orbit records the acceptance and writes `eula=true` only after approval.
3. Complete the one-time online-access setup below. Under Allowlist, approve the exact nickname for Nonpremium, or resolve and approve the Minecraft account for Premium. No operator permissions are granted.
4. Click Host server. Orbit verifies Mojang version/server SHA-1, installs or verifies the managed Java runtime, verifies every Fabric Maven library where applicable, writes constrained server properties and starts a separate Java process. Partial downloads use the existing verified downloader.
5. Local Minecraft readiness and external access are separate. Online requires the server-ready signal, a local Minecraft response, the authorized tunnel agent and a matching Minecraft status response through the public address. The public round-trip is performed **from the host**, not from an independent network; the interface states that distinction.
6. Play connects the host client through `127.0.0.1:<port>` using Minecraft 1.21.1 Quick Play. For Nonpremium it uses the chosen offline nickname without Microsoft sign-in; for Premium it requires an authenticated owned account. It checks memory, approves the local host profile, installs/verifies client files and then launches. Local and internet addresses have separate Copy buttons. Existing launchers can use Multiplayer / Direct Connection with those addresses. Friend clients use the external address. Leaving the client does not stop the server.

The PC must remain powered on, connected and awake. Orbit does not change power settings. Player slots are a configuration cap, not a promised capacity. CPU, available memory, upload bandwidth, world generation, view distance and mods affect performance. Recommendations reserve memory for Windows and a separate client; Orbit does not allocate all RAM.

## One-time playit setup

Official references reviewed 2026-11-08:

- [Playit download and supported free games](https://playit.gg/download): Minecraft Java is listed in the free tier; only the host needs the agent.
- [Official Java setup guide](https://playit.gg/support/how-to-setup-a-mc-server/).
- [Agent source at released v1.2.0](https://github.com/playit-cloud/playit-agent/tree/v1.2.0), pinned to `7acf13568a99707dda73028234fdb376e45b15da` in Cargo.lock.
- [Terms](https://playit.gg/terms/) and [prohibited uses](https://playit.gg/support/prohibited-uses/).
- [Latency guidance](https://playit.gg/support/how-to-lower-ping/) and [premium feature distinction](https://playit.gg/support/playit-premium/). No premium feature or payment is requested by Orbit.

Click Set up online access. Orbit uses the official claim/setup and claim/exchange calls to prepare a one-time browser claim. **You** sign in/register and approve the Orbit host in your system browser. Passwords and developer API keys are never entered in Orbit. Pending claim codes can be resumed after restarting setup. The returned host-agent credential is stored with the existing installation-scoped Windows Credential Manager facilities, and supplied directly to the embedded official agent core in Rust memory. It is not written to argv, frontend storage, SQLite or a plaintext agent file.

After authorization, Orbit connects a temporary setup agent with an **empty forwarding map**, so the website can detect the host before a tunnel exists. No local ports are exposed by this setup agent. Hosting replaces it with the restricted Minecraft agent; that agent also stays connected while waiting for a matching tunnel, avoiding a circular “waiting for agent” setup. Setup attempts have bounded timeouts and can be retried.

If the agent was revoked or the wrong account was chosen, stop hosting and use Advanced online access → Disconnect this host, then authorize again. Confirmation removes only this installation's playit credentials. Revoking/deleting the remote agent remains a separate dashboard action; Orbit does not silently delete provider resources.

**Orbit 0.4.1 automatic setup:** Host server opens authorization when this installation has no host credential. Use your own verified playit account. After approval, an already-ready server reconnects automatically. The self-managed agent requests or reuses one free global Minecraft Java TCP tunnel targeting exactly **127.0.0.1:<configured port>**, with no Proxy Protocol. No premium allocation, account-wide API key or developer credential is used. The older official SDK `/tunnels/create` endpoint passed real creation/origin/cleanup verification; the newer `/v1/tunnels/create` endpoint rejected its SDK schema with a body-validation error and is not used.

Allocation can take time. Pending allocations are reused, requests are serialized, and an uncertain response prevents another request for five minutes. Orbit does not buy capacity or silently change another tunnel. If playit refuses an allocation (unverified email, account quota, permissions or service failure), the app displays the reason and offers its dashboard. Manual fallback: create a **free Minecraft Java** tunnel for this installation's Orbit agent to **127.0.0.1:<port>**, Proxy Protocol disabled, then Reconnect online access. Joining friends need neither an agent nor a playit account. Each host PC needs its own one-time approval; downloading Orbit cannot bypass provider authorization or quotas.

The official agent core is embedded under its BSD-2-Clause license; see [license notice](third-party/PLAYIT-LICENSE.txt). Orbit does not install a Windows service, change router/firewall settings, require administrator rights, or control an unrelated installed playit agent. Only the selected one-port loopback Minecraft origin is inserted into the core's forwarding map; other account tunnels are not exposed. Stopping that server cancels Orbit's agent independently. Updates to the pinned core require a new verified Orbit build; there is no silent downloaded executable updater.

Playit uses an outbound connection, avoiding the need for an inbound public IP/port forward and therefore supporting the intended CGNAT workflow when outbound traffic is allowed. Blocked UDP, restrictive firewalls, provider availability, account/tunnel quotas or routing may still prevent access. Free-tier availability is not an uptime, bandwidth, retention or low-latency guarantee. Free global routing may add distance; regional premium options are not configured. The dashboard's current applicable limits govern. Joining friends need neither playit nor router configuration.

## Advanced manual mode

Set a public Minecraft address and local TCP port under Advanced connection settings. This mode binds that port to network interfaces. It requires a reachable public address and appropriate router/firewall rules, and may not work behind CGNAT. Orbit neither performs UPnP/automatic port forwarding nor opens unrelated ports. A status check from the host may fail when a router lacks NAT loopback even if a remote path works; Orbit will not label that unverified path Online. Diagnose the connection explicitly instead.

## Invitations and access

Send a server card to an accepted friend under Invite friends. Signed NIP-17 envelopes carry host/server/session identity, version, bare-loader compatibility, public address, ordered revision and a 150-second expiration. They carry no game traffic. Online announcements refresh about once a minute while available. Graceful stop queues Offline; missed updates expire. Removing/blocking a friend stops future announcements and prevents using their stored Join target.

For Nonpremium both peers need Orbit 0.4.1 or later; cards without an authentication field remain Premium. Peers need Orbit 0.4 or later for server cards. Conversations displays the structured card, and Minecraft → Servers lists cached friend servers. Join rechecks accepted friendship, expiration, public DNS/address, protocol and the unique session marker. It offers a clean matching client, explicit install/repair, or creation of a mode-compatible profile with an editable offline nickname. No incompatible modded client is silently launched. Quick Play is scoped to supported 1.21.1 arguments, based on [Mojang's documented mechanism](https://feedback.minecraft.net/hc/en-us/articles/16499677456781-Minecraft-Java-Edition-1-20-Trails-Tales); actual authenticated client entry remains unverified without publisher registration.

**A card is not access permission.** The host approves players separately. Both modes keep `white-list=true` and `enforce-whitelist=true`; RCON, query and JMX stay disabled, and no operator permissions are granted automatically.

- **Nonpremium:** `online-mode=false`, `enforce-secure-profile=false`. Local `OfflinePlayer:<exact name>` UUIDs require no Mojang lookup. Orbit writes the explicit UUID allowlist and reloads it on running servers. No Microsoft sign-in. Nickname case matters. This is not identity verification: someone claiming an approved nickname can impersonate that player. The allowlist alone does not prevent impersonation; use trusted players. This mode does not grant ownership or access to authenticated servers/Realms.
- **Premium:** `online-mode=true`, `enforce-secure-profile=true`. Minecraft authenticates connecting accounts. Integrated client sign-in needs Orbit's approved publisher registration; an already-authenticated official launcher can connect without logging in through Orbit. No Microsoft account is needed merely to install/run the dedicated server.

Existing servers stay Premium until changed while stopped. Mode changes require explicit confirmation because player UUIDs differ: inventories, pets and permissions may appear different. Separate mode-specific allowlists are retained; player data is not converted. Back up the stopped world first. Removing a player also kicks them; removing an Orbit friendship alone does not silently edit Minecraft access.

## Worlds, backups and shutdown

Directories: `%LOCALAPPDATA%\app.orbit.launcher\minecraft\servers\<uuid>\`. Each server has independent world/configuration, verified libraries and a `backups/<uuid>` directory. Runtime files reuse the launcher's managed Java tree. Import copies the original; it does not run on the original directory. Symlinks/reparse points are rejected, active-world locks are respected, and Orbit holds a server management file lock to prevent another instance modifying the same server.

Backups require a stopped server. Orbit never calls a raw active-world copy a consistent backup. The list shows location, creation time, size and restore/delete actions. Retention is manual: delete selected backups after explicit confirmation; nothing is silently pruned. Restore makes a safety backup, restores the world and retains the previous world directory, while keeping current authentication and allowlist settings. Old recovery directories also occupy disk; preserve them until you have verified the restored world. Social identity exports do not include Minecraft world bytes or playit credentials; use these separate world backups and reconnect playit after moving computers.

Stop sends Minecraft's `stop` command, allowing world save and player disconnect. Restart waits for graceful completion before starting a new process. Force stop is an explicitly confirmed recovery action for a stuck managed process, with possible unsaved progress loss. Orbit never kills all Java processes. Logs retain 400 bounded lines in the UI, with Minecraft's own logs in the server directory. Resident memory, configured heap, uptime and connected player counts are shown when available.

While hosting, the launcher's Exit-after-launch preference minimizes instead of exiting. Closing Orbit offers Save and stop then exit, Keep hosting/minimize, or Cancel. Minimized Orbit retains control; hosting does not survive exiting Orbit or a system crash as a managed background service. Abnormal process/OS termination can lose recent progress, which is why stopped-world backups matter.

See [verification](HOSTING_VERIFICATION.md) for tested flows and the boundaries around real authenticated players and independent-network connectivity.
