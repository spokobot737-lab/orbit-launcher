# External integrations and registration

Investigated 2026-11-08. The project has no hosted server and collects no passwords, Roblox cookies, or application secrets.

## Roblox: Home is not Explore

A live unauthenticated POST to `https://apis.roblox.com/discovery-api/omni-recommendation`, with `pageType: Home` and a random session UUID, returned:

```json
{"errors":[{"code":9002,"subcode":0,"message":"Authentication token is missing"}]}
```

HTTP status: **401**. A session UUID is an analytics identifier, not authentication. The launcher attempts this Home request automatically when opening Roblox, displays its restriction, and never silently replaces it with popular or hardcoded experiences.

The separate `GET https://apis.roblox.com/explore-api/v1/get-sorts` returned HTTP 200 with actual public sections including Top Trending, Up-and-Coming, Top Playing Now, Fun with Friends and Top Revisited. Their presence and ordering are service-controlled. This is **public Explore**, not the user's personalized Roblox Home page. The user can explicitly opt into it; the preference can then automatically load Explore on later visits. No experience IDs are hardcoded in the shipped catalog.

The public web discovery/search endpoints are not a stable Open Cloud contract and can change or be restricted. They use the same native HTTP service as other integrations, cache results, and surface failures rather than inventing a replacement catalog.

Other live endpoints:

- `https://apis.roblox.com/search-api/omni-search` with query and page token.
- `https://games.roblox.com/v1/games?universeIds=...` for batched details/creators/statistics.
- `https://thumbnails.roblox.com/v1/games/icons?...` for icons; images downloaded only from HTTPS Roblox CDN hosts.

Roblox documents Home recommendations as personalized. Its documented Open Cloud OAuth identity scopes do not grant access to the website Home recommendation service. OAuth/PKCE is available for documented resources, but signing a user into Open Cloud does not establish a website `.ROBLOSECURITY` session or justify treating that credential as a Home API token. No supported third-party personalized-Home authorization path was established in this investigation. Orbit therefore does **not** offer a misleading Roblox login or ask for cookies. This requested feature remains blocked pending a supported Roblox integration/permission.

Sources:

- [Roblox discovery documentation](https://create.roblox.com/docs/discovery)
- [Roblox OAuth implementation, including PKCE](https://create.roblox.com/docs/cloud/auth/oauth2-develop)
- [Roblox OAuth reference](https://create.roblox.com/docs/cloud/auth/oauth2-reference)
- [Open Cloud scopes](https://create.roblox.com/docs/cloud/reference/scopes)

### Launching a selected experience

Rust re-fetches the selected universe's root place, validates a positive numeric ID, checks the Windows `roblox` protocol registration, then dispatches `roblox://placeId=<rootPlaceId>`. It never merely opens the generic Roblox home screen. The runtime's existing account and restrictions govern entry. Orbit confirms dispatch only; it cannot observe successful server entry through the protocol.

[Roblox's deep-link documentation](https://create.roblox.com/docs/production/promotion/deeplinks) documents this direct-to-app format but now marks the old deep-link creation process deprecated in favor of share links. Programmatically creating share links for arbitrary third-party experiences is not a documented capability established here. The existing protocol is retained with that compatibility limitation documented. If Roblox removes support, the integration needs revision, not a fabricated success state.

## Microsoft/Minecraft

**An application registration is necessary. It does not require embedding a secret or running a hosted backend.** The publisher creates one application for Orbit; end users normally use that publisher client ID rather than each registering their own app. This preview has no publisher registration because none was provided or created.

1. In [Microsoft Entra app registrations](https://entra.microsoft.com/), create an application supporting personal Microsoft accounts (or a supported account type including personal accounts).
2. Configure **Mobile and desktop applications**, with the system-browser loopback redirect `http://localhost`. Orbit binds an ephemeral port on `127.0.0.1`; Microsoft's native-app localhost port exception supports this pattern.
3. Use an authorization-code public-client flow with S256 PKCE. Do not create or distribute an application secret. Orbit requests `XboxLive.signin offline_access` on the `consumers` authority.
4. Copy the **Application (client) ID** into Orbit Settings and save it.
5. Request Minecraft API approval for that exact client ID through [Minecraft's AppID review link](https://aka.ms/mce-reviewappid). At investigation time that link redirected to a Microsoft Forms review form. The browsing tool could not read the form, so its current submission fields, eligibility criteria, cost and approval timing are **not verified**. The old Minecraft help article currently returned a generic help page, not usable registration instructions. Contact Minecraft support if the form is unavailable. Do not assume an Entra scope setting alone grants Minecraft Services access, or that ID@Xbox can approve a Minecraft launcher.
6. Once permitted, sign in using the system browser with an account owning Java Edition, and test entitlement/profile/refresh behavior. A `403` from Minecraft Services can indicate missing application permission; Xbox authorization can separately fail for account/family/region reasons.

The implemented chain is Microsoft OAuth → Xbox Live → XSTS → Minecraft `login_with_xbox` → entitlements → Java profile. The refresh credential and account metadata are saved using **Windows Credential Manager**. Minecraft access tokens remain in Rust memory and are replaced with redacted text in launcher-managed output. Refresh happens before every authenticated launch; failures require sign-in again. Signing out deletes Orbit's saved credential, not the browser's Microsoft session.

The app registration and account permission are external prerequisites, so this flow is **implemented but not end-to-end verified**. No other launcher's client ID is used.

Sources:

- [Microsoft authorization-code/PKCE public-client protocol](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow)
- [Microsoft redirect URI restrictions](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- [Minecraft AppID review redirect](https://aka.ms/mce-reviewappid)

### Installation sources

Minecraft version metadata starts at [Mojang's version manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json). Each version provides trusted hashes/URLs for the client, libraries, asset index, logging and Java component. Java comes from [Mojang's runtime manifest](https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json). Native Windows x64 libraries are selected by platform rules and safely extracted where needed. Java and Minecraft are invoked directly; no external launcher is required.

Fabric profile/library metadata comes from [Fabric Meta](https://github.com/FabricMC/fabric-meta/blob/master/README.md). Its Maven artifacts are checked against upstream SHA-1 sidecars. The performance preset targets 1.21.1 and pins tested Fabric/mod versions in `performance-profile.json`, resolving their files/dependencies using [Modrinth's version API](https://docs.modrinth.com/api/operations/getprojectversions/). Source projects are [Sodium](https://modrinth.com/mod/sodium), [Lithium](https://modrinth.com/mod/lithium) and [FerriteCore](https://modrinth.com/mod/ferrite-core). Their code is not incorporated into Orbit.

Offline profiles use conventional deterministic `OfflinePlayer:<name>` identities and token `0`. They are prominently labeled; they do not grant game ownership, valid Microsoft authentication, Realms or authenticated multiplayer. Expected offline Realms/session errors can appear in Minecraft's own logs.
