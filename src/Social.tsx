import ProfileCard from "./ProfileCard";
import NetworkSettings from "./NetworkSettings";
import ProfileActivity, { SharedActivity } from "./ProfileActivity";
import React, { useState, useEffect, useRef, useCallback } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import { QRCodeSVG } from "qrcode.react";
import {
  UserRound,
  Users,
  MessageSquare,
  Radio,
  Link2,
  ShieldCheck,
  Shield,
  Wallet,
  Plus,
  ArrowUpRight,
  Copy,
  Check,
  X,
  Settings,
  Send,
  Paperclip,
  Image,
  Download,
  RefreshCw,
  File,
  Film,
  Music2,
  Gamepad2,
  ChevronRight,
  LockKeyhole,
  HardDrive,
  Cloud,
  Eye,
  EyeOff,
  CheckCheck,
  AlertCircle,
  FolderOpen,
  Camera,
  MoreHorizontal,
  LogOut,
} from "lucide-react";
import { api, errorText, native, imageUrl } from "./api";
import { Heading, Empty } from "./components";
import "./social.css";
type Profile = {
  id: string;
  npub: string;
  name: string;
  bio: string;
  avatar?: string;
  banner?: string;
  gameVisibility: boolean;
  musicVisibility: boolean;
};
type Contact = {
  id: string;
  name: string;
  bio?: string;
  avatar?: string;
  connections?: Record<string, { name?: string; url?: string }>;
  state: string;
  wallet?: any;
  banner?: string;
  imagesPending?: boolean;
};
type Attachment = {
  id: string;
  name: string;
  type: string;
  size: number;
  state?: string;
  url?: string;
};
type Message = {
  id: string;
  direction: string;
  kind: string;
  created: number;
  state: string;
  error?: string;
  body: { text?: string; attachments?: Attachment[]; message?: string };
};
const short = (s: string) => (s ? `${s.slice(0, 12)}…${s.slice(-7)}` : "");
function ProviderImage({ url, alt }: { url: string; alt: string }) {
  const [src, setSrc] = useState("");
  useEffect(() => {
    let active = true;
    setSrc("");
    imageUrl(url)
      .then((v) => {
        if (active) setSrc(v);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [url]);
  return src ? <img src={src} alt={alt} /> : null;
}
const bytes = (n: number) =>
  n >= 1024 ** 3
    ? `${(n / 1024 ** 3).toFixed(2)} GiB`
    : n >= 1024 ** 2
      ? `${(n / 1024 ** 2).toFixed(1)} MiB`
      : n >= 1024
        ? `${(n / 1024).toFixed(0)} KiB`
        : `${n} B`;
function Avatar({ person, size = "" }: { person: any; size?: string }) {
  return (
    <span className={`social-avatar ${size}`}>
      {person?.avatar ? (
        <img src={person.avatar} alt="" />
      ) : (
        (person?.name?.[0] || "O").toUpperCase()
      )}
    </span>
  );
}
const providers = [
  {
    id: "microsoft",
    name: "Microsoft / Minecraft",
    letter: "⊞",
    color: "#91a7d6",
    note: "Java ownership and profile verification. Requires Orbit’s approved publisher registration.",
  },
  {
    id: "roblox",
    name: "Roblox",
    letter: "◈",
    color: "#c7bbc9",
    note: "OAuth connects your identity. Personalized Home and friends require Roblox’s official site session.",
  },
  {
    id: "spotify",
    name: "Spotify",
    letter: "≋",
    color: "#8fb49a",
    note: "Profile and playback activity. Development access: five allowlisted users; Premium app owner required.",
  },
  {
    id: "discord",
    name: "Discord",
    letter: "◡",
    color: "#a69de1",
    note: "Profile only. Public-client authorization must be enabled by the publisher; no user tokens or self-bots.",
  },
  {
    id: "x",
    name: "X",
    letter: "𝕏",
    color: "#b6b5ba",
    note: "Profile API is billable and disabled by default. A manual profile link works without API access.",
  },
  {
    id: "instagram",
    name: "Instagram",
    letter: "◎",
    color: "#c397b5",
    note: "General personal-account login is unavailable. Add a manual link; professional accounts require Meta eligibility.",
  },
  {
    id: "bluesky",
    name: "Bluesky",
    letter: "⋈",
    color: "#91b4dd",
    note: "Official OAuth with a separate DID. Only public profile/posts are read. Local-client sessions may be short-lived.",
  },
];
export default function SocialView({
  page,
  onNavigate,
  onProfileChanged,
}: {
  page: string;
  onNavigate: (p: string) => void;
  onProfileChanged: () => Promise<void>;
}) {
  const [profile, setProfile] = useState<Profile | null>(null),
    [contacts, setContacts] = useState<Contact[]>([]),
    [connections, setConnections] = useState<any>({}),
    [publisher, setPublisher] = useState<any>({}),
    [wallet, setWallet] = useState<any>(null),
    [config, setConfig] = useState<any>({}),
    [status, setStatus] = useState<any>({}),
    [account, setAccount] = useState<any>(null),
    [activity, setActivity] = useState<any>({}),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(""),
    [loaded, setLoaded] = useState(false),
    [sync, setSync] = useState<any>({}),
    [friendQuery, setFriendQuery] = useState(""),
    [invite, setInvite] = useState<any>(null),
    [code, setCode] = useState(""),
    [manual, setManual] = useState(""),
    [handle, setHandle] = useState(""),
    [peer, setPeer] = useState(""),
    [backup, setBackup] = useState<"export" | "restore" | null>(null),
    [passphrase, setPassphrase] = useState("");
  const load = useCallback(async () => {
    const [p, c, a, w, cfg, pub, network, mc, act] = await Promise.all([
      api<Profile | null>("orbit_profile"),
      api<Contact[]>("orbit_contacts"),
      api("orbit_connections"),
      api("orbit_wallet"),
      api("orbit_config"),
      api("orbit_publisher"),
      api("orbit_status"),
      api("mc_account"),
      api("orbit_activity"),
    ]);
    setProfile(p);
    setContacts(c);
    setConnections(a);
    setWallet(w);
    setConfig(cfg);
    setPublisher(pub);
    setStatus(network);
    setAccount(mc);
    setActivity(act);
    setSync(await api("orbit_profile_sync_status"));
    setLoaded(true);
  }, []);
  const run = async (
    key: string,
    task: () => Promise<any>,
    message?: string,
  ) => {
    setBusy(key);
    setError("");
    try {
      const result = await task();
      await load();
      if (message) setNotice(message);
      return result;
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy("");
    }
  };
  useEffect(() => {
    api("orbit_social_open").catch(() => {});
    load().catch((e) => {
      setError(errorText(e));
      setLoaded(true);
    });
    const un = listen("social-changed", () => load().catch(() => {}));
    return () => {
      un.then((fn) => fn());
    };
  }, [load]);
  useEffect(() => {
    if (notice) {
      const t = setTimeout(() => setNotice(""), 5000);
      return () => clearTimeout(t);
    }
  }, [notice]);
  const accepted = contacts.filter((c) => c.state === "accepted"),
    incoming = contacts.filter((c) => c.state === "incoming");
  const disconnect = (provider: string) =>
    run(
      provider,
      async () => {
        if (provider === "microsoft") await api("mc_signout");
        else await api("orbit_connection_disconnect", { provider });
      },
      "Disconnected on this device. External grants can also be revoked in the provider’s account settings.",
    );
  const connect = (id: string) => {
    if (id === "bluesky") {
      setManual("bluesky-oauth");
      setHandle("");
      return;
    }
    return run(
      id,
      () =>
        id === "microsoft"
          ? api("mc_signin")
          : api("orbit_connection_oauth", { provider: id }),
      "Account identity connected",
    );
  };
  const saveProfile = async (v: Profile): Promise<Profile> => {
    setBusy("profile");
    setError("");
    try {
      const saved = await api<Profile>("orbit_profile_save", v);
      setProfile(saved);
      await onProfileChanged();
      await load();
      return saved;
    } catch (e) {
      setError(errorText(e));
      throw e;
    } finally {
      setBusy("");
    }
  };
  return (
    <div className="social-view">
      {error && (
        <div className="alert error">
          <AlertCircle size={17} />
          <span>{error}</span>
          <button className="icon-btn" onClick={() => setError("")}>
            <X size={15} />
          </button>
        </div>
      )}
      {notice && (
        <div className="social-notice">
          <Check size={16} />
          {notice}
        </div>
      )}
      {!loaded ? (
        <div className="loading">Opening your space…</div>
      ) : !profile && page !== "Settings" ? (
        <>
          <Heading
            eyebrow="A SPACE THAT’S YOURS"
            title="Meet your Orbit."
            subtitle="A profile for your worlds, your people, and everything you share."
          />
          <div className="identity-intro">
            <div className="identity-orbit">
              ◒<i />
              <i />
            </div>
            <h2>
              Start with a name.
              <br />
              Keep control of the rest.
            </h2>
            <p>
              Your profile begins on this computer. A separate protected
              communication key identifies you to friends. A wallet is optional,
              and costs nothing to create.
            </p>
            <ProfileForm
              profile={{
                id: "",
                npub: "",
                name: "",
                bio: "",
                gameVisibility: false,
                musicVisibility: false,
              }}
              onSave={(v) => void saveProfile(v).catch(() => {})}
              busy={!!busy}
            />
            <button
              className="text-button"
              onClick={() => {
                setBackup("restore");
                setPassphrase("");
              }}
            >
              <Download size={14} /> Already have an Orbit backup?
            </button>
          </div>
        </>
      ) : (
        <>
          <div className="social-topline">
            <span
              className={`network-badge ${(status.phase || "").toLowerCase()}`}
            >
              {status.phase || "Connecting"}
            </span>
            <div className="social-tabs">
              {["Profile", "Connections", "Conversations"].map((p) => (
                <button
                  key={p}
                  className={page === p ? "selected" : ""}
                  onClick={() => onNavigate(p)}
                >
                  {p}
                </button>
              ))}
            </div>
          </div>
          {page === "Profile" && profile && (
            <>
              <ProfileCard
                profile={profile}
                connections={connections}
                count={accepted.length}
                sync={sync}
                onSave={saveProfile}
                onConnections={() => onNavigate("Connections")}
                onFriends={() => onNavigate("Conversations")}
                onRetry={() => run("sync", () => api("orbit_profile_retry"))}
              />
              <ProfileActivity
                activity={activity}
                profile={profile}
                busy={!!busy}
                onRun={run}
                onChanged={load}
                onSettings={() => onNavigate("Settings")}
              />
            </>
          )}
          {page === "Settings" && (
            <>
              <Heading
                eyebrow="PREFERENCES"
                title="Settings"
                subtitle="Wallet, recovery, privacy and connection preferences."
              />
              <div className="social-two-col">
                <section className="social-panel wallet-panel">
                  <div className="panel-heading">
                    <span className="panel-icon">
                      <Wallet size={19} />
                    </span>
                    <div>
                      <h2>Wallet</h2>
                      <p>Identity without a balance requirement.</p>
                    </div>
                  </div>
                  {wallet ? (
                    <>
                      <div className="wallet-address">
                        <ShieldCheck size={17} />
                        <code>{short(wallet.address)}</code>
                        <span>
                          {wallet.mode === "local"
                            ? "Protected on Windows"
                            : "External wallet"}
                        </span>
                      </div>
                      <p>
                        Ownership was verified on this installation. Friends
                        verify it independently using a challenge. A wallet
                        address by itself is not proof.
                      </p>
                      <div className="button-row">
                        <button
                          className="secondary"
                          onClick={() => {
                            setBackup("export");
                            setPassphrase("");
                          }}
                        >
                          <Download size={14} /> Encrypted backup
                        </button>
                        <button
                          className="text-button muted"
                          disabled={!!busy}
                          onClick={() =>
                            run(
                              "wallet",
                              () => api("orbit_wallet_disconnect"),
                              "Wallet disconnected. Its private key was removed from this device.",
                            )
                          }
                        >
                          <LogOut size={13} /> Disconnect wallet
                        </button>
                      </div>
                      <p className="small-warning">
                        Disconnecting a local wallet removes its key. Export an
                        encrypted backup first if you want to recover it.
                      </p>
                    </>
                  ) : (
                    <>
                      <p>
                        Create a new local wallet, or connect a wallet extension
                        in your browser. Signing a login message is off-chain.
                        No funds, transfers, approvals, or tokens are involved.
                      </p>
                      <div className="button-row">
                        <button
                          className="primary"
                          disabled={!!busy}
                          onClick={() =>
                            run(
                              "wallet",
                              () => api("orbit_wallet_create"),
                              "Wallet created. Export an encrypted backup before relying on it.",
                            )
                          }
                        >
                          <Plus size={14} /> Create local wallet
                        </button>
                        <button
                          className="secondary"
                          disabled={!!busy}
                          onClick={() =>
                            run(
                              "wallet",
                              () => api("orbit_wallet_connect"),
                              "Wallet ownership verified on this device",
                            )
                          }
                        >
                          <ArrowUpRight size={14} /> Connect existing
                        </button>
                      </div>
                    </>
                  )}
                </section>
                <section className="social-panel">
                  <div className="panel-heading">
                    <span className="panel-icon">
                      <LockKeyhole size={19} />
                    </span>
                    <div>
                      <h2>Backup and recovery</h2>
                      <p>Keep more than a wallet address.</p>
                    </div>
                  </div>
                  <p>
                    An encrypted Orbit backup contains your communication
                    identity, contacts, saved message history, and local wallet
                    key when present. OAuth accounts must reconnect. Media files
                    are not bundled.
                  </p>
                  <p>
                    Recovering a wallet alone does not recover your contacts or
                    conversations.
                  </p>
                  <button
                    className="secondary"
                    onClick={() => {
                      setBackup("export");
                      setPassphrase("");
                    }}
                  >
                    <Download size={14} /> Export encrypted backup
                  </button>
                  {!profile && (
                    <button
                      className="secondary"
                      onClick={() => {
                        setBackup("restore");
                        setPassphrase("");
                      }}
                    >
                      Restore Orbit backup
                    </button>
                  )}
                </section>
              </div>
              <section className="social-panel">
                <div className="panel-heading">
                  <span className="panel-icon">
                    <Eye size={19} />
                  </span>
                  <div>
                    <h2>Privacy</h2>
                    <p>Game and music activity have separate controls.</p>
                  </div>
                </div>
                <div className="privacy-row">
                  <div>
                    <strong>Game activity</strong>
                    <p>
                      Share supported game cards with accepted friends. No
                      unsupported “Join” actions.
                    </p>
                  </div>
                  <Toggle
                    label="Game activity visibility"
                    disabled={!profile || !!busy}
                    value={!!profile?.gameVisibility}
                    onChange={(v) =>
                      profile &&
                      void saveProfile({ ...profile, gameVisibility: v }).catch(
                        () => {},
                      )
                    }
                  />
                </div>
                <div className="privacy-row">
                  <div>
                    <strong>Music activity</strong>
                    <p>
                      Share Spotify playback or local Windows media-session
                      metadata. Activity expires.
                    </p>
                  </div>
                  <Toggle
                    label="Music activity visibility"
                    disabled={!profile || !!busy}
                    value={!!profile?.musicVisibility}
                    onChange={(v) =>
                      profile &&
                      void saveProfile({
                        ...profile,
                        musicVisibility: v,
                      }).catch(() => {})
                    }
                  />
                </div>
              </section>
              <NetworkSettings
                config={config}
                status={status}
                onSave={async (v) => {
                  await api("orbit_config_save", v);
                  await load();
                }}
                onReconnect={async () => {
                  await api("orbit_network_start");
                  await load();
                }}
              />
            </>
          )}
          {page === "Connections" && (
            <>
              <Heading
                eyebrow="CONNECTED, NOT LOCKED IN"
                title="Bring your worlds together."
                subtitle="Verified account connections and personal profile links, clearly distinguished."
              />
              <div className="connection-grid">
                {providers.map((p) => {
                  const c = p.id === "microsoft" ? account : connections[p.id];
                  const available =
                    p.id === "bluesky" ||
                    (p.id !== "instagram" &&
                      !!publisher[p.id] &&
                      (p.id !== "x" || publisher.xBillable));
                  return (
                    <section key={p.id} className="connection-card">
                      <div className="connection-card-top">
                        <span
                          className="provider-logo"
                          style={{ color: p.color }}
                        >
                          {p.letter}
                        </span>
                        <h2>{p.name}</h2>
                        {c && (
                          <span
                            className={`connection-status ${c.authenticated ? "verified" : ""}`}
                          >
                            {c.authenticated ? (
                              <ShieldCheck size={12} />
                            ) : (
                              <Link2 size={12} />
                            )}{" "}
                            {c.authenticated ? "Authenticated" : "Manual link"}
                          </span>
                        )}
                      </div>
                      <p>{p.note}</p>
                      {c ? (
                        <>
                          <div className="connected-person">
                            {c.avatarUrl && (
                              <ProviderImage
                                url={c.avatarUrl}
                                alt="Account avatar"
                              />
                            )}
                            <strong>{c.name}</strong>
                            <span>{c.id ? short(c.id) : c.url}</span>
                          </div>
                          <div className="connection-card-actions">
                            {c.url && (
                              <button
                                className="text-button"
                                onClick={() =>
                                  run(p.id, () =>
                                    api("orbit_connection_open", {
                                      provider: p.id,
                                    }),
                                  )
                                }
                              >
                                View profile <ArrowUpRight size={12} />
                              </button>
                            )}
                            <button
                              className="text-button muted"
                              disabled={!!busy}
                              onClick={() => disconnect(p.id)}
                            >
                              Disconnect
                            </button>
                          </div>
                          {p.id !== "microsoft" && (
                            <label className="checkbox">
                              <input
                                type="checkbox"
                                checked={!!c.visible}
                                onChange={(e) =>
                                  run(p.id, () =>
                                    api("orbit_connection_visibility", {
                                      provider: p.id,
                                      visible: e.target.checked,
                                    }),
                                  )
                                }
                              />{" "}
                              Show in shared profile cards
                            </label>
                          )}
                          {p.id === "bluesky" && (
                            <button
                              className="secondary"
                              onClick={() => setManual("bluesky-posts")}
                            >
                              Read public posts <ChevronRight size={13} />
                            </button>
                          )}
                        </>
                      ) : (
                        <div className="connection-card-actions">
                          {available ? (
                            <button
                              className="secondary"
                              disabled={!!busy}
                              onClick={() => connect(p.id)}
                            >
                              {busy === p.id
                                ? "Finish in browser…"
                                : "Connect account"}
                              <ArrowUpRight size={13} />
                            </button>
                          ) : (
                            <span className="integration-limitation">
                              {p.id === "instagram"
                                ? "Personal-account OAuth unavailable"
                                : p.id === "x"
                                  ? "Publisher has not enabled paid API access"
                                  : "Not enabled in this Orbit release"}
                            </span>
                          )}
                          {p.id !== "microsoft" && (
                            <button
                              className="text-button"
                              onClick={() => {
                                setManual(p.id);
                                setHandle("");
                              }}
                            >
                              <Plus size={12} /> Add profile link
                            </button>
                          )}
                        </div>
                      )}
                    </section>
                  );
                })}
              </div>
              <div className="note">
                <Shield size={16} /> Manual links are not proof of account
                ownership. Profile links, Ethereum addresses, and AT Protocol
                DIDs are separate identities. Visibility controls never publish
                private conversations or contacts to a public repository.
              </div>
            </>
          )}
          {page === "Conversations" && (
            <>
              <Heading
                eyebrow="A SMALLER, BETTER CIRCLE"
                title="Conversations"
                subtitle="Friendship is accepted by both people. It isn’t a public follow."
                action={
                  <button
                    className="primary"
                    disabled={!!busy}
                    onClick={() =>
                      run("invite", async () =>
                        setInvite(await api("orbit_invitation")),
                      )
                    }
                  >
                    <Plus size={15} /> Invite a friend
                  </button>
                }
              />
              <div className="invite-input">
                <Link2 size={18} />
                <input
                  placeholder="Paste a signed Orbit invitation…"
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                />
                <button
                  className="secondary"
                  disabled={!!busy || !code}
                  onClick={() =>
                    run(
                      "accept-code",
                      async () => {
                        await api("orbit_invite_accept", { code });
                        setCode("");
                      },
                      "Friend request queued. They must accept before you can chat.",
                    )
                  }
                >
                  Send request <ArrowUpRight size={13} />
                </button>
              </div>
              {incoming.length > 0 && (
                <section className="social-panel">
                  <h2>
                    Invitations for you{" "}
                    <span className="count-badge">{incoming.length}</span>
                  </h2>
                  {incoming.map((c) => (
                    <div className="friend-row" key={c.id}>
                      <Avatar person={c} />
                      <div>
                        <strong>{c.name}</strong>
                        <p>{c.bio || short(c.id)}</p>
                      </div>
                      <button
                        className="primary"
                        disabled={!!busy}
                        onClick={() =>
                          run(
                            c.id,
                            () => api("orbit_friend_accept", { id: c.id }),
                            "Friendship accepted",
                          )
                        }
                      >
                        <Check size={14} /> Accept
                      </button>
                      <button
                        className="icon-btn"
                        title="Block invitation"
                        onClick={() =>
                          run(c.id, () =>
                            api("orbit_block", { id: c.id, blocked: true }),
                          )
                        }
                      >
                        <X size={15} />
                      </button>
                    </div>
                  ))}
                </section>
              )}
              <details className="friend-directory">
                <summary>Friend list · {accepted.length} accepted</summary>
                <input
                  aria-label="Search friends"
                  placeholder="Search friends…"
                  value={friendQuery}
                  onChange={(e) => setFriendQuery(e.target.value)}
                />
                <VirtualFriends
                  contacts={accepted.filter((c) =>
                    c.name.toLowerCase().includes(friendQuery.toLowerCase()),
                  )}
                  onChat={(c) => {
                    setPeer(c.id);
                    onNavigate("Conversations");
                  }}
                  onBlock={(c) =>
                    run(c.id, () =>
                      api("orbit_block", {
                        id: c.id,
                        blocked: c.state !== "blocked",
                      }),
                    )
                  }
                />
              </details>
              {contacts.some((c) => c.state === "outgoing") && (
                <section className="social-panel">
                  <h3>Pending outgoing requests</h3>
                  {contacts
                    .filter((c) => c.state === "outgoing")
                    .map((c) => (
                      <div className="friend-row" key={c.id}>
                        <Avatar person={c} />
                        <span>{c.name} · Waiting for acceptance</span>
                        <button
                          className="text-button"
                          onClick={() =>
                            run(c.id, () =>
                              api("orbit_block", { id: c.id, blocked: true }),
                            )
                          }
                        >
                          Block
                        </button>
                      </div>
                    ))}
                </section>
              )}
              {contacts.some(
                (c) => c.state === "blocked" || c.state === "removed",
              ) && (
                <details>
                  <summary>Blocked and removed contacts</summary>
                  <VirtualFriends
                    contacts={contacts.filter(
                      (c) => c.state === "blocked" || c.state === "removed",
                    )}
                    onChat={() => {}}
                    onBlock={(c) =>
                      run(c.id, () =>
                        api("orbit_block", {
                          id: c.id,
                          blocked: c.state !== "blocked",
                        }),
                      )
                    }
                  />
                </details>
              )}
            </>
          )}
          {page === "Conversations" && (
            <>
              <Conversations
                activity={activity}
                contacts={accepted}
                peer={peer}
                onPeer={setPeer}
                config={config}
                onError={setError}
                onNotice={setNotice}
              />
            </>
          )}
        </>
      )}
      {invite && (
        <div className="modal-backdrop">
          <div className="modal invite-modal">
            <div className="modal-heading">
              <h2>A place in your Orbit.</h2>
              <button className="icon-btn" onClick={() => setInvite(null)}>
                <X />
              </button>
            </div>
            <div className="qr-wrap">
              {invite.code.length <= 2300 ? (
                <QRCodeSVG value={invite.code} size={230} level="L" />
              ) : (
                <p>
                  This invitation is too long for a reliable QR. Copy its code
                  below.
                </p>
              )}
            </div>
            <p>
              This signed, one-use invitation expires{" "}
              {new Date(invite.expires * 1000).toLocaleDateString()}. Share it
              only with the person you want to invite. They can verify your
              identity before accepting.
            </p>
            <textarea readOnly value={invite.code} rows={3} />
            <button
              className="primary"
              onClick={() => {
                navigator.clipboard.writeText(invite.code);
                setNotice("Signed invitation copied");
              }}
            >
              <Copy size={14} /> Copy invitation
            </button>
            <div className="note">
              The invitation discloses your display name, bio, visible account
              links, public communication identity and inbox relays. Share it
              privately.
            </div>
          </div>
        </div>
      )}
      {manual && (
        <div className="modal-backdrop">
          <div className="modal">
            <div className="modal-heading">
              <h2>
                {manual === "bluesky-oauth"
                  ? "Connect your Bluesky account"
                  : manual === "bluesky-posts"
                    ? "Public Bluesky posts"
                    : "Add a profile link"}
              </h2>
              <button className="icon-btn" onClick={() => setManual("")}>
                <X />
              </button>
            </div>
            {manual === "bluesky-posts" ? (
              <PublicPosts />
            ) : (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  run(
                    manual,
                    async () => {
                      if (manual === "bluesky-oauth")
                        await api("orbit_bluesky_connect", { handle });
                      else
                        await api("orbit_connection_manual", {
                          provider: manual,
                          url: handle,
                          visible: false,
                        });
                      setManual("");
                    },
                    manual === "bluesky-oauth"
                      ? "Bluesky identity authenticated"
                      : "Manual link saved; ownership is not verified",
                  );
                }}
              >
                <p className="muted">
                  {manual === "bluesky-oauth"
                    ? "Orbit uses the official OAuth flow in your system browser. No app password is requested. This preview uses the local-client exception if publisher metadata is not configured."
                    : "This is a user-entered link, not an authenticated or verified connection."}
                </p>
                <label>
                  {manual === "bluesky-oauth"
                    ? "Bluesky handle"
                    : "HTTPS profile URL"}
                  <input
                    required
                    type={manual === "bluesky-oauth" ? "text" : "url"}
                    value={handle}
                    onChange={(e) => setHandle(e.target.value)}
                    placeholder={
                      manual === "bluesky-oauth"
                        ? "you.bsky.social"
                        : `https://${manual === "instagram" ? "instagram.com/you" : manual === "x" ? "x.com/you" : "…"}`
                    }
                  />
                </label>
                <button className="primary" disabled={!!busy}>
                  {busy
                    ? "Waiting for browser…"
                    : manual === "bluesky-oauth"
                      ? "Continue in browser"
                      : "Save manual link"}
                  <ArrowUpRight size={14} />
                </button>
              </form>
            )}
          </div>
        </div>
      )}
      {backup && (
        <div className="modal-backdrop">
          <div className="modal">
            <div className="modal-heading">
              <h2>
                {backup === "export"
                  ? "Take your Orbit with you."
                  : "Restore your Orbit."}
              </h2>
              <button
                className="icon-btn"
                onClick={() => {
                  setBackup(null);
                  setPassphrase("");
                }}
              >
                <X />
              </button>
            </div>
            <p className="muted">
              The passphrase protects your communication key, contacts, local
              history and local wallet key. Losing both this Windows
              installation and your backup/passphrase loses access. Attachments
              require separately retained files or available provider copies.
              External accounts must reconnect.
            </p>
            <label>
              Backup passphrase{" "}
              {backup === "export" ? "(at least 16 characters)" : ""}
              <input
                type="password"
                autoComplete="new-password"
                value={passphrase}
                minLength={backup === "export" ? 16 : 1}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </label>
            <button
              className="primary"
              disabled={
                !!busy || passphrase.length < (backup === "export" ? 16 : 1)
              }
              onClick={() =>
                run(
                  "backup",
                  async () => {
                    const path =
                      backup === "export"
                        ? await save({
                            defaultPath: `Orbit-backup-${new Date().toISOString().slice(0, 10)}.age`,
                            filters: [
                              {
                                name: "Encrypted Orbit backup",
                                extensions: ["age"],
                              },
                            ],
                          })
                        : await open({
                            filters: [
                              {
                                name: "Encrypted Orbit backup",
                                extensions: ["age"],
                              },
                            ],
                          });
                    if (typeof path !== "string") return;
                    await api(
                      backup === "export"
                        ? "orbit_backup_export"
                        : "orbit_backup_restore",
                      { path, password: passphrase },
                    );
                    setPassphrase("");
                    setBackup(null);
                    await onProfileChanged();
                  },
                  backup === "export"
                    ? "Encrypted backup saved. Keep the passphrase separately."
                    : "Identity, contacts and saved history restored. Network connections remain off.",
                )
              }
            >
              <Download size={14} />
              {busy
                ? "Protecting your data…"
                : backup === "export"
                  ? "Export encrypted backup"
                  : "Choose backup and restore"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
function Toggle({
  value,
  label,
  disabled,
  onChange,
}: {
  value: boolean;
  label?: string;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      className={`toggle ${value ? "on" : ""}`}
      role="switch"
      aria-label={label}
      disabled={disabled}
      aria-checked={value}
      onClick={() => onChange(!value)}
    >
      <span />
    </button>
  );
}
function ProfileForm({
  profile,
  onSave,
  busy,
}: {
  profile: Profile;
  onSave: (v: Profile) => void;
  busy: boolean;
}) {
  const [v, setV] = useState(profile);
  useEffect(() => setV(profile), [profile]);
  return (
    <form
      className="profile-form"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(v);
      }}
    >
      <label>
        Display name
        <input
          required
          maxLength={60}
          value={v.name}
          onChange={(e) => setV({ ...v, name: e.target.value })}
          placeholder="How should friends know you?"
        />
      </label>
      <label>
        A little about you
        <textarea
          maxLength={2000}
          value={v.bio}
          onChange={(e) => setV({ ...v, bio: e.target.value })}
          rows={3}
          placeholder="Your favorite worlds, a small hello, or something entirely you."
        />
      </label>
      <button className="primary" disabled={busy}>
        <Check size={14} />
        {busy ? "Saving…" : profile.id ? "Save profile" : "Create my Orbit"}
      </button>
      <span className="form-footnote">
        <Shield size={12} /> Private until you choose to share. No wallet
        required.
      </span>
    </form>
  );
}
function VirtualFriends({
  contacts,
  onChat,
  onBlock,
}: {
  contacts: Contact[];
  onChat: (c: Contact) => void;
  onBlock: (c: Contact) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: contacts.length,
    getScrollElement: () => ref.current,
    estimateSize: () => 86,
    overscan: 5,
  });
  if (!contacts.length)
    return (
      <Empty icon={Users} title="Every good circle starts somewhere">
        Create an invitation, or paste one from a friend. Both sides must accept
        before messaging.
      </Empty>
    );
  return (
    <div className="friend-list" ref={ref}>
      <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
        {virtual.getVirtualItems().map((row) => {
          const c = contacts[row.index];
          return (
            <div
              key={c.id}
              className="friend-row virtual-row"
              style={{ transform: `translateY(${row.start}px)` }}
            >
              <Avatar person={c} />
              <div>
                <strong>{c.name}</strong>
                <p>
                  {c.state === "accepted"
                    ? c.wallet
                      ? c.wallet.expires * 1000 > Date.now()
                        ? "Wallet proof verified by this device"
                        : "Wallet proof expired"
                      : "Friend · private conversation"
                    : c.state === "incoming"
                      ? "Invitation received"
                      : c.state === "outgoing"
                        ? "Waiting for acceptance"
                        : c.state === "blocked"
                          ? "Blocked"
                          : "Removed"}
                </p>
              </div>
              {c.state === "accepted" && (
                <button className="secondary" onClick={() => onChat(c)}>
                  <MessageSquare size={14} /> Message
                </button>
              )}
              <button className="text-button muted" onClick={() => onBlock(c)}>
                {c.state === "blocked" ? "Unblock" : "Block"}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
function Conversations({
  activity,
  contacts,
  peer,
  onPeer,
  config,
  onError,
  onNotice,
}: {
  activity: any;
  contacts: Contact[];
  peer: string;
  onPeer: (p: string) => void;
  config: any;
  onError: (s: string) => void;
  onNotice: (s: string) => void;
}) {
  const [messages, setMessages] = useState<Message[]>([]),
    [text, setText] = useState(""),
    [attachments, setAttachments] = useState<Attachment[]>([]),
    [busy, setBusy] = useState(false),
    [compress, setCompress] = useState(false),
    [proofOpen, setProofOpen] = useState(false),
    [proofAddress, setProofAddress] = useState(""),
    [progress, setProgress] = useState<Record<string, any>>({});
  const [searchFriends, setSearchFriends] = useState("");
  const listRef = useRef<HTMLDivElement>(null),
    [gallery, setGallery] = useState<{ url: string; name: string } | null>(
      null,
    );
  const selected = contacts.find((c) => c.id === peer);
  const filtered = messages.filter(
    (m) =>
      m.kind === "message" ||
      m.kind === "wallet_challenge" ||
      m.kind === "wallet_proof",
  );
  const load = useCallback(async () => {
    if (peer) setMessages(await api("orbit_messages", { peer }));
  }, [peer]);
  const virtual = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => 130,
    overscan: 4,
    measureElement: (el) => el.getBoundingClientRect().height,
  });
  useEffect(() => {
    if (!peer && contacts[0]) onPeer(contacts[0].id);
  }, [contacts, peer]);
  useEffect(() => {
    setMessages([]);
    load().catch((e) => onError(errorText(e)));
    const un = listen("social-changed", () => load().catch(() => {}));
    return () => {
      un.then((f) => f());
    };
  }, [load]);
  useEffect(() => {
    const un = listen<any>("media-progress", (e) =>
      setProgress((p) => ({ ...p, [e.payload.id]: e.payload })),
    );
    return () => {
      un.then((f) => f());
    };
  }, []);
  const add = async (paths: string[]) => {
    if (!peer) {
      onError("Choose an accepted friend before attaching files.");
      return;
    }
    if (paths.length + attachments.length > 10) {
      onError("Attach at most ten files per message.");
      return;
    }
    setBusy(true);
    try {
      for (const path of paths) {
        const result = await api<Attachment>("orbit_media_prepare", {
          path,
          compress,
        });
        setAttachments((a) => [...a, result]);
      }
    } catch (e) {
      onError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const addRef = useRef(add);
  addRef.current = add;
  useEffect(() => {
    const handler = (e: Event) =>
      addRef.current((e as CustomEvent<string[]>).detail);
    window.addEventListener("orbit-files", handler);
    return () => window.removeEventListener("orbit-files", handler);
  }, []);
  const send = async () => {
    setBusy(true);
    try {
      for (const a of attachments) {
        await api("orbit_media_upload", { id: a.id });
      }
      await api("orbit_send", {
        peer,
        text,
        attachments: attachments.map((a) => a.id),
      });
      setText("");
      setAttachments([]);
      await load();
      setTimeout(
        () =>
          virtual.scrollToIndex(Math.max(0, filtered.length), { align: "end" }),
        80,
      );
    } catch (e) {
      onError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="conversation-layout">
      <aside className="conversation-list">
        <div className="conversation-list-label">
          FRIEND LIST <span>{contacts.length}</span>
        </div>
        <input
          className="friend-search"
          aria-label="Search conversation friends"
          value={searchFriends}
          onChange={(e) => setSearchFriends(e.target.value)}
          placeholder="Search friends…"
        />
        {contacts
          .filter((c) =>
            c.name.toLowerCase().includes(searchFriends.toLowerCase()),
          )
          .map((c) => (
            <button
              key={c.id}
              className={c.id === peer ? "selected" : ""}
              onClick={() => onPeer(c.id)}
            >
              <Avatar person={c} />
              <span>
                <strong>{c.name}</strong>
                <small>Accepted friend</small>
                <SharedActivity activity={activity} peer={c.id} />
              </span>
              <ChevronRight size={13} />
            </button>
          ))}
        {!contacts.length && <p>Accepted friendships appear here.</p>}
      </aside>
      <section className="conversation">
        <div className="conversation-heading">
          {selected ? (
            <>
              <Avatar person={selected} />
              <div>
                <strong>{selected.name}</strong>
                <p>
                  <LockKeyhole size={10} /> Encrypted conversation ·{" "}
                  {short(selected.id)}
                </p>
              </div>
              <button
                className="text-button"
                onClick={() => setProofOpen(!proofOpen)}
              >
                View profile
              </button>
            </>
          ) : (
            <>
              <MessageSquare size={22} />
              <strong>Your conversations</strong>
            </>
          )}
        </div>
        {selected && proofOpen && (
          <div className="contact-details">
            {selected.banner && (
              <div
                className="friend-profile-banner"
                style={{ backgroundImage: `url(${selected.banner})` }}
              />
            )}
            <div className="friend-profile-heading">
              <Avatar person={selected} size="large" />
              <h2>{selected.name}</h2>
            </div>
            <small>
              Saved friend profile · available copies do not indicate online
              presence.
            </small>
            {selected.imagesPending && (
              <div className="button-row">
                <small>Some profile images are waiting to download.</small>
                <button
                  className="text-button"
                  onClick={() =>
                    api("orbit_contact_profile_retry", {
                      peer: selected.id,
                    }).catch((e) => onError(errorText(e)))
                  }
                >
                  Retry images
                </button>
              </div>
            )}

            <SharedActivity activity={activity} peer={selected.id} />

            <p>{selected.bio || "No bio shared."}</p>
            <div className="button-row">
              {Object.entries(selected.connections || {}).map(
                ([provider, c]) => (
                  <button
                    key={provider}
                    className="text-button"
                    title={`${c.url || ""} · Shared claim, not independently verified ownership`}
                    onClick={() =>
                      api("orbit_contact_link", { peer, provider }).catch((e) =>
                        onError(errorText(e)),
                      )
                    }
                  >
                    {provider} · {c.name || "Profile"}{" "}
                    <ArrowUpRight size={12} />
                  </button>
                ),
              )}
            </div>
            <small>
              Shared account links are claims from this friend. They are not
              independently authenticated by your device.
            </small>
            <form
              className="button-row"
              onSubmit={async (e) => {
                e.preventDefault();
                try {
                  await api("orbit_wallet_request", {
                    peer,
                    address: proofAddress,
                  });
                  onNotice(
                    "A three-minute ownership challenge was queued. Your friend must review and sign it.",
                  );
                  await load();
                } catch (e) {
                  onError(errorText(e));
                }
              }}
            >
              <input
                aria-label="Claimed wallet address"
                placeholder="Friend's claimed wallet address (0x…)"
                value={proofAddress}
                onChange={(e) => setProofAddress(e.target.value)}
                required
                pattern="0x[0-9a-fA-F]{40}"
              />
              <button className="secondary" type="submit">
                Request wallet proof
              </button>
            </form>
            <small>
              The wallet address alone proves nothing. No transaction is
              requested.
            </small>
          </div>
        )}
        {selected ? (
          <>
            <div className="conversation-safety">
              <Shield size={12} /> Queued stays local. Sent means relay
              acknowledgment. Delivered means the recipient’s Orbit acknowledged
              it.
            </div>
            <div ref={listRef} className="message-list">
              {messages.length >= 60 && (
                <button
                  className="text-button"
                  onClick={async () => {
                    try {
                      const older = await api<Message[]>("orbit_messages", {
                        peer,
                        before: messages[0]?.created,
                        beforeId: messages[0]?.id,
                      });
                      setMessages((m) => [...older, ...m]);
                    } catch (e) {
                      onError(errorText(e));
                    }
                  }}
                >
                  Load earlier messages
                </button>
              )}
              <div
                style={{ height: virtual.getTotalSize(), position: "relative" }}
              >
                {virtual.getVirtualItems().map((row) => {
                  const m = filtered[row.index];
                  return (
                    <div
                      data-index={row.index}
                      ref={virtual.measureElement}
                      key={m.id}
                      className={`message-row ${m.direction === "out" ? "outgoing" : ""}`}
                      style={{ transform: `translateY(${row.start}px)` }}
                    >
                      <div className="message-bubble">
                        {m.kind === "wallet_challenge" ? (
                          <>
                            <strong>
                              <Wallet size={13} /> Wallet verification request
                            </strong>
                            <pre className="siwe-review">{m.body.message}</pre>
                            {m.direction === "in" && (
                              <button
                                className="secondary"
                                onClick={async () => {
                                  try {
                                    await api("orbit_wallet_approve", {
                                      id: m.id,
                                    });
                                    onNotice(
                                      "Proof queued for this verifying friend. External wallets cannot approve peer challenges in this preview.",
                                    );
                                  } catch (e) {
                                    onError(errorText(e));
                                  }
                                }}
                              >
                                Reviewed · Sign with my local wallet
                              </button>
                            )}
                          </>
                        ) : m.kind === "wallet_proof" ? (
                          <p>
                            <ShieldCheck size={14} /> Wallet ownership proof
                          </p>
                        ) : (
                          <>
                            {m.body.text && <p>{m.body.text}</p>}
                            {!!m.body.attachments?.length && (
                              <div className="attachment-gallery">
                                {m.body.attachments.map((a) => (
                                  <AttachmentCard
                                    key={a.id}
                                    file={a}
                                    message={m.id}
                                    progress={progress[a.id]}
                                    onError={onError}
                                    onGallery={setGallery}
                                  />
                                ))}
                              </div>
                            )}
                          </>
                        )}
                        <div className="message-meta">
                          <time>
                            {new Date(m.created * 1000).toLocaleTimeString([], {
                              hour: "2-digit",
                              minute: "2-digit",
                            })}
                          </time>
                          {m.direction === "out" && (
                            <span title={m.error || ""}>
                              {m.state === "delivered" ? (
                                <CheckCheck size={12} />
                              ) : m.state === "sent" ? (
                                <Check size={12} />
                              ) : m.state === "failed" ? (
                                <AlertCircle size={12} />
                              ) : (
                                <span className="queued-dot" />
                              )}
                              {m.state}
                            </span>
                          )}
                          {m.state === "failed" && (
                            <button
                              onClick={() =>
                                api("orbit_retry", { id: m.id })
                                  .then(load)
                                  .catch((e) => onError(errorText(e)))
                              }
                            >
                              Retry
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
              {!filtered.length && (
                <div className="conversation-start">
                  <span>
                    <LockKeyhole size={23} />
                  </span>
                  <h3>A conversation between you two.</h3>
                  <p>
                    Say hello, send a memory, or share something worth keeping.
                  </p>
                </div>
              )}
            </div>
            <div className="composer">
              {attachments.length > 0 && (
                <div className="staged-files">
                  {attachments.map((a) => (
                    <span key={a.id}>
                      <File size={12} />
                      {a.name}
                      <small>{bytes(a.size)}</small>
                      <button
                        title="Remove staged attachment"
                        disabled={busy}
                        onClick={() =>
                          setAttachments((xs) =>
                            xs.filter((x) => x.id !== a.id),
                          )
                        }
                      >
                        <X size={12} />
                      </button>
                    </span>
                  ))}
                </div>
              )}
              {Object.values(progress)
                .filter(
                  (j) =>
                    ![
                      "staged",
                      "ready",
                      "uploaded",
                      "failed",
                      "canceled",
                    ].includes(j.state),
                )
                .map((j) => (
                  <div key={j.id} className="inline-transfer">
                    <span>
                      {j.state} · {bytes(j.done)} / {bytes(j.total)}
                    </span>
                    <progress max={j.total || 1} value={j.done} />
                    <button
                      onClick={() => api("orbit_media_cancel", { id: j.id })}
                    >
                      Cancel
                    </button>
                  </div>
                ))}
              <div className="composer-input">
                <textarea
                  aria-label="Message"
                  placeholder={`Message ${selected.name}…`}
                  value={text}
                  maxLength={8000}
                  onChange={(e) => setText(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      if (!busy && (text.trim() || attachments.length)) send();
                    }
                  }}
                  rows={2}
                />
                <div className="composer-tools">
                  <button
                    className="icon-btn"
                    title="Attach files"
                    disabled={busy}
                    onClick={async () => {
                      const paths = await open({ multiple: true });
                      if (paths)
                        await add(Array.isArray(paths) ? paths : [paths]);
                    }}
                  >
                    <Paperclip size={19} />
                  </button>
                  <label className="checkbox">
                    <input
                      type="checkbox"
                      checked={compress}
                      onChange={(e) => setCompress(e.target.checked)}
                    />{" "}
                    Compress photos
                  </label>
                  <button
                    className="primary"
                    disabled={busy || (!text.trim() && !attachments.length)}
                    onClick={send}
                  >
                    {busy ? "Preparing…" : "Send"}
                    <Send size={14} />
                  </button>
                </div>
              </div>
              <p className="composer-note">
                {attachments.length
                  ? `Encrypted files upload to ${config.mediaServer || "a provider you configure"} when you send. Provider limits and retention apply.`
                  : "Drop photos, GIFs, videos or files here. Originals stay unchanged unless you choose compression."}
              </p>
            </div>
          </>
        ) : (
          <Empty icon={MessageSquare} title="Make room for a good hello">
            Accept a friend invitation to start an encrypted conversation.
          </Empty>
        )}
      </section>
      {gallery && (
        <div
          className="modal-backdrop gallery-view"
          onClick={() => setGallery(null)}
        >
          <button
            className="icon-btn gallery-close"
            onClick={() => setGallery(null)}
          >
            <X />
          </button>
          <img src={gallery.url} alt={gallery.name} />
          <p>{gallery.name}</p>
        </div>
      )}
    </div>
  );
}
function AttachmentCard({
  file,
  message,
  progress,
  onError,
  onGallery,
}: {
  file: Attachment;
  message: string;
  progress: any;
  onError: (s: string) => void;
  onGallery: (v: { url: string; name: string }) => void;
}) {
  const [path, setPath] = useState(""),
    [busy, setBusy] = useState(false);
  const image = file.type.startsWith("image/"),
    video = ["video/mp4", "video/webm"].includes(file.type);
  const fetch = async () => {
    setBusy(true);
    try {
      const result = await api("orbit_media_retrieve", {
        id: file.id,
        message,
      });
      setPath(convertFileSrc(result.path));
    } catch (e) {
      onError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      className={`attachment-card ${path && (image || video) ? "preview" : ""}`}
    >
      {path && image ? (
        <button
          className="image-preview"
          onClick={() => onGallery({ url: path, name: file.name })}
        >
          <img src={path} alt={file.name} loading="lazy" />
        </button>
      ) : path && video ? (
        <video
          src={path}
          controls
          preload="metadata"
          onError={() =>
            onError(
              "This video codec is not supported by the installed WebView2 runtime. Download the original to play it in another app.",
            )
          }
        />
      ) : (
        <div className="attachment-placeholder">
          {image ? (
            <Image size={24} />
          ) : video ? (
            <Film size={24} />
          ) : (
            <File size={24} />
          )}
          <button className="secondary" disabled={busy} onClick={fetch}>
            <Download size={13} />
            {busy
              ? "Downloading…"
              : image || video
                ? "Download & preview"
                : "Download encrypted file"}
          </button>
        </div>
      )}
      <div className="attachment-caption">
        <span>
          <strong>{file.name}</strong>
          <small>
            {bytes(file.size)} · {file.type}
          </small>
        </span>
        {path && (
          <button
            className="icon-btn"
            title="Save original file"
            onClick={async () => {
              try {
                const destination = await save({ defaultPath: file.name });
                if (destination)
                  await api("orbit_media_save", {
                    id: file.id,
                    path: destination,
                  });
              } catch (e) {
                onError(errorText(e));
              }
            }}
          >
            <Download size={15} />
          </button>
        )}
      </div>
      {busy && progress && (
        <div className="attachment-progress">
          <progress value={progress.done} max={progress.total || 1} />
          <button onClick={() => api("orbit_media_cancel", { id: file.id })}>
            Cancel
          </button>
        </div>
      )}
      <small className="attachment-source">
        {path
          ? "Verified local copy"
          : `Availability depends on ${file.url ? providerHost(file.url) : "the sender’s provider"}`}
      </small>
    </div>
  );
}
function PublicPosts() {
  const [posts, setPosts] = useState<any[]>([]),
    [cursor, setCursor] = useState(""),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(false);
  const load = async (append = false) => {
    setLoading(true);
    try {
      const r = await api("orbit_bluesky_posts", {
        cursor: append ? cursor : undefined,
      });
      setPosts((p) => (append ? [...p, ...r.feed] : r.feed));
      setCursor(r.cursor || "");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    load();
  }, []);
  return (
    <div className="public-posts">
      <div className="note">
        Public AT Protocol posts. Your contacts, messages and private activity
        are never written to this repository.
      </div>
      {error && <div className="alert error">{error}</div>}
      {posts.map((entry, i) => (
        <article key={entry.post.uri || i}>
          <strong>
            {entry.post.author.displayName || entry.post.author.handle}
          </strong>
          <p>{entry.post.record.text}</p>
          <time>
            {new Date(entry.post.record.createdAt).toLocaleDateString()}
          </time>
        </article>
      ))}
      {loading && <p className="muted">Reading public posts…</p>}
      {cursor && (
        <button
          className="secondary"
          disabled={loading}
          onClick={() => load(true)}
        >
          Load more
        </button>
      )}
    </div>
  );
}

function providerHost(raw: string) {
  try {
    return new URL(raw).hostname;
  } catch {
    return "the sender’s provider";
  }
}
