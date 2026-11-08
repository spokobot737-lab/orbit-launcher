import React, { lazy, Suspense, useState, useEffect, useRef } from "react";
import { createRoot } from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Home,
  Boxes,
  Grid2X2,
  Monitor,
  Folder,
  Download,
  Settings,
  Search,
  Plus,
  ArrowUpRight,
  Play,
  Star,
  MoreHorizontal,
  X,
  ChevronRight,
  RefreshCw,
  Check,
  AlertCircle,
  Terminal,
  SlidersHorizontal,
  Gamepad2,
  Clock,
  CheckCircle2,
  Layers,
  Minus,
  Pencil,
  Trash2,
  UserRound,
  Users,
  MessageSquare,
  Radio,
} from "lucide-react";
import { api, native, Item, Collection, errorText, imageUrl } from "./api";
import "./style.css";
import { Cover, Empty, Heading } from "./components";
const SocialView = lazy(() => import("./Social"));
const RobloxView = lazy(() => import("./Roblox"));
const ExperienceDetail = lazy(() =>
  import("./Roblox").then((m) => ({ default: m.ExperienceDetail })),
);

type Page =
  | "Home"
  | "Minecraft"
  | "Roblox"
  | "Programs"
  | "Collections"
  | "Downloads"
  | "Settings"
  | "Profile"
  | "Connections"
  | "Friends"
  | "Conversations"
  | "Activity";
type Progress = {
  stage: string;
  done: number;
  total: number;
  error?: string;
  at?: string;
};
const nav = [
  { name: "Home", icon: Home },
  { name: "Minecraft", icon: Boxes },
  { name: "Roblox", icon: Grid2X2 },
  { name: "Programs", icon: Monitor },
  { name: "Collections", icon: Folder },
  { name: "Downloads", icon: Download },
  { name: "Settings", icon: Settings },
  { name: "Profile", icon: UserRound },
  { name: "Friends", icon: Users },
  { name: "Conversations", icon: MessageSquare },
  { name: "Activity", icon: Radio },
] as const;
function App() {
  const [page, setPage] = useState<Page>("Home"),
    [items, setItems] = useState<Item[]>([]),
    [collections, setCollections] = useState<Collection[]>([]),
    [settings, setSettings] = useState<Record<string, any>>({}),
    [account, setAccount] = useState<any>(null),
    [publisher, setPublisher] = useState<any>({}),
    [orbitProfile, setOrbitProfile] = useState<any>(null),
    [query, setQuery] = useState(""),
    [notice, setNotice] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState<string>(""),
    [progress, setProgress] = useState<Progress | null>(null),
    [jobs, setJobs] = useState<Progress[]>([]),
    [editor, setEditor] = useState<Item | null>(null),
    [detail, setDetail] = useState<any>(null),
    [logs, setLogs] = useState<{ name: string; text: string } | null>(null),
    [context, setContext] = useState<{
      item: Item;
      x: number;
      y: number;
    } | null>(null),
    [running, setRunning] = useState<Record<string, boolean>>({}),
    [selectedCollection, setSelectedCollection] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const pageRef = useRef<Page>(page);
  useEffect(() => {
    pageRef.current = page;
  }, [page]);
  const load = async () => {
    const [i, c, s, a] = await Promise.all([
      api<Item[]>("library"),
      api<Collection[]>("collections"),
      api("settings"),
      api("mc_account"),
    ]);
    setItems(i);
    setCollections(c);
    setSettings(s);
    setAccount(a);
    setPublisher(await api("orbit_publisher"));
    setOrbitProfile(await api("orbit_profile"));
  };
  const perform = async (
    key: string,
    action: () => Promise<any>,
    message?: string,
  ) => {
    setError("");
    setBusy(key);
    try {
      await action();
      if (message) setNotice(message);
      await load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy("");
    }
  };
  useEffect(() => {
    if (native) load().catch((e) => setError(errorText(e)));
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        searchRef.current?.focus();
      }
      if (e.key === "Escape") {
        setEditor(null);
        setDetail(null);
        setContext(null);
        setLogs(null);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  useEffect(() => {
    if (!native) return;
    const ps = [
      listen<Progress>("progress", (e) => setProgress(e.payload)),
      listen<any>("process", (e) => {
        setRunning((r) => ({ ...r, [e.payload.id]: !!e.payload.running }));
        if (e.payload.exitCode)
          setError(
            `Game exited with code ${e.payload.exitCode}. Open its logs for details.`,
          );
      }),
      getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          if (pageRef.current === "Conversations")
            window.dispatchEvent(
              new CustomEvent("orbit-files", { detail: e.payload.paths }),
            );
          else {
            const paths = e.payload.paths.filter((p) =>
              /\.(exe|lnk)$/i.test(p),
            );
            if (paths.length) importPaths(paths);
            else
              setError(
                "Open a conversation to attach photos, videos or files.",
              );
          }
        }
      }),
    ];
    return () => {
      ps.forEach((p) => p.then((un) => un()));
    };
  }, []);
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(""), 6000);
    return () => clearTimeout(t);
  }, [notice]);
  async function importPaths(paths: string[]) {
    for (const path of paths) {
      try {
        const item = await api<Item>("program_import", { path });
        await api("save_item", item);
      } catch (e) {
        setError(errorText(e));
      }
    }
    await load();
    setPage("Programs");
  }
  const pick = async () => {
    try {
      const paths = await open({
        multiple: true,
        filters: [{ name: "Programs & shortcuts", extensions: ["exe", "lnk"] }],
      });
      if (paths) await importPaths(Array.isArray(paths) ? paths : [paths]);
    } catch (e) {
      setError(errorText(e));
    }
  };
  const save = (item: Item) =>
    perform(
      "save",
      async () => {
        await api("save_item", item);
        setEditor(null);
      },
      "Saved to your library",
    );
  const favorite = (item: Item) =>
    perform("favorite", () =>
      api("save_item", { ...item, favorite: !item.favorite }),
    );
  const launch = (item: Item) =>
    perform(item.id, async () => {
      const result = await api(
        `${item.kind === "minecraft" ? "mc" : item.kind}_launch`,
        { id: item.id, universeId: item.universeId },
      );
      setNotice(result.message || `${item.name} started`);
    });
  const install = (item: Item) =>
    perform(
      item.id,
      async () => {
        setProgress({ stage: "Preparing installation", done: 0, total: 1 });
        try {
          await api("mc_install", { id: item.id });
          setJobs((j) => [
            {
              stage: `${item.name} installed`,
              done: 1,
              total: 1,
              at: new Date().toLocaleTimeString(),
            },
            ...j,
          ]);
        } catch (e) {
          setJobs((j) => [
            { stage: item.name, error: errorText(e), done: 0, total: 1 },
            ...j,
          ]);
          throw e;
        } finally {
          setProgress(null);
        }
      },
      "Installation verified. Ready to play.",
    );
  const newProfile = () =>
    setEditor({
      id: crypto.randomUUID(),
      kind: "minecraft",
      name: "My Minecraft",
      version: "1.21.1",
      loader: "vanilla",
      memory: 4096,
      width: 1280,
      height: 720,
      javaArgs: [],
      account: "offline",
      offlineName: "Player",
      fullscreen: false,
    });
  const visible = items.filter((i) =>
    i.name.toLowerCase().includes(query.toLowerCase()),
  );
  const card = (i: Item) => (
    <article
      className="item-card"
      key={i.id}
      onContextMenu={(e) => {
        e.preventDefault();
        setContext({
          item: i,
          x: Math.min(e.clientX, window.innerWidth - 230),
          y: Math.min(e.clientY, window.innerHeight - 330),
        });
      }}
    >
      <button
        className="card-cover"
        onClick={() =>
          i.kind === "roblox"
            ? setDetail({ universeId: i.universeId })
            : launch(i)
        }
      >
        <Cover src={i.image} kind={i.kind} name={i.name} />
        <span className="cover-play">
          <Play size={22} fill="currentColor" />
        </span>
        {i.favorite && (
          <span className="cover-star">
            <Star size={13} fill="currentColor" />
          </span>
        )}
      </button>
      <div className="card-body">
        <div>
          <h3>{i.name}</h3>
          <p>
            {i.kind === "minecraft"
              ? `${i.version} · ${i.loader}`
              : i.kind === "roblox"
                ? "Roblox experience"
                : "Desktop application"}
          </p>
        </div>
        <button
          className="icon-btn"
          title="Item options"
          onClick={(e) =>
            setContext({
              item: i,
              x: Math.min(e.clientX, window.innerWidth - 230),
              y: Math.min(e.clientY, window.innerHeight - 330),
            })
          }
        >
          <MoreHorizontal size={19} />
        </button>
      </div>
      <div className="card-actions">
        <button
          disabled={busy === i.id || running[i.id]}
          onClick={() => launch(i)}
        >
          <Play size={13} />
          {running[i.id] ? "Running" : busy === i.id ? "Working…" : "Play"}
        </button>
        {i.kind === "minecraft" && (
          <button
            title="Install or repair all files"
            disabled={!!busy}
            onClick={() => install(i)}
          >
            <Download size={13} /> Install / Repair
          </button>
        )}
        <button
          className="icon-btn"
          title="Edit entry"
          onClick={() => setEditor(i)}
        >
          <SlidersHorizontal size={14} />
        </button>
      </div>
    </article>
  );
  const section = (
    title: string,
    data: Item[],
    emptyTitle: string,
    emptyText: string,
  ) => (
    <section className="library-section">
      <div className="section-title">
        <h2>
          {title}
          <span>{data.length}</span>
        </h2>
      </div>
      {data.length ? (
        <div className="card-grid">{data.map(card)}</div>
      ) : (
        <Empty title={emptyTitle}>{emptyText}</Empty>
      )}
    </section>
  );
  return (
    <div className="app" onClick={() => context && setContext(null)}>
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-symbol">◒</span>orbit
          <span className="brand-dot">.</span>
        </div>
        <div className="nav-label">YOUR SPACE</div>
        <nav>
          {nav.map(({ name, icon: Icon }, n) => (
            <React.Fragment key={name}>
              {n === 4 && <div className="nav-divider" />}
              <button
                className={page === name ? "active" : ""}
                onClick={() => {
                  setPage(name);
                  setQuery("");
                }}
              >
                <Icon size={19} />
                <span>{name}</span>
                {name === "Downloads" && progress && (
                  <span className="nav-count">1</span>
                )}
              </button>
            </React.Fragment>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="local-pill">
            <span /> Local-first. Always yours.
          </div>
          <button className="account-button" onClick={() => setPage("Profile")}>
            <span className="avatar">
              {orbitProfile?.name?.slice(0, 1) ||
                account?.name?.slice(0, 1) ||
                "P"}
            </span>
            <span>
              <strong>
                {orbitProfile?.name || account?.name || "Your space"}
              </strong>
              <small>
                {account ? "Microsoft connected" : "On this computer"}
              </small>
            </span>
            <Settings size={15} />
          </button>
          <div className="version">
            ORBIT LAUNCHER <span>v0.2.0</span>
          </div>
        </div>
      </aside>
      <div className="main-shell">
        <header>
          <div className="breadcrumb">
            Workspace
            <ChevronRight size={13} />
            <strong>{page}</strong>
          </div>
          <div className="search">
            <Search size={16} />
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={
                page === "Roblox"
                  ? "Search Roblox experiences…"
                  : "Search your library…"
              }
              aria-label="Search"
            />
            <kbd>Ctrl K</kbd>
            {query && (
              <button className="icon-btn" onClick={() => setQuery("")}>
                <X size={13} />
              </button>
            )}
          </div>
          <span className="header-status">
            <span /> LOCAL STORAGE
          </span>
        </header>
        <main>
          {(
            [
              "Profile",
              "Connections",
              "Friends",
              "Conversations",
              "Activity",
            ] as string[]
          ).includes(page) && (
            <SocialView
              page={page}
              onNavigate={(p) => setPage(p as Page)}
              onProfileChanged={load}
            />
          )}
          {!native && (
            <div className="alert">
              Browser preview · Native integrations are available in the desktop
              app. No sample library data is shown.
            </div>
          )}
          {error && (
            <div className="alert error">
              <AlertCircle size={18} />
              <span>{error}</span>
              <button className="icon-btn" onClick={() => setError("")}>
                <X size={16} />
              </button>
            </div>
          )}
          {page === "Home" && (
            <>
              <div className="page-heading">
                <div>
                  <p className="eyebrow">
                    A LITTLE LESS FRICTION. A LOT MORE PLAY.
                  </p>
                  <h1>Your next adventure.</h1>
                  <p>All your worlds. One place to begin.</p>
                </div>
                <button className="secondary" onClick={pick}>
                  <Plus size={16} /> Add a program
                </button>
              </div>
              <div className="hero">
                <div className="hero-art">
                  <div className="sun" />
                  <div className="mountain back" />
                  <div className="mountain front" />
                  <div className="landscape-grid" />
                  <span className="floating-block block-one" />
                  <span className="floating-block block-two" />
                </div>
                <div className="hero-content">
                  <span className="tag">
                    <span /> YOUR WORLD, YOUR WAY
                  </span>
                  <h2>
                    A fresh world
                    <br />
                    is waiting.
                  </h2>
                  <p>
                    Build something extraordinary.
                    <br />
                    Minecraft Java, right from Orbit.
                  </p>
                  <button
                    className="primary light"
                    onClick={() => setPage("Minecraft")}
                  >
                    <Play size={16} fill="currentColor" /> Explore Minecraft{" "}
                    <ArrowUpRight size={16} />
                  </button>
                  <span className="hero-footnote">
                    VANILLA & FABRIC · INDEPENDENT PROFILES
                  </span>
                </div>
                <div className="hero-label">
                  <Boxes size={19} /> MINECRAFT <span>JAVA EDITION</span>
                </div>
              </div>
              <div className="quick-links">
                <button onClick={() => setPage("Roblox")}>
                  <span className="quick-icon purple">
                    <Grid2X2 size={23} />
                  </span>
                  <span>
                    <strong>Find your next favorite</strong>
                    <small>Explore experiences on Roblox</small>
                  </span>
                  <ArrowUpRight size={19} />
                </button>
                <button onClick={pick}>
                  <span className="quick-icon blue">
                    <Monitor size={23} />
                  </span>
                  <span>
                    <strong>Make yourself at home</strong>
                    <small>Add your games and everyday apps</small>
                  </span>
                  <Plus size={19} />
                </button>
              </div>
              {section(
                "Jump back in",
                visible.filter((i) => i.lastLaunch).slice(0, 8),
                "Your adventures start here",
                "Launch a Minecraft profile, Roblox experience, or program and it will appear here.",
              )}
              {section(
                "Pinned favorites",
                visible.filter((i) => i.favorite),
                "Keep the good ones close",
                "Use the star in an item’s menu to pin it to Home.",
              )}
            </>
          )}
          {page === "Minecraft" && (
            <>
              <Heading
                eyebrow="JAVA EDITION"
                title="A world of possibilities."
                subtitle="Install, customize, and launch. Every profile is its own world."
                action={
                  <button className="primary" onClick={newProfile}>
                    <Plus size={16} /> New profile
                  </button>
                }
              />
              <div className="info-strip">
                <Boxes size={23} />
                <div>
                  <strong>Built for your way to play</strong>
                  <p>
                    Direct installation · Managed Java · Isolated game folders ·
                    Verified downloads
                  </p>
                </div>
              </div>
              <div className="account-strip">
                <span className="avatar">
                  <Gamepad2 size={19} />
                </span>
                <div>
                  <strong>
                    {account ? account.name : "Choose how you play"}
                  </strong>
                  <p>
                    {account
                      ? "Microsoft account connected · Entitlement checked again at launch"
                      : "Connect Microsoft from your Profile. Availability depends on this Orbit release."}
                  </p>
                </div>
                <button
                  className="secondary"
                  disabled={!!busy}
                  onClick={() =>
                    publisher.microsoft
                      ? perform(
                          "signin",
                          () => api("mc_signin"),
                          "Microsoft account connected",
                        )
                      : setPage("Connections")
                  }
                >
                  {busy === "signin"
                    ? "Waiting for browser…"
                    : account
                      ? "Switch account"
                      : "Connect Microsoft"}
                  <ArrowUpRight size={14} />
                </button>
              </div>
              {section(
                "Installation profiles",
                visible.filter((i) => i.kind === "minecraft"),
                "Start with a new profile",
                "Choose your Minecraft version, memory, and account. Orbit downloads the files and Java you need.",
              )}
              <div className="note">
                <AlertCircle size={16} />
                Offline profiles are local identities. They do not grant
                ownership or access to authenticated multiplayer. The Fabric
                performance preset targets 1.21.1 and installs Sodium, Lithium,
                and FerriteCore directly from Modrinth.
              </div>
            </>
          )}
          {page === "Roblox" && (
            <RobloxView query={query} onDetails={setDetail} />
          )}
          {page === "Programs" && (
            <>
              <Heading
                eyebrow="YOUR DESKTOP, ORGANIZED"
                title="Everything you love to open."
                subtitle="Games, tools, and everyday essentials. Drag an EXE or shortcut anywhere to add it."
                action={
                  <button className="primary" onClick={pick}>
                    <Plus size={16} /> Add program
                  </button>
                }
              />
              {section(
                "Your programs",
                visible.filter((i) => i.kind === "program"),
                "Bring your favorites along",
                "Choose installed .exe files or Windows shortcuts. Orbit keeps the originals where they are.",
              )}
              {!items.some((i) => i.kind === "program") && (
                <button className="dropzone" onClick={pick}>
                  <Plus size={26} />
                  <strong>Drop an EXE or shortcut here</strong>
                  <span>or click to browse your computer</span>
                </button>
              )}
            </>
          )}
          {page === "Collections" && (
            <>
              <Heading
                eyebrow="A PLACE FOR EVERYTHING"
                title="Curate your space."
                subtitle="Group your games and apps in a way that makes sense to you."
                action={
                  <button
                    className="primary"
                    onClick={() => setSelectedCollection("__new")}
                  >
                    <Plus size={16} /> New collection
                  </button>
                }
              />
              {selectedCollection === "__new" && (
                <CollectionForm
                  onSave={(name) =>
                    perform("collection", async () => {
                      await api("save_collection", { name });
                      setSelectedCollection("");
                    })
                  }
                  onClose={() => setSelectedCollection("")}
                />
              )}
              <div className="collection-tabs">
                <button
                  className={!selectedCollection ? "selected" : ""}
                  onClick={() => setSelectedCollection("")}
                >
                  All collections
                </button>
                {collections.map((c) => (
                  <button
                    key={c.id}
                    className={selectedCollection === c.id ? "selected" : ""}
                    onClick={() => setSelectedCollection(c.id)}
                  >
                    {c.name}
                    <span>{c.items.length}</span>
                  </button>
                ))}
              </div>
              {selectedCollection && selectedCollection !== "__new" ? (
                section(
                  collections.find((c) => c.id === selectedCollection)?.name ||
                    "",
                  visible.filter((i) =>
                    collections
                      .find((c) => c.id === selectedCollection)
                      ?.items.includes(i.id),
                  ),
                  "This collection is ready for you",
                  "Open an item’s menu to add it to this collection.",
                )
              ) : collections.length ? (
                <div className="collection-grid">
                  {collections.map((c) => (
                    <button
                      key={c.id}
                      onClick={() => setSelectedCollection(c.id)}
                    >
                      <Folder size={36} />
                      <h3>{c.name}</h3>
                      <span>
                        {c.items.length} items <ChevronRight size={14} />
                      </span>
                    </button>
                  ))}
                </div>
              ) : (
                <Empty icon={Folder} title="Your library, your order">
                  Create a collection, then add items from their context menus.
                </Empty>
              )}
            </>
          )}
          {page === "Downloads" && (
            <>
              <Heading
                eyebrow="BEHIND THE ADVENTURE"
                title="Getting you ready."
                subtitle="Installation progress and results for this session."
              />
              {progress && <ProgressCard value={progress} />}
              {jobs.map((j, n) => (
                <ProgressCard key={n} value={j} />
              ))}
              {!progress && !jobs.length && (
                <Empty icon={Download} title="All quiet here">
                  Install a Minecraft profile to see download progress.
                  Interrupted installations resume when you choose Install /
                  Repair.
                </Empty>
              )}
              <div className="note">
                <CheckCircle2 size={16} />
                Files are verified against published SHA-1 hashes. Up to four
                files download at once.
              </div>
            </>
          )}
          {page === "Settings" && (
            <>
              <Heading
                eyebrow="MAKE IT YOURS"
                title="A little fine-tuning."
                subtitle="Your preferences stay on this computer."
              />
              <div className="settings-panel">
                <h2>Your connected accounts</h2>
                <p>
                  Manage authenticated accounts and profile links in Profile →
                  Connections. Orbit handles publisher registrations; you never
                  need to enter developer keys.
                </p>
                <button
                  className="secondary"
                  onClick={() => setPage("Connections")}
                >
                  Manage connections <ArrowUpRight size={15} />
                </button>
              </div>
              <div className="settings-panel">
                <h2>After a game starts</h2>
                <p>Choose what Orbit does once a launch has been dispatched.</p>
                <select
                  value={settings.afterLaunch || "stay"}
                  onChange={(e) =>
                    perform("settings", () =>
                      api("setting", {
                        key: "afterLaunch",
                        value: e.target.value,
                      }),
                    )
                  }
                >
                  <option value="stay">Keep Orbit open</option>
                  <option value="minimize">Minimize the window</option>
                  <option value="exit">Exit Orbit</option>
                </select>
              </div>
              <div className="settings-panel">
                <h2>Roblox browsing</h2>
                <p>
                  Personalized Home feed access is restricted. Public Explore is
                  a separate live catalog.
                </p>
                <p>
                  Public Explore loads automatically. Open official Home in your
                  browser for its personalized view; Orbit cannot read that
                  session.
                </p>
              </div>
              <div className="settings-panel about">
                <span className="brand-symbol">◒</span>
                <div>
                  <h2>Orbit 0.2.0 · Social prototype</h2>
                  <p>
                    Rust + Tauri + TypeScript + SQLite. No Orbit account. No
                    hosted backend.
                    <br />
                    An independent launcher, unaffiliated with Mojang,
                    Microsoft, or Roblox.
                  </p>
                </div>
              </div>
            </>
          )}
        </main>
        <footer>
          <span>
            <span className="status-dot" /> Your library, on your computer
          </span>
          <span>
            {items.length} {items.length === 1 ? "item" : "items"} in your space{" "}
            <Minus size={12} /> Orbit 0.2.0
          </span>
        </footer>
      </div>
      {progress && page !== "Downloads" && (
        <button className="download-toast" onClick={() => setPage("Downloads")}>
          <Download size={19} />
          <span>
            <strong>{progress.stage}</strong>
            <small>
              {progress.done} / {progress.total} files
            </small>
            <i
              style={{
                width: `${progress.total ? (progress.done / progress.total) * 100 : 0}%`,
              }}
            />
          </span>
          <ChevronRight size={17} />
        </button>
      )}
      {notice && (
        <div className="toast">
          <Check size={18} />
          {notice}
        </div>
      )}
      {editor && (
        <Editor
          item={editor}
          busy={!!busy}
          onClose={() => setEditor(null)}
          onSave={save}
        />
      )}
      {detail && (
        <ExperienceDetail
          universeId={detail.universeId}
          onClose={() => setDetail(null)}
          onPlay={(i) => launch(i)}
          onPin={(i) =>
            perform(
              "pin",
              () => api("save_item", { ...i, favorite: true }),
              "Pinned to Home",
            )
          }
        />
      )}
      {logs && (
        <div className="modal-backdrop">
          <div className="modal wide">
            <div className="modal-heading">
              <h2>{logs.name} · Launch logs</h2>
              <button className="icon-btn" onClick={() => setLogs(null)}>
                <X />
              </button>
            </div>
            <pre>{logs.text}</pre>
          </div>
        </div>
      )}
      {context && (
        <div
          className="context-menu"
          style={{ left: context.x, top: context.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => {
              launch(context.item);
              setContext(null);
            }}
          >
            <Play size={15} />
            Play
          </button>
          <button
            onClick={() => {
              favorite(context.item);
              setContext(null);
            }}
          >
            <Star size={15} />
            {context.item.favorite ? "Unpin from Home" : "Pin to Home"}
          </button>
          <button
            onClick={() => {
              setEditor(context.item);
              setContext(null);
            }}
          >
            <Pencil size={15} />
            Edit entry
          </button>
          {context.item.kind === "minecraft" && (
            <button
              onClick={() => {
                const i = context.item;
                setContext(null);
                perform("logs", async () =>
                  setLogs({
                    name: i.name,
                    text: await api("logs", { id: i.id }),
                  }),
                );
              }}
            >
              <Terminal size={15} />
              View launch logs
            </button>
          )}
          {collections.map((c) => (
            <button
              key={c.id}
              onClick={() => {
                perform("collect", () =>
                  api("collect", {
                    id: context.item.id,
                    collection: c.id,
                    remove: c.items.includes(context.item.id),
                  }),
                );
                setContext(null);
              }}
            >
              <Folder size={15} />
              {c.items.includes(context.item.id)
                ? "Remove from"
                : "Add to"}{" "}
              {c.name}
            </button>
          ))}
          <button
            className="danger"
            onClick={() => {
              perform(
                "remove",
                () => api("remove_item", { id: context.item.id }),
                "Removed from library. Installed files are preserved.",
              );
              setContext(null);
            }}
          >
            <Trash2 size={15} />
            Remove from library
          </button>
        </div>
      )}
    </div>
  );
}
function ProgressCard({ value: v }: { value: Progress }) {
  return (
    <div className={`progress-card ${v.error ? "failed" : ""}`}>
      {v.error ? (
        <AlertCircle />
      ) : v.done === v.total ? (
        <CheckCircle2 />
      ) : (
        <Download />
      )}
      <div>
        <h3>{v.stage}</h3>
        <p>
          {v.error ||
            `${v.done} of ${v.total} files · ${v.at || "Downloading and verifying"}`}
        </p>
        <progress value={v.done} max={v.total || 1} />
      </div>
    </div>
  );
}
function CollectionForm({
  onSave,
  onClose,
}: {
  onSave: (name: string) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  return (
    <form
      className="inline-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim()) onSave(name);
      }}
    >
      <input
        autoFocus
        placeholder="Collection name"
        value={name}
        maxLength={100}
        onChange={(e) => setName(e.target.value)}
      />
      <button className="primary">Create collection</button>
      <button type="button" className="icon-btn" onClick={onClose}>
        <X />
      </button>
    </form>
  );
}
function Editor({
  item,
  busy,
  onClose,
  onSave,
}: {
  item: Item;
  busy: boolean;
  onClose: () => void;
  onSave: (i: Item) => void;
}) {
  const [v, setV] = useState(item),
    [versions, setVersions] = useState<{ id: string }[]>([]),
    [args, setArgs] = useState(
      JSON.stringify(
        item.kind === "minecraft" ? item.javaArgs : item.args || [],
      ),
    ),
    [error, setError] = useState("");
  useEffect(() => {
    if (item.kind === "minecraft")
      api<{ id: string }[]>("mc_versions")
        .then(setVersions)
        .catch((e) => setError(errorText(e)));
  }, []);
  const field = (k: keyof Item, val: any) => setV((x) => ({ ...x, [k]: val }));
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <form
        className="modal"
        onSubmit={(e) => {
          e.preventDefault();
          try {
            const a = JSON.parse(args);
            if (!Array.isArray(a) || !a.every((x) => typeof x === "string"))
              throw new Error("Arguments must be a JSON array of strings.");
            onSave({
              ...v,
              [item.kind === "minecraft" ? "javaArgs" : "args"]: a,
            });
          } catch (e) {
            setError(errorText(e));
          }
        }}
      >
        <div className="modal-heading">
          <div>
            <p className="eyebrow">
              {item.kind === "minecraft"
                ? "INSTALLATION PROFILE"
                : "LIBRARY ENTRY"}
            </p>
            <h2>
              {item.kind === "minecraft"
                ? "Your world, your settings."
                : "Make it yours."}
            </h2>
          </div>
          <button type="button" className="icon-btn" onClick={onClose}>
            <X />
          </button>
        </div>
        {error && <div className="alert error">{error}</div>}
        <label>
          Name
          <input
            required
            value={v.name}
            maxLength={200}
            onChange={(e) => field("name", e.target.value)}
          />
        </label>
        {item.kind === "minecraft" ? (
          <>
            <div className="form-grid">
              <label>
                Minecraft version
                <select
                  disabled={!!v.performance}
                  value={v.version}
                  onChange={(e) => field("version", e.target.value)}
                >
                  {!versions.some((x) => x.id === v.version) && (
                    <option>{v.version}</option>
                  )}
                  {versions.map((x) => (
                    <option key={x.id}>{x.id}</option>
                  ))}
                </select>
              </label>
              <label>
                Loader
                <select
                  disabled={!!item.performance}
                  value={v.performance ? "performance" : v.loader}
                  onChange={(e) =>
                    setV((x) => ({
                      ...x,
                      loader:
                        e.target.value === "performance"
                          ? "fabric"
                          : e.target.value,
                      performance: e.target.value === "performance",
                      ...(e.target.value === "performance"
                        ? { version: "1.21.1" }
                        : {}),
                    }))
                  }
                >
                  <option value="vanilla">Vanilla</option>
                  <option value="fabric">Fabric Loader</option>
                  <option value="performance">
                    Fabric · Performance preset (1.21.1)
                  </option>
                </select>
              </label>
            </div>
            <div className="form-grid">
              <label>
                Account
                <select
                  value={v.account}
                  onChange={(e) => field("account", e.target.value)}
                >
                  <option value="offline">
                    Offline profile · no authentication
                  </option>
                  <option value="microsoft">
                    Microsoft · requires Java ownership
                  </option>
                </select>
              </label>
              {v.account === "offline" && (
                <label>
                  Offline player name
                  <input
                    value={v.offlineName}
                    minLength={3}
                    maxLength={16}
                    pattern="[A-Za-z0-9_]+"
                    required
                    onChange={(e) => field("offlineName", e.target.value)}
                  />
                </label>
              )}
            </div>
            {v.account === "offline" && (
              <div className="note">
                Offline identities do not provide ownership or authenticated
                multiplayer access.
              </div>
            )}
            <div className="form-grid thirds">
              <label>
                Memory (MB)
                <input
                  type="number"
                  min={1024}
                  max={32768}
                  step={512}
                  value={v.memory}
                  onChange={(e) => field("memory", +e.target.value)}
                />
              </label>
              <label>
                Width
                <input
                  type="number"
                  min={480}
                  max={7680}
                  value={v.width}
                  onChange={(e) => field("width", +e.target.value)}
                />
              </label>
              <label>
                Height
                <input
                  type="number"
                  min={480}
                  max={7680}
                  value={v.height}
                  onChange={(e) => field("height", +e.target.value)}
                />
              </label>
            </div>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={v.fullscreen}
                onChange={(e) => field("fullscreen", e.target.checked)}
              />{" "}
              Start fullscreen
            </label>
            <p className="muted">
              Game directory: Orbit app data / minecraft / profiles / {v.id} /
              game. Each profile keeps its own worlds, options, and mods.
            </p>
          </>
        ) : item.kind === "program" ? (
          <>
            <label>
              Executable path
              <div className="input-row">
                <input
                  required
                  value={v.path || ""}
                  onChange={(e) => field("path", e.target.value)}
                />
                <button
                  type="button"
                  className="secondary"
                  onClick={async () => {
                    const path = await open({
                      filters: [{ name: "Executable", extensions: ["exe"] }],
                    });
                    if (typeof path === "string") field("path", path);
                  }}
                >
                  Browse
                </button>
              </div>
            </label>
            <label>
              Working directory
              <input
                value={v.workingDir || ""}
                placeholder="Defaults to the executable’s folder"
                onChange={(e) => field("workingDir", e.target.value)}
              />
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={!!v.gameActivity}
                onChange={(e) => field("gameActivity", e.target.checked)}
              />
              Recognize this program as game activity
            </label>
            <label>
              Cover image
              <div className="input-row">
                <button
                  type="button"
                  className="secondary"
                  onClick={async () => {
                    try {
                      const path = await open({
                        filters: [
                          {
                            name: "Cover image",
                            extensions: ["png", "jpg", "jpeg"],
                          },
                        ],
                      });
                      if (typeof path === "string")
                        field("image", await api("program_image", { path }));
                    } catch (e) {
                      setError(errorText(e));
                    }
                  }}
                >
                  Choose PNG or JPEG
                </button>
                {v.image && (
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => field("image", "")}
                  >
                    Clear image
                  </button>
                )}
              </div>
              <span className="muted">Stored locally · Up to 5 MB</span>
            </label>
          </>
        ) : (
          <p className="muted">
            Roblox universe {v.universeId}. The official runtime controls
            sign-in and joining.
          </p>
        )}
        {item.kind !== "roblox" && (
          <label>
            {item.kind === "minecraft"
              ? "Extra Java arguments"
              : "Launch arguments"}{" "}
            (JSON array)
            <textarea
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              rows={2}
              spellCheck={false}
              placeholder={'["--example", "value"]'}
            />
          </label>
        )}
        <div className="modal-footer">
          <button type="button" className="secondary" onClick={onClose}>
            Cancel
          </button>
          <button className="primary" disabled={busy}>
            {busy ? "Saving…" : "Save to library"}
          </button>
        </div>
      </form>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <Suspense fallback={<div className="loading">Loading Orbit…</div>}>
    <App />
  </Suspense>,
);
