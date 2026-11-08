import { useEffect, useState } from "react";
import { Gamepad2, Music2, Radio, RefreshCw } from "lucide-react";
import { api, imageUrl } from "./api";
function useExpiry() {
  const [, tick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => tick((n) => n + 1), 15000);
    return () => clearInterval(timer);
  }, []);
}
function Artwork({ url }: { url: string }) {
  const [src, setSrc] = useState("");
  useEffect(() => {
    let active = true;
    if (url.startsWith("data:image/")) {
      setSrc(url);
      return;
    }
    imageUrl(url)
      .then((s) => {
        if (active) setSrc(s);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [url]);
  return src ? (
    <img className="activity-art" src={src} alt="Media artwork" />
  ) : null;
}
export function SharedActivity({
  activity,
  peer,
}: {
  activity: any;
  peer: string;
}) {
  useExpiry();
  const entries = (activity?.friends || []).filter(
    (a: any) => a.peer === peer && a.expires * 1000 > Date.now(),
  );
  return (
    <div className="shared-activity">
      {entries.map((a: any) => {
        const b = a.activity.body;
        return (
          <div className="shared-activity-card" key={a.activity.kind}>
            {a.activity.kind === "game_activity" ? (
              <Gamepad2 size={15} />
            ) : (
              <Music2 size={15} />
            )}
            <span>
              {a.activity.kind === "game_activity"
                ? b.name
                : `${b.title} · ${b.artist || ""}`}
            </span>
            <small>Shared recently · expires</small>
          </div>
        );
      })}
    </div>
  );
}
export default function ProfileActivity({
  activity,
  profile,
  busy,
  onRun,
  onChanged,
  onSettings,
}: {
  activity: any;
  profile: any;
  busy: boolean;
  onRun: (
    key: string,
    action: () => Promise<any>,
    message?: string,
  ) => Promise<any>;
  onChanged: () => Promise<void>;
  onSettings: () => void;
}) {
  useExpiry();
  const [games, setGames] = useState<any[]>([]),
    [source, setSource] = useState("windows");
  const game =
      activity?.game?.expires * 1000 > Date.now() ? activity.game : null,
    music =
      activity?.music?.expires * 1000 > Date.now() ? activity.music : null;
  async function refreshMusic() {
    await api(
      source === "windows" ? "orbit_music_windows" : "orbit_music_spotify",
    );
    await onChanged();
  }
  return (
    <section className="profile-activity">
      <div className="profile-activity-heading">
        <h2>Activity</h2>
        <button className="text-button" onClick={onSettings}>
          Privacy settings
        </button>
      </div>
      <div className="social-two-col">
        <article className="social-panel compact-activity">
          <Gamepad2 size={20} />
          <div>
            <h3>{game?.name || "No game to share"}</h3>
            <p>
              {game
                ? "Tracked on this computer"
                : "Launch a game or check supported running games."}
            </p>
            <small>
              {profile.gameVisibility
                ? "Visible to accepted friends when shared"
                : "Private"}
            </small>
          </div>
          <button
            className="icon-btn"
            title="Detect supported running games"
            aria-label="Detect supported running games"
            disabled={busy}
            onClick={() =>
              onRun("detect", async () => {
                setGames(await api("orbit_games_detect"));
                await onChanged();
              })
            }
          >
            <RefreshCw size={14} />
          </button>
        </article>
        <article className="social-panel compact-activity">
          {music?.artwork || music?.artworkUrl ? (
            <Artwork url={music.artwork || music.artworkUrl} />
          ) : (
            <Music2 size={20} />
          )}
          <div>
            <h3>{music?.title || "Nothing playing"}</h3>
            <p>
              {music
                ? `${music.artist || ""} · ${music.playing ? "Playing" : "Paused"}`
                : "Windows session or connected Spotify"}
            </p>
            <small>
              {profile.musicVisibility
                ? "Visible to accepted friends when shared"
                : "Private"}
              {music?.source === "windows-this-device"
                ? " · This device only"
                : ""}
            </small>
          </div>
          <button
            className="icon-btn"
            title="Refresh media activity"
            aria-label="Refresh media activity"
            disabled={busy}
            onClick={() => onRun("music", refreshMusic)}
          >
            <RefreshCw size={14} />
          </button>
        </article>
      </div>
      <div className="activity-actions">
        <label>
          Media source
          <select value={source} onChange={(e) => setSource(e.target.value)}>
            <option value="windows">This Windows device</option>
            <option value="spotify">Connected Spotify</option>
          </select>
        </label>
        <button
          className="secondary"
          disabled={
            busy || (!profile.gameVisibility && !profile.musicVisibility)
          }
          onClick={() =>
            onRun(
              "share",
              () => api("orbit_activity_share"),
              "Activity queued for friends; it expires after 90 seconds.",
            )
          }
        >
          <Radio size={13} />
          Share current activity
        </button>
      </div>
      {!!games.length && (
        <details>
          <summary>Detected games</summary>
          {games.map((g) => (
            <button
              key={g.id}
              className="text-button"
              onClick={() =>
                onRun("game", async () => {
                  await api("orbit_game_select", { id: g.id });
                  await onChanged();
                })
              }
            >
              {g.name}
            </button>
          ))}
        </details>
      )}
    </section>
  );
}
