import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open, confirm } from "@tauri-apps/plugin-dialog";
import {
  Server,
  Plus,
  Play,
  Square,
  RefreshCw,
  Copy,
  Users,
  Archive,
  Terminal,
  Settings,
  Globe,
  ShieldCheck,
  ArrowUpRight,
  X,
} from "lucide-react";
import { api, errorText } from "./api";
import "./servers.css";
const fresh = (memory = 2048) => ({
  id: "",
  name: "My private world",
  version: "1.21.1",
  loader: "vanilla",
  authMode: "offline",
  memory,
  players: 6,
  difficulty: "normal",
  gameMode: "survival",
  seed: "",
  viewDistance: 8,
  simulationDistance: 6,
  port: 25565,
  access: "playit",
  address: "",
  acceptEula: false,
});
function useClock() {
  const [time, setTime] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setTime(Date.now()), 15000);
    return () => clearInterval(t);
  }, []);
  return time;
}
export function ServerJoin({
  target,
  onCreated,
}: {
  target: any;
  onCreated?: () => void;
}) {
  const [data, setData] = useState<any>(null),
    [nickname, setNickname] = useState("Player"),
    [profile, setProfile] = useState(""),
    [busy, setBusy] = useState(""),
    [error, setError] = useState("");
  useEffect(() => {
    setData(null);
    setProfile("");
    setError("");
  }, [target.hostingId, target.peer, target.server]);
  async function options() {
    setBusy("check");
    setError("");
    try {
      const result = await api("server_join_options", target);
      setData(result);
      setProfile(result.profiles[0]?.id || "");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy("");
    }
  }
  async function action(op: string, args: any) {
    setBusy(op);
    setError("");
    try {
      await api(op, args);
      onCreated?.();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy("");
    }
  }
  return (
    <div className="server-join">
      {!data ? (
        <button className="primary" disabled={!!busy} onClick={options}>
          <Play size={14} />
          {busy ? "Checking…" : "Play / Join"}
        </button>
      ) : (
        <>
          <p className="muted">{data.note}</p>
          {data.profiles.length > 0 && (
            <label>
              Compatible client
              <select
                value={profile}
                onChange={(e) => setProfile(e.target.value)}
              >
                {data.profiles.map((p: any) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.loader}
                  </option>
                ))}
              </select>
            </label>
          )}
          {data.target.authMode === "offline" && (
            <label>
              Nickname for a new nonpremium profile
              <input
                value={nickname}
                maxLength={16}
                pattern="[A-Za-z0-9_]{3,16}"
                onChange={(e) => setNickname(e.target.value)}
              />
              <small>
                Use the exact spelling approved by the host. Existing profiles
                keep their own nickname.
              </small>
            </label>
          )}
          <div className="button-row">
            {profile && (
              <>
                <button
                  className="secondary"
                  disabled={!!busy}
                  onClick={() => action("mc_install", { id: profile })}
                >
                  Install / Repair client
                </button>
                <button
                  className="primary"
                  disabled={!!busy}
                  onClick={() => action("server_join", { ...target, profile })}
                >
                  <Play size={14} />
                  {busy === "server_join"
                    ? "Installing / launching…"
                    : "Install, launch and connect"}
                </button>
              </>
            )}
            <button
              className="secondary"
              disabled={!!busy}
              onClick={async () => {
                setBusy("create");
                try {
                  const p = await api("server_create_client", {
                    ...target,
                    nickname,
                  });
                  await options();
                  setProfile(p.id);
                  onCreated?.();
                } catch (e) {
                  setError(errorText(e));
                } finally {
                  setBusy("");
                }
              }}
            >
              Create clean compatible profile
            </button>
            <button className="text-button" onClick={() => setData(null)}>
              Close
            </button>
          </div>
          <small>
            {data.target.authMode === "offline"
              ? "Nonpremium: no Microsoft sign-in. Host Play approves the selected profile's nickname locally. Friends need host approval. Offline mode does not grant game ownership or access to authenticated servers."
              : "Premium: Microsoft sign-in and game ownership are required. Host Play approves your authenticated account. Friends need host allowlist approval."}
          </small>
        </>
      )}
      {busy && (
        <p className="muted">
          {busy === "mc_install"
            ? "Installing verified client files…"
            : busy === "create"
              ? "Creating profile…"
              : "Working…"}
        </p>
      )}
      {error && (
        <p className="alert error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
export function ServerInvitationCard({
  body,
  peer,
  outgoing,
}: {
  body: any;
  peer: string;
  outgoing: boolean;
}) {
  const now = useClock();
  const [latest, setLatest] = useState<any>(body);
  useEffect(() => {
    const refresh = () =>
      api<any[]>("server_shared")
        .then((rows) => {
          const found = rows.find(
            (r) => r.peer === peer && r.server === body.server,
          );
          if (found) setLatest(found);
        })
        .catch(() => {});
    refresh();
    const un = listen("social-changed", refresh);
    return () => {
      un.then((f) => f());
    };
  }, [peer, body.server]);
  const fresh = latest.online === true && latest.expires * 1000 > now;
  return (
    <section className="server-message-card">
      <strong>
        <Server size={16} /> {body.name}
      </strong>
      <p>
        Minecraft {body.version} · {body.loader} · no required mods
      </p>
      <span className={`server-badge ${fresh ? "online" : "offline"}`}>
        {fresh ? "Recently announced · check on Join" : "Expired / offline"}
      </span>
      <p className="muted">
        {latest.authMode === "offline"
          ? "Nonpremium · nickname allowlist; identity is not verified."
          : "Premium · Minecraft authentication and host approval required."}{" "}
        This card does not prove the host is still online.
      </p>
      {(fresh || outgoing) && (
        <ServerJoin
          target={
            outgoing
              ? { hostingId: body.server }
              : { peer, server: body.server }
          }
        />
      )}
    </section>
  );
}
export default function Servers({
  onLibraryChanged,
}: {
  onLibraryChanged: () => void;
}) {
  const [servers, setServers] = useState<any[]>([]),
    [shared, setShared] = useState<any[]>([]),
    [friends, setFriends] = useState<any[]>([]),
    [tunnel, setTunnel] = useState<any>({}),
    [memory, setMemory] = useState<any>({}),
    [loaded, setLoaded] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(""),
    [edit, setEdit] = useState<any>(null),
    [world, setWorld] = useState(""),
    [selected, setSelected] = useState(""),
    [tab, setTab] = useState("overview"),
    [logs, setLogs] = useState<string[]>([]),
    [backups, setBackups] = useState<any[]>([]),
    [allowlist, setAllowlist] = useState<any[]>([]),
    [player, setPlayer] = useState(""),
    [approved, setApproved] = useState(false),
    [peer, setPeer] = useState("");
  const now = useClock();
  const load = useCallback(async () => {
    const [a, b, c, d, e] = await Promise.all([
      api<any[]>("server_list"),
      api<any[]>("server_shared"),
      api<any[]>("orbit_contacts"),
      api("server_tunnel_status"),
      api("server_memory"),
    ]);
    setServers(a);
    setShared(b);
    setFriends(c.filter((x) => x.state === "accepted"));
    setTunnel(d);
    setMemory(e);
    setLoaded(true);
  }, []);
  useEffect(() => {
    load().catch((e) => {
      setError(errorText(e));
      setLoaded(true);
    });
    const un = listen("servers-changed", () => load().catch(() => {})),
      social = listen("social-changed", () => load().catch(() => {}));
    return () => {
      un.then((f) => f());
      social.then((f) => f());
    };
  }, [load]);
  const current = servers.find((s) => s.id === selected);
  useEffect(() => {
    if (!selected && servers.length) setSelected(servers[0].id);
  }, [servers, selected]);
  const detail = useCallback(async () => {
    if (!selected) return;
    if (tab === "logs") setLogs(await api("server_logs", { id: selected }));
    if (tab === "backups")
      setBackups(await api("server_backups", { id: selected }));
    if (tab === "allowlist")
      setAllowlist(await api("server_allowlist_get", { id: selected }));
  }, [selected, tab]);
  useEffect(() => {
    detail().catch(() => {});
    const un = listen("servers-changed", () => detail().catch(() => {}));
    return () => {
      un.then((f) => f());
    };
  }, [detail]);
  async function run(key: string, fn: () => Promise<any>, success = "") {
    setBusy(key);
    setError("");
    try {
      const result = await fn();
      await load();
      await detail();
      if (success) setNotice(success);
      return result;
    } catch (e) {
      setError(errorText(e));
      throw e;
    } finally {
      setBusy("");
    }
  }
  const act = (op: string, args: any = {}, success = "") =>
    run(op, () => api(op, args), success).catch(() => {});
  const active = (s: any) =>
    !["Offline", "Error"].includes(s.runtime?.phase || "Offline");
  return (
    <section className="servers-area">
      <div className="server-heading">
        <div>
          <span className="eyebrow">YOUR PC. YOUR WORLD.</span>
          <h1>Minecraft servers</h1>
          <p>Host a private world here. Friends connect over the internet.</p>
        </div>
        <button
          className="primary"
          onClick={() => {
            setEdit(fresh(memory.recommendedServerMiB));
            setWorld("");
          }}
        >
          <Plus size={16} />
          Create server
        </button>
      </div>
      {error && (
        <p className="alert error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="alert" role="status">
          {notice}
        </p>
      )}
      <div className="server-network">
        <Globe size={22} />
        <div>
          <h3>Online access · playit.gg</h3>
          <p>
            {tunnel.authorized
              ? "This installation is authorized. Hosting requests or reuses a free Minecraft Java tunnel automatically."
              : "One-time setup on each host PC: authorize Orbit with your own verified playit account in the browser. Orbit then requests the free Minecraft Java tunnel. Joining players need no agent or playit account."}
          </p>
          <small>
            {current
              ? `Tunnel origin for ${current.name}: 127.0.0.1:${current.port}. `
              : "Default origin: 127.0.0.1:25565. "}
            No router changes or paid plan. Availability and latency depend on
            playit and your network.
          </small>
          {tunnel.phase && <p>{tunnel.phase}</p>}
          {tunnel.error && <p className="error">{tunnel.error}</p>}
        </div>
        <div className="button-row">
          <button
            disabled={!!busy}
            className="secondary"
            onClick={() =>
              act(
                "server_tunnel_setup",
                {},
                "Authorize this installation in your browser. If already hosting, Orbit resumes online setup automatically after approval.",
              )
            }
          >
            {tunnel.authorized ? "Check authorization" : "Set up online access"}
            <ArrowUpRight size={14} />
          </button>
          {tunnel.authorized && current && (
            <button
              className="secondary"
              disabled={!!busy}
              onClick={() =>
                act(
                  "server_tunnel_configure",
                  { id: current.id },
                  "Free tunnel requested or reused. Wait for allocation; do not buy a plan.",
                )
              }
            >
              Configure free tunnel
            </button>
          )}
          <button
            className="text-button"
            onClick={() => act("server_tunnel_dashboard")}
          >
            Open playit dashboard
          </button>
        </div>
      </div>
      <details className="server-memory">
        <summary>Advanced online access</summary>
        <p>
          If the agent was revoked or you want a different playit account, stop
          hosting and disconnect this installation first. This removes only the
          local host credential; provider-side revocation remains in your
          dashboard.
        </p>
        <button
          className="text-button danger"
          disabled={!!busy || servers.some(active)}
          onClick={async () => {
            if (
              await confirm(
                "Remove this installation's playit authorization? Stop hosting first. Your worlds, Orbit profile and other credentials remain unchanged.",
                { title: "Disconnect this host?", kind: "warning" },
              )
            )
              act(
                "server_tunnel_reset",
                { confirmed: true },
                "Local host authorization removed. You can authorize again in the browser.",
              );
          }}
        >
          Disconnect this host…
        </button>
      </details>
      <p className="server-memory">
        {memory.availableMiB
          ? `${Math.round(memory.availableMiB / 1024)} GiB available of ${Math.round(memory.totalMiB / 1024)} GiB · `
          : ""}
        {memory.note} Hosting requires this PC to remain powered on, online and
        awake. Orbit does not change your power settings.
      </p>
      <h2>My servers</h2>
      {!loaded ? (
        <p>Loading servers…</p>
      ) : servers.length === 0 ? (
        <div className="server-empty">
          <Server size={32} />
          <h3>A world for your circle</h3>
          <p>
            Create a server to install verified files and host from this
            computer.
          </p>
        </div>
      ) : (
        <div className="server-grid">
          {servers.map((s) => (
            <button
              className={`server-tile ${selected === s.id ? "selected" : ""}`}
              key={s.id}
              onClick={() => {
                setSelected(s.id);
                setTab("overview");
              }}
            >
              <div className="server-tile-art">
                <Server size={30} />
                <span
                  className={`server-badge ${s.runtime.phase.toLowerCase().replaceAll(" ", "-")}`}
                >
                  {s.runtime.phase}
                </span>
              </div>
              <strong>{s.name}</strong>
              <p>
                {s.version} ·{" "}
                {s.loader === "fabric" ? "Fabric 0.16.14 · no mods" : "Vanilla"}
              </p>
              <small>
                {s.memory} MiB · up to {s.players} configured slots
              </small>
            </button>
          ))}
        </div>
      )}
      {current && (
        <section className="server-detail">
          <div className="server-heading">
            <div>
              <h2>{current.name}</h2>
              <p>
                {current.runtime.phase}
                {current.runtime.pid ? ` · PID ${current.runtime.pid}` : ""}
              </p>
            </div>
            <div className="button-row">
              <button
                disabled={!!busy || active(current)}
                className="primary"
                onClick={() =>
                  run("host", async () => {
                    await api("server_start", { id: selected });
                    if (current.access === "playit" && !tunnel.authorized)
                      await api("server_tunnel_setup");
                  }).catch(() => {})
                }
              >
                <Play size={14} />
                Host server
              </button>
              <button
                disabled={!!busy || !active(current)}
                className="secondary"
                onClick={() => act("server_stop", { id: selected })}
              >
                <Square size={14} />
                {busy === "server_stop" ? "Saving and stopping…" : "Stop"}
              </button>
              <button
                disabled={!!busy || !active(current)}
                className="secondary"
                onClick={() => act("server_restart", { id: selected })}
              >
                <RefreshCw size={14} />
                Restart
              </button>
            </div>
          </div>
          <div className="server-detail-tabs">
            {[
              ["overview", "Overview"],
              ["allowlist", "Allowlist"],
              ["invite", "Invite friends"],
              ["logs", "Logs"],
              ["backups", "Backups"],
            ].map(([id, label]) => (
              <button
                className={tab === id ? "active" : ""}
                key={id}
                onClick={() => setTab(id)}
              >
                {label}
              </button>
            ))}
            <button
              onClick={() => {
                setEdit({ ...current, acceptEula: false });
                setWorld("");
              }}
              disabled={active(current)}
            >
              <Settings size={13} />
              Settings
            </button>
          </div>
          {current.runtime.error && (
            <p className="alert error">{current.runtime.error}</p>
          )}
          {tab === "overview" && (
            <>
              <div className="server-facts">
                <span>
                  <strong>
                    {current.runtime.localReady ? "Ready" : "Waiting"}
                  </strong>
                  Local Minecraft
                </span>
                <span>
                  <strong>
                    {current.runtime.tunnelReady ? "Ready" : "Not ready"}
                  </strong>
                  External access
                </span>
                <span>
                  <strong>{current.runtime.players?.online ?? "—"}</strong>
                  Connected players
                </span>
                <span>
                  <strong>
                    {current.runtime.residentMiB
                      ? `${current.runtime.residentMiB} MiB`
                      : "—"}
                  </strong>
                  Resident memory · {current.memory} MiB maximum heap
                </span>
              </div>
              <div className="server-address">
                <span>
                  <small>This PC · Minecraft Java {current.version}</small>
                  <code>127.0.0.1:{current.port}</code>
                </span>
                <button
                  className="secondary"
                  onClick={() =>
                    navigator.clipboard
                      .writeText(`127.0.0.1:${current.port}`)
                      .catch((e) => setError(errorText(e)))
                  }
                >
                  <Copy size={14} />
                  Copy local address
                </button>
              </div>
              <p className="muted">
                {current.authMode === "offline"
                  ? "Nonpremium · nicknames are not verified. Add the exact player nickname in Allowlist before joining."
                  : "Premium · authenticated Minecraft accounts only."}{" "}
                You can also use your existing launcher: start Java{" "}
                {current.version}, Multiplayer → Direct Connection, and paste
                the address.
              </p>
              {current.runtime.address && (
                <div className="server-address">
                  <span>
                    <small>
                      Internet · for friends{" "}
                      {current.runtime.tunnelReady
                        ? "· responding from this PC"
                        : "· not ready yet"}
                    </small>
                    <code>{current.runtime.address}</code>
                  </span>
                  <button
                    className="secondary"
                    onClick={() =>
                      navigator.clipboard
                        .writeText(current.runtime.address)
                        .catch((e) => setError(errorText(e)))
                    }
                  >
                    <Copy size={14} />
                    Copy address
                  </button>
                </div>
              )}
              {!current.runtime.address && (
                <div className="server-setup-steps">
                  <strong>Internet address pending</strong>
                  <p>
                    {!tunnel.authorized
                      ? "1. Set up online access above and approve this host in your browser."
                      : "1. Host server, then wait for automatic free tunnel allocation. Use Configure free tunnel to retry."}
                  </p>
                  <p>
                    2. If playit requires email verification or refuses
                    allocation, finish setup in its dashboard. Choose Minecraft
                    Java / Free Network and use the origin shown above, with
                    Proxy Protocol disabled.
                  </p>
                  <p>
                    3. When the public address appears here, copy it for your
                    friends and approve their nicknames in Allowlist.
                  </p>
                </div>
              )}
              {current.runtime.setupError && (
                <p className="alert error">{current.runtime.setupError}</p>
              )}
              <p>
                {current.runtime.accessNote ||
                  "The server runs as a separate managed Java process. Leaving Minecraft does not stop it. Stop saves the world and disconnects players."}
              </p>
              <p className="muted">
                External-player verification: not independently verified. A
                tunnel handshake or status check from this PC does not prove
                another player joined from another network.
              </p>
              {current.runtime.localReady && (
                <ServerJoin
                  target={{ hostingId: selected }}
                  onCreated={onLibraryChanged}
                />
              )}
              <div className="button-row">
                <button
                  className="secondary"
                  disabled={!!busy || !current.runtime.localReady}
                  onClick={() => act("server_tunnel_retry", { id: selected })}
                >
                  <RefreshCw size={14} />
                  Reconnect online access
                </button>
                <button
                  className="text-button danger"
                  disabled={!!busy || !active(current)}
                  onClick={async () => {
                    if (
                      await confirm(
                        "Force termination may lose unsaved world progress. Use only if graceful Stop failed.",
                        {
                          title: "Force stop managed server?",
                          kind: "warning",
                        },
                      )
                    )
                      act("server_force", { id: selected, confirmed: true });
                  }}
                >
                  Force stop…
                </button>
              </div>
            </>
          )}
          {tab === "invite" && (
            <>
              <h3>Invite accepted friends</h3>
              <p>
                This sends an encrypted server card.{" "}
                {current.authMode === "offline"
                  ? "Approve your friend's exact nickname in Allowlist. Nonpremium nicknames are not verified identities."
                  : "Approve your friend's authenticated Minecraft account separately."}
              </p>
              {friends.length === 0 ? (
                <p>Add an accepted friend under Conversations first.</p>
              ) : (
                <div className="button-row">
                  <select
                    aria-label="Friend to invite"
                    value={peer}
                    onChange={(e) => setPeer(e.target.value)}
                  >
                    <option value="">Choose a friend</option>
                    {friends.map((f) => (
                      <option key={f.id} value={f.id}>
                        {f.name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="primary"
                    disabled={
                      !!busy || !peer || current.runtime.phase !== "Online"
                    }
                    onClick={() =>
                      act(
                        "server_share",
                        { id: selected, peer },
                        "Server card queued. Approve the friend's Minecraft account separately in Allowlist.",
                      )
                    }
                  >
                    <Users size={14} />
                    Send server card
                  </button>
                </div>
              )}
              <small>
                Pending requests are not accepted friends. Updates expire after
                150 seconds. Removing an Orbit friend stops future
                announcements; it does not remove their Minecraft allowlist
                entry.
              </small>
            </>
          )}
          {tab === "allowlist" && (
            <>
              <h3>
                <ShieldCheck size={17} />
                Private access
              </h3>
              <p>
                {current.authMode === "offline"
                  ? "Nonpremium: add the exact nickname, including upper/lower case. No Microsoft account or Mojang lookup is required. The allowlist limits names, but anyone who claims an approved name can impersonate that player. No operator permissions are granted."
                  : "Premium: resolve and approve an authenticated Minecraft account. Minecraft verifies ownership when the player logs in. No operator permissions are granted."}
              </p>
              <label>
                {current.authMode === "offline"
                  ? "Exact player nickname"
                  : "Minecraft account name"}
                <input
                  value={player}
                  maxLength={16}
                  onChange={(e) => setPlayer(e.target.value)}
                />
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={approved}
                  onChange={(e) => setApproved(e.target.checked)}
                />
                I approve this account for my server
              </label>
              <button
                className="primary"
                disabled={!!busy || !approved || !player}
                onClick={() =>
                  act(
                    "server_allowlist",
                    { id: selected, name: player, approved: true },
                    "Player approval saved or queued. Check server logs before joining.",
                  )
                }
              >
                {current.authMode === "offline"
                  ? "Approve nickname"
                  : "Resolve and approve account"}
              </button>
              {allowlist.map((p) => (
                <div className="server-list-row" key={p.uuid}>
                  <span>
                    {p.name}
                    <small>{p.uuid}</small>
                  </span>
                  <button
                    className="text-button danger"
                    disabled={!!busy}
                    onClick={async () => {
                      if (
                        await confirm(
                          `Remove ${p.name} and disconnect that player?`,
                          {
                            title: "Remove Minecraft access?",
                            kind: "warning",
                          },
                        )
                      )
                        act("server_allowlist", {
                          id: selected,
                          name: p.name,
                          approved: true,
                          remove: true,
                        });
                    }}
                  >
                    Remove and disconnect
                  </button>
                </div>
              ))}
            </>
          )}
          {tab === "logs" && (
            <>
              <h3>
                <Terminal size={16} />
                Server logs
              </h3>
              <small>
                Last 400 bounded lines from this managed process. Authentication
                failures, startup and player events appear here.
              </small>
              <pre className="server-log">
                {logs.join("\n") || "Start the server to collect logs."}
              </pre>
              <button
                className="secondary"
                onClick={() => detail().catch((e) => setError(errorText(e)))}
              >
                Refresh logs
              </button>
            </>
          )}
          {tab === "backups" && (
            <>
              <h3>
                <Archive size={16} />
                World backups
              </h3>
              <p>
                Stop the server gracefully before copying. Every restore creates
                a safety backup and preserves the previous world directory.
                Current authentication and allowlist settings remain enforced.
              </p>
              <div className="button-row">
                <button
                  className="primary"
                  disabled={!!busy || active(current)}
                  onClick={() =>
                    act(
                      "server_backup",
                      { id: selected },
                      "Consistent stopped-world backup created.",
                    )
                  }
                >
                  Create stopped-world backup
                </button>
                <button
                  className="secondary"
                  disabled={!!busy || active(current)}
                  onClick={async () => {
                    const path = await open({
                      directory: true,
                      multiple: false,
                    });
                    if (
                      typeof path === "string" &&
                      (await confirm(
                        "Copy this compatible 1.21.1 world into this server? The original remains unchanged. Any current server world is backed up first.",
                        { title: "Import a copy?", kind: "warning" },
                      ))
                    )
                      act(
                        "server_import",
                        { id: selected, path, confirmed: true },
                        "World copied. Original preserved.",
                      );
                  }}
                >
                  Import world copy
                </button>
              </div>
              <small>
                Retention is manual: delete individual backups below. Nothing is
                silently pruned; safety copies also occupy disk.
              </small>
              {backups.length === 0 && <p>No backups yet.</p>}
              {backups.map((b) => (
                <div className="server-backup" key={b.id}>
                  <strong>
                    {new Date(b.created * 1000).toLocaleString()} ·{" "}
                    {(b.bytes / 1024 / 1024).toFixed(1)} MiB
                  </strong>
                  <code>{b.path}</code>
                  <div className="button-row">
                    <button
                      className="secondary"
                      disabled={!!busy || active(current)}
                      onClick={async () => {
                        if (
                          await confirm(
                            "Replace the stopped server world with this backup? A safety backup will be created first.",
                            { title: "Restore world backup?", kind: "warning" },
                          )
                        )
                          act(
                            "server_restore",
                            { id: selected, backup: b.id, confirmed: true },
                            "World restored; previous world and safety backup retained.",
                          );
                      }}
                    >
                      Restore…
                    </button>
                    <button
                      className="text-button danger"
                      disabled={!!busy || active(current)}
                      onClick={async () => {
                        if (
                          await confirm("Permanently delete this backup?", {
                            title: "Delete backup?",
                            kind: "warning",
                          })
                        )
                          act("server_delete_backup", {
                            id: selected,
                            backup: b.id,
                            confirmed: true,
                          });
                      }}
                    >
                      Delete…
                    </button>
                  </div>
                </div>
              ))}
            </>
          )}
        </section>
      )}
      <h2>Servers shared by friends</h2>
      {shared.length === 0 ? (
        <div className="server-empty">
          <Users size={25} />
          <p>Accepted friends' encrypted server cards appear here.</p>
        </div>
      ) : (
        <div className="server-grid">
          {shared.map((c) => (
            <article className="server-friend" key={`${c.peer}:${c.server}`}>
              <h3>{c.name}</h3>
              <p>
                {friends.find((f) => f.id === c.peer)?.name ||
                  "Accepted friend"}{" "}
                · {c.version} · {c.loader}
              </p>
              <span className="server-badge">
                {c.online && c.expires * 1000 > now
                  ? "Recently announced"
                  : "Expired / offline"}
              </span>
              <small>
                No required mods ·{" "}
                {c.authMode === "offline"
                  ? "nonpremium nickname"
                  : "premium account"}{" "}
                · host allowlist approval required.
              </small>
              {c.online && c.expires * 1000 > now && (
                <ServerJoin
                  target={{ peer: c.peer, server: c.server }}
                  onCreated={onLibraryChanged}
                />
              )}
            </article>
          ))}
        </div>
      )}
      {edit && (
        <div className="modal-backdrop">
          <form
            className="modal server-editor"
            onSubmit={async (e) => {
              e.preventDefault();
              if (
                edit.id &&
                (current?.authMode || "online") !== (edit.authMode || "online")
              ) {
                if (
                  !(await confirm(
                    "Changing premium/nonpremium mode changes player UUIDs. Inventories, pets and permissions may appear different. Each mode keeps its own allowlist; no player data is converted. Back up the world first.",
                    { title: "Change player identity mode?", kind: "warning" },
                  ))
                )
                  return;
                edit.confirmAuthChange = true;
              }
              try {
                const saved = await run(
                  "save",
                  async () => {
                    const saved = await api("server_save", edit);
                    setEdit(saved);
                    setSelected(saved.id);
                    if (world)
                      await api("server_import", {
                        id: saved.id,
                        path: world,
                        confirmed: true,
                      });
                    return saved;
                  },
                  "Server saved. Review Allowlist, then Host server.",
                );
                setSelected(saved.id);
                setEdit(null);
              } catch {
                void load().catch(() => {});
              }
            }}
          >
            <div className="server-heading">
              <h2>{edit.id ? "Server settings" : "Create server"}</h2>
              <button
                type="button"
                className="icon-btn"
                aria-label="Close server editor"
                disabled={!!busy}
                onClick={() => setEdit(null)}
              >
                <X size={18} />
              </button>
            </div>
            <div className="server-form-grid">
              <label>
                Name
                <input
                  required
                  maxLength={80}
                  value={edit.name}
                  onChange={(e) => setEdit({ ...edit, name: e.target.value })}
                />
              </label>
              <label>
                Minecraft version
                <select value={edit.version} disabled>
                  <option>1.21.1</option>
                </select>
              </label>
              <label>
                Loader
                <select
                  disabled={!!edit.id}
                  value={edit.loader}
                  onChange={(e) => setEdit({ ...edit, loader: e.target.value })}
                >
                  <option value="vanilla">Vanilla</option>
                  <option value="fabric">Fabric 0.16.14 · no mods</option>
                </select>
              </label>
              <label>
                Player authentication
                <select
                  value={edit.authMode || "online"}
                  onChange={(e) =>
                    setEdit({ ...edit, authMode: e.target.value })
                  }
                >
                  <option value="offline">Nonpremium · local nicknames</option>
                  <option value="online">Premium · Microsoft accounts</option>
                </select>
              </label>
              <label>
                Memory (MiB)
                <input
                  type="number"
                  min={1024}
                  max={16384}
                  step={256}
                  value={edit.memory}
                  onChange={(e) =>
                    setEdit({ ...edit, memory: +e.target.value })
                  }
                />
              </label>
              <label>
                Player limit
                <input
                  type="number"
                  min={1}
                  max={20}
                  value={edit.players}
                  onChange={(e) =>
                    setEdit({ ...edit, players: +e.target.value })
                  }
                />
              </label>
              <label>
                Difficulty
                <select
                  value={edit.difficulty}
                  onChange={(e) =>
                    setEdit({ ...edit, difficulty: e.target.value })
                  }
                >
                  {["peaceful", "easy", "normal", "hard"].map((v) => (
                    <option key={v}>{v}</option>
                  ))}
                </select>
              </label>
              <label>
                Game mode
                <select
                  value={edit.gameMode}
                  onChange={(e) =>
                    setEdit({ ...edit, gameMode: e.target.value })
                  }
                >
                  {["survival", "creative", "adventure", "spectator"].map(
                    (v) => (
                      <option key={v}>{v}</option>
                    ),
                  )}
                </select>
              </label>
              <label>
                Optional seed
                <input
                  maxLength={64}
                  value={edit.seed}
                  onChange={(e) => setEdit({ ...edit, seed: e.target.value })}
                />
              </label>
              <label>
                View distance
                <input
                  type="number"
                  min={2}
                  max={16}
                  value={edit.viewDistance}
                  onChange={(e) =>
                    setEdit({ ...edit, viewDistance: +e.target.value })
                  }
                />
              </label>
              <label>
                Simulation distance
                <input
                  type="number"
                  min={2}
                  max={12}
                  value={edit.simulationDistance}
                  onChange={(e) =>
                    setEdit({ ...edit, simulationDistance: +e.target.value })
                  }
                />
              </label>
            </div>
            {edit.authMode === "offline" && (
              <p className="alert">
                Nonpremium mode accepts local nickname profiles without
                Microsoft sign-in. Nicknames can be impersonated; the allowlist
                is not identity verification. Game ownership and authenticated
                Minecraft services remain separate.
              </p>
            )}
            <p className="muted">
              {memory.note} Player slots are a configuration limit, not a
              capacity guarantee. Only this version and bare loaders are
              supported. Modpacks and version changes are not silently
              installed.
            </p>
            {(!edit.id || !!world) && (
              <div className="server-world-choice">
                <strong>
                  {world
                    ? "Import a copy of this 1.21.1 world"
                    : "Create a new world"}
                </strong>
                {world && <code>{world}</code>}
                <button
                  className="secondary"
                  type="button"
                  onClick={async () => {
                    const path = await open({
                      directory: true,
                      multiple: false,
                    });
                    if (typeof path === "string") setWorld(path);
                  }}
                >
                  Choose existing world…
                </button>
                {world && (
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => setWorld("")}
                  >
                    Use a new world
                  </button>
                )}
                <small>
                  Close the original world first. Only DataVersion 3955 is
                  accepted; the original is never moved or modified. Upgrades
                  can prevent later downgrades.
                </small>
              </div>
            )}
            <details>
              <summary>Advanced connection settings</summary>
              <label>
                Access
                <select
                  value={edit.access}
                  onChange={(e) => setEdit({ ...edit, access: e.target.value })}
                >
                  <option value="playit">playit.gg (recommended)</option>
                  <option value="manual">
                    Manual public address / port forwarding
                  </option>
                </select>
              </label>
              <label>
                Local TCP port
                <input
                  type="number"
                  min={1024}
                  max={65535}
                  value={edit.port}
                  onChange={(e) => setEdit({ ...edit, port: +e.target.value })}
                />
              </label>
              {edit.access === "manual" && (
                <>
                  <label>
                    Public Minecraft address
                    <input
                      value={edit.address}
                      placeholder="host.example:25565"
                      onChange={(e) =>
                        setEdit({ ...edit, address: e.target.value })
                      }
                    />
                  </label>
                  <p>
                    Requires a reachable address and correct router/firewall
                    rules; often unavailable behind CGNAT. This mode binds the
                    server's port to network interfaces. Orbit does not
                    configure your router or firewall.
                  </p>
                </>
              )}
            </details>
            <button
              type="button"
              className="text-button"
              onClick={() => act("server_eula")}
            >
              Read Minecraft server EULA ↗
            </button>
            {!edit.eulaAcceptedAt && (
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={edit.acceptEula}
                  onChange={(e) =>
                    setEdit({ ...edit, acceptEula: e.target.checked })
                  }
                />
                I reviewed and accept the Minecraft EULA for this server
              </label>
            )}
            {error && <p className="alert error">{error}</p>}
            <div className="button-row">
              <button
                className="primary"
                disabled={!!busy || (!edit.eulaAcceptedAt && !edit.acceptEula)}
                type="submit"
              >
                {busy ? "Saving…" : "Save server"}
              </button>
              <button
                className="secondary"
                disabled={!!busy}
                type="button"
                onClick={() => setEdit(null)}
              >
                Cancel
              </button>
            </div>
          </form>
        </div>
      )}
    </section>
  );
}
