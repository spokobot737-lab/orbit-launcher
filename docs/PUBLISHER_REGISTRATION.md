# Publisher account configuration

Only the **Orbit publisher** registers applications. Ordinary users click Connect and authorize in their browser; they never paste developer IDs, API keys, cookies, passwords or secrets into Orbit. Public IDs are compiled into Rust through the variables below. No client secret is accepted or distributed.

Set public build variables in the PowerShell process that invokes `npm run tauri -- build`. Rebuild when they change. Unconfigured providers show a clear unavailable state and manual profile-link options where appropriate.

| Provider | Build variable | Callback / permissions | Current status and restrictions |
|---|---|---|---|
| Microsoft / Minecraft | `ORBIT_MICROSOFT_CLIENT_ID` | Public native app, personal Microsoft accounts; `http://localhost` with native ephemeral-port exception; `XboxLive.signin offline_access`, S256 PKCE. | Publisher must register its own app and obtain Minecraft Services access for that exact ID. No approved ID is available in this release; authentication is unverified. See [integration research](INTEGRATIONS.md). |
| Spotify | `ORBIT_SPOTIFY_CLIENT_ID` | `http://127.0.0.1:43821/callback`; code + S256 PKCE; `user-read-private user-read-currently-playing`. | Development mode currently allows five allowlisted users and requires a Premium app owner. Wider access requires Spotify approval and current organization/scale eligibility. Refresh is protected by Credential Manager. No registered publisher ID is available here. |
| Discord | `ORBIT_DISCORD_CLIENT_ID` | `http://127.0.0.1:43822/callback`; minimal `identify`, PKCE, callback issuer validation. | Publisher must enable the official public-client capability and confirm a secret-free token exchange is permitted. The current documentation recommends PKCE but also shows secret-based code examples; this build has no secret fallback and is not claimed end-to-end verified. No user-token/self-bot access or friends list. |
| Roblox | `ORBIT_ROBLOX_CLIENT_ID` | `http://127.0.0.1:43823/callback`; public-client code + S256 PKCE, `openid profile`. | Subject to registered redirect acceptance/app eligibility. This grants documented identity access, not the website's session cookie or personalized Home/friends API. No registered publisher ID is available here. |
| X | `ORBIT_X_CLIENT_ID` | `http://127.0.0.1:43824/callback`; native/public client; `users.read tweet.read offline.access`, code + PKCE. | Disabled unless publisher also sets `ORBIT_X_ENABLE_BILLABLE_API=true`. **Do not set that without explicit spending authorization.** X currently lists User Read at $0.010 per resource with prepaid API credits; no credits were purchased or billable requests performed. Manual profile links remain available. |
| Instagram | None in this build | Manual HTTPS profile link. | Current official Instagram Login API targets professional Business/Creator accounts. General personal-account OAuth is not offered. No secret-based professional login flow is bundled. |
| Bluesky | `ORBIT_BLUESKY_CLIENT_METADATA` | HTTPS public JSON client metadata URL; `http://127.0.0.1:43825/callback`, native public client, `atproto`, PKCE + PAR + DPoP via Atrium. | Without publisher metadata, the official localhost development-client exception is used and labeled. It can have short session limits and generic branding. Publisher should serve static metadata for production and test with a real user authorization. No backend secret is needed. |

Example (replace only with the publisher's own public ID):

```powershell
$env:ORBIT_MICROSOFT_CLIENT_ID = '<publisher-public-client-id>'
npm run tauri -- build
```

Never put credentials into `.env` committed to Git, the frontend, logs or SQLite. Public IDs may be source-controlled in a publisher-controlled build configuration; refresh/access tokens and DPoP/private wallet keys may not.

## Bluesky public metadata example

Serve JSON over HTTPS (static hosting is sufficient). The URL must exactly match `client_id`, and declared redirect/scopes must match this build. Hosting approval/TLS availability must be checked before publishing a production ID.

```json
{
  "client_id": "https://publisher.example/orbit/oauth-client-metadata.json",
  "client_name": "Orbit",
  "client_uri": "https://github.com/spokobot737-lab/orbit-launcher",
  "redirect_uris": ["http://127.0.0.1:43825/callback"],
  "scope": "atproto",
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "token_endpoint_auth_method": "none",
  "application_type": "native",
  "dpop_bound_access_tokens": true
}
```

Only public profile and author posts are displayed. AT Protocol repositories are public; Orbit never publishes private contacts, messages or activity there. A DID, wallet address and Orbit communication key are separate identities. External connections are authenticated only after a valid provider flow; manual/restored links are labeled accordingly. Shared links are claims from the friend, not independently verified external ownership at the receiving peer.

## Roblox official-session investigation

The website Home endpoint still requires its own authenticated session. A public OAuth identity token is not that session. Orbit opens `https://www.roblox.com/home` in the **system browser** as an explicit official-session option, using only the browser's own login. It neither inspects that page nor extracts credentials. An embedded WebView or iframe is not assumed to support Roblox login/authorization; a supported third-party Home/friends authorization path has not been established. The native catalog automatically displays **Public Explore**, labeled separately with the Home access limitation. No fixed game IDs or fake friend data are shipped.

## Official references (reviewed 2026-11-08)

- [Microsoft public-client OAuth/PKCE](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow), [redirects](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url), [Minecraft AppID review](https://aka.ms/mce-reviewappid).
- [Spotify quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes), [PKCE](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow), [redirect URIs](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri).
- [Discord OAuth](https://docs.discord.com/developers/topics/oauth2).
- [Roblox public-client OAuth](https://create.roblox.com/docs/cloud/auth/oauth2-develop), [scopes](https://create.roblox.com/docs/cloud/reference/scopes).
- [X OAuth code/PKCE](https://docs.x.com/fundamentals/authentication/oauth-2-0/authorization-code), [pricing](https://docs.x.com/x-api/getting-started/pricing).
- [Meta's Instagram API collection](https://www.postman.com/meta/instagram/documentation/6yqw8pt/instagram-api). Direct Meta documentation returned rate-limit errors in this environment.
- [AT Protocol OAuth](https://atproto.com/specs/oauth), [Atrium OAuth SDK](https://docs.rs/atrium-oauth/0.2.0/atrium_oauth/).

No registration, approval, paid service or account authorization is implied by a successful build. Before enabling a provider in a wider release, test browser success/denial, callback validation, entitlement/eligibility, refresh, disconnect/revocation and quota errors with the publisher's real registration.
