# Orbit 0.4.1 - nonpremium hosting (prerelease)

Windows x64: **Orbit_0.4.1_x64-setup.exe**, or **orbit-launcher.exe** with WebView2 installed. This is an unsigned integration preview. SHA-256 checksums and the embedded playit core license accompany the downloads.

- New servers default to **Nonpremium**. Local nickname profiles connect without Microsoft login. Premium remains an explicit alternative; existing servers retain their mode.
- Correct exact-case offline UUID allowlists, runtime reload/removal, mode-compatible encrypted invitations and profile creation. Host Play approves the selected local nickname; friend approval remains explicit. The allowlist limits names but cannot prevent offline nickname impersonation. No operator permissions are automatically granted.
- **Install, launch and connect** installs/verifies the selected clean 1.21.1 client before direct Quick Play. Mode/version/session checks prevent using changed or incompatible targets.
- Every server shows a copyable **local address** and a separate **internet address** with readiness. Existing official launchers can use these addresses directly; Microsoft login inside Orbit is not required for that.
- Host server starts one-time browser authorization if needed. After approval, a ready server resumes automatically. Orbit's self-managed playit agent requests/reuses a **free global Minecraft Java tunnel** for its exact local port. No developer key, premium allocation or Orbit backend is needed. Each host PC uses its own verified playit account and authorization.
- Pending allocations and uncertain responses are bounded to avoid duplicate creation. Provider quota, verification and permission errors stay visible, with explicit dashboard fallback. No paid upgrade is performed.
- Mode changes require a stopped server and confirmation because UUIDs/inventories/permissions differ. Separate mode-specific allowlists are preserved; player data is not converted. Back up existing worlds first.

**Verified:** 18 native tests; real Java nonpremium rejection/admission locally and actual world entry through a public playit tunnel from the host PC; real free tunnel creation, exact-origin check and cleanup.

Verification and limitations are in [HOSTING_VERIFICATION.md](HOSTING_VERIFICATION.md). Setup: [HOSTING.md](HOSTING.md). The host PC must remain powered on, awake and online. Provider/network availability is not guaranteed. Integrated Premium Microsoft/Minecraft sign-in still requires Orbit's approved publisher registration; Nonpremium does not grant game ownership or authenticated multiplayer access. A genuinely separate-network player and two-player gameplay remain unverified.
