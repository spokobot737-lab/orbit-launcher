import React, { useState, useEffect, useRef } from "react";
import {
  AlertCircle,
  ArrowUpRight,
  ChevronRight,
  Grid2X2,
  Search,
  RefreshCw,
  Play,
  Star,
  X,
} from "lucide-react";
import { api, Item, errorText } from "./api";
import { Cover, Empty, Heading } from "./components";
type Game = {
  universeId: number;
  rootPlaceId?: number;
  name?: string;
  playerCount?: number;
  image?: string;
  creator?: string;
  description?: string;
  playing?: number;
  visits?: number;
  genre?: string;
};
export default function RobloxView({
  query,
  onDetails,
  settings,
  onPublic,
}: {
  query: string;
  onDetails: (v: any) => void;
  settings: any;
  onPublic: () => void;
}) {
  const [homeError, setHomeError] = useState(""),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [sorts, setSorts] = useState<any[]>([]),
    [stale, setStale] = useState(false),
    [mode, setMode] = useState(!!settings.robloxPublic),
    [results, setResults] = useState<Game[]>([]),
    [token, setToken] = useState("");
  const [searched, setSearched] = useState(false);
  const searchGeneration = useRef(0);
  useEffect(() => {
    searchGeneration.current++;
    setResults([]);
    setToken("");
    setSearched(false);
  }, [query]);
  const home = async () => {
    setLoading(true);
    try {
      await api("roblox_home");
      setHomeError(
        "Home responded, but its response format has not been validated. No replacement catalog is presented as Home.",
      );
    } catch (e) {
      setHomeError(errorText(e));
    } finally {
      setLoading(false);
    }
  };
  const explore = async () => {
    setLoading(true);
    setError("");
    try {
      const v = await api("roblox_explore");
      setSorts(
        (v.data.sorts || []).filter(
          (s: any) => s.contentType === "Games" && s.games?.length,
        ),
      );
      setStale(v.stale);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    home();
  }, []);
  useEffect(() => {
    if (mode) explore();
  }, [mode]);
  const doSearch = async (append = false) => {
    const generation = ++searchGeneration.current;
    setLoading(true);
    setError("");
    try {
      const v = await api("roblox_search", {
        query,
        pageToken: append ? token : undefined,
      });
      const games = (v.searchResults || [])
        .filter((x: any) => x.contentGroupType === "Game")
        .flatMap((x: any) => x.contents || [])
        .map((g: any) => ({
          ...g,
          universeId: g.universeId || g.contentId,
          name: g.name,
        }));
      if (generation !== searchGeneration.current) return;
      setSearched(true);
      setResults((old) => (append ? [...old, ...games] : games));
      setToken(v.nextPageToken || "");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };
  return (
    <>
      <Heading
        eyebrow="ROBLOX EXPERIENCES"
        title="Find your kind of play."
        subtitle="Discover an experience. Launch it in the official Roblox runtime."
        action={
          <button
            className="secondary"
            onClick={() =>
              query.length > 1 ? doSearch() : mode ? explore() : home()
            }
          >
            <RefreshCw size={15} /> Refresh
          </button>
        }
      />
      {homeError && (
        <div className="feed-notice">
          <AlertCircle size={19} />
          <div>
            <strong>Personalized Home is unavailable</strong>
            <p>{homeError}</p>
            <button onClick={home}>Retry Home feed</button>
            {!mode && (
              <button
                onClick={() => {
                  setMode(true);
                  onPublic();
                }}
              >
                Browse public Explore instead <ArrowUpRight size={13} />
              </button>
            )}
          </div>
        </div>
      )}
      {query.length > 0 && (
        <form
          className="roblox-search"
          onSubmit={(e) => {
            e.preventDefault();
            doSearch();
          }}
        >
          <span>Search Roblox for “{query}”</span>
          <button className="primary" disabled={loading || query.length < 2}>
            <Search size={15} /> Search experiences
          </button>
        </form>
      )}
      {error && (
        <div className="alert error">
          {error}
          <button onClick={() => (query.length > 1 ? doSearch() : explore())}>
            Retry
          </button>
        </div>
      )}
      {loading && (
        <div className="loading">
          <RefreshCw size={19} />
          Connecting to Roblox…
        </div>
      )}
      {results.length > 0 ? (
        <>
          <ExperienceRow
            title="Search results"
            games={results}
            onDetails={onDetails}
          />
          {token && (
            <button
              className="secondary"
              disabled={loading}
              onClick={() => doSearch(true)}
            >
              Load more results
            </button>
          )}
        </>
      ) : query.length > 1 && !loading ? (
        <Empty
          icon={Search}
          title={searched ? "No matching experiences" : "Search Roblox"}
        >
          {searched
            ? "Try a different name or keyword."
            : "Press Search experiences to retrieve matching results."}
        </Empty>
      ) : (
        mode && (
          <>
            <div className="catalog-label">
              <span className="status-dot" /> PUBLIC EXPLORE{" "}
              <span>
                {stale
                  ? "Cached metadata · network unavailable"
                  : "Live Roblox discovery · Not your personalized Home feed"}
              </span>
            </div>
            {sorts.map((s) => (
              <ExperienceRow
                key={s.sortId}
                title={s.sortDisplayName}
                games={s.games}
                onDetails={onDetails}
              />
            ))}
            {!loading && !sorts.length && !error && (
              <Empty title="No experiences returned">
                Roblox returned an empty catalog. Try refreshing.
              </Empty>
            )}
          </>
        )
      )}
    </>
  );
}
function ExperienceRow({
  title,
  games,
  onDetails,
}: {
  title: string;
  games: Game[];
  onDetails: (v: any) => void;
}) {
  const [count, setCount] = useState(8),
    [metadata, setMetadata] = useState<Record<number, any>>({}),
    [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    const ids = games
      .slice(0, count)
      .map((g) => g.universeId)
      .filter(Boolean);
    if (!ids.length) return;
    const chunks: number[][] = [];
    for (let n = 0; n < ids.length; n += 50) chunks.push(ids.slice(n, n + 50));
    (async () => {
      const out: Record<number, any> = {};
      for (const ids of chunks) {
        const data = await api("roblox_details", { ids });
        for (const g of data.games || [])
          out[g.id] = {
            ...g,
            image: (data.thumbnails || []).find((t: any) => t.targetId === g.id)
              ?.imageUrl,
          };
      }
      if (live) {
        setMetadata(out);
        setError("");
      }
    })().catch((e) => live && setError(errorText(e)));
    return () => {
      live = false;
    };
  }, [games, count]);
  return (
    <section className="library-section experience-section">
      <div className="section-title">
        <h2>
          {title}
          <span>{games.length}</span>
        </h2>
        {count < games.length && (
          <button
            onClick={() => setCount((n) => Math.min(n + 8, games.length))}
          >
            Show more <ChevronRight size={14} />
          </button>
        )}
      </div>
      {error && <p className="muted">Metadata unavailable: {error}</p>}
      <div className="experience-grid">
        {games.slice(0, count).map((g, n) => {
          const m = metadata[g.universeId] || {};
          return (
            <button
              key={`${g.universeId}-${n}`}
              className="experience-card"
              onClick={() => onDetails({ universeId: g.universeId })}
            >
              <Cover kind="roblox" src={m.image} name={m.name || g.name} />
              <h3>{m.name || g.name || `Experience ${g.universeId}`}</h3>
              <p>{m.creator?.name || "Loading creator…"}</p>
              <span>
                <span className="status-dot" />
                {(m.playing ?? g.playerCount ?? 0).toLocaleString()} playing
                {m.visits != null && (
                  <small>
                    {Intl.NumberFormat("en", { notation: "compact" }).format(
                      m.visits,
                    )}{" "}
                    visits
                  </small>
                )}
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
export function ExperienceDetail({
  universeId,
  onClose,
  onPlay,
  onPin,
}: {
  universeId: number;
  onClose: () => void;
  onPlay: (i: Item) => void;
  onPin: (i: Item) => void;
}) {
  const [game, setGame] = useState<any>(null),
    [error, setError] = useState("");
  const load = () =>
    api("roblox_details", { ids: [universeId] })
      .then((d) => {
        if (!d.games?.[0]) throw new Error("This experience is unavailable.");
        setGame({ ...d.games[0], image: d.thumbnails?.[0]?.imageUrl });
      })
      .catch((e) => setError(errorText(e)));
  useEffect(() => {
    load();
  }, [universeId]);
  const item: Item = {
    id: `roblox-${universeId}`,
    kind: "roblox",
    name: game?.name || "",
    universeId,
    rootPlaceId: game?.rootPlaceId,
    image: game?.image,
  };
  return (
    <div className="modal-backdrop">
      <div className="modal experience-detail">
        <button className="icon-btn close-detail" onClick={onClose}>
          <X />
        </button>
        {error ? (
          <Empty
            icon={AlertCircle}
            title="Experience unavailable"
            action={
              <button className="secondary" onClick={load}>
                Retry
              </button>
            }
          >
            {error}
          </Empty>
        ) : !game ? (
          <div className="loading">Loading experience…</div>
        ) : (
          <>
            <Cover src={game.image} kind="roblox" name={game.name} />
            <span className="eyebrow">ROBLOX EXPERIENCE</span>
            <h2>{game.name}</h2>
            <p>By {game.creator?.name}</p>
            <div className="experience-stats">
              <span>
                <strong>{game.playing?.toLocaleString()}</strong>Playing now
              </span>
              <span>
                <strong>
                  {Intl.NumberFormat("en", { notation: "compact" }).format(
                    game.visits || 0,
                  )}
                </strong>
                Visits
              </span>
              <span>
                <strong>{game.maxPlayers}</strong>Server size
              </span>
            </div>
            <div className="button-row">
              <button className="primary" onClick={() => onPlay(item)}>
                <Play size={16} fill="currentColor" /> Play experience
              </button>
              <button className="secondary" onClick={() => onPin(item)}>
                <Star size={16} /> Pin to Home
              </button>
            </div>
            <p className="description">{game.description}</p>
            <div className="note">
              Requires the installed official Roblox runtime. Its active
              account, parental controls, and experience permissions apply.
              Orbit can confirm dispatch, not successful entry into a server.
            </div>
          </>
        )}
      </div>
    </div>
  );
}
