import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Camera, Check, Copy, Link2, Pencil, RefreshCw, X } from "lucide-react";
import { api, errorText } from "./api";
export type OrbitProfile = {
  id: string;
  npub: string;
  name: string;
  bio: string;
  avatar?: string;
  banner?: string;
  revision?: number;
  gameVisibility: boolean;
  musicVisibility: boolean;
};
export default function ProfileCard({
  profile,
  connections,
  count,
  sync,
  onSave,
  onConnections,
  onFriends,
  onRetry,
}: {
  profile: OrbitProfile;
  connections: any;
  count: number;
  sync: any;
  onSave: (p: OrbitProfile) => Promise<OrbitProfile>;
  onConnections: () => void;
  onFriends: () => void;
  onRetry: () => void;
}) {
  const [draft, setDraft] = useState(profile),
    [editing, setEditing] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    if (!editing) setDraft(profile);
  }, [profile, editing]);
  const shown = editing ? draft : profile;
  async function choose(banner: boolean) {
    setError("");
    setBusy(true);
    if (!editing) {
      setDraft(profile);
      setEditing(true);
    }
    try {
      const path = await open({
        multiple: false,
        filters: [
          {
            name: "Profile images",
            extensions: ["jpg", "jpeg", "png", "webp"],
          },
        ],
      });
      if (typeof path === "string") {
        const image = await api<string>("orbit_profile_image", {
          path,
          banner,
        });
        setDraft((current) => ({
          ...current,
          [banner ? "banner" : "avatar"]: image,
        }));
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    setBusy(true);
    setError("");
    try {
      await onSave(draft);
      setEditing(false);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="profile-hero" aria-label="Your profile">
      <div
        className="profile-banner"
        style={
          shown.banner ? { backgroundImage: `url(${shown.banner})` } : undefined
        }
      >
        <div className="profile-orbit-art" aria-hidden="true" />
        <button
          className="banner-hit"
          aria-label="Choose banner image"
          disabled={busy}
          onClick={() => choose(true)}
        >
          <span>
            <Camera size={14} /> Change banner
          </span>
        </button>
      </div>
      <div className="profile-overlap">
        <button
          className="avatar-picker"
          aria-label="Choose avatar image"
          disabled={busy}
          onClick={() => choose(false)}
        >
          <span className="social-avatar large">
            {shown.avatar ? (
              <img src={shown.avatar} alt="Your avatar" />
            ) : (
              shown.name.slice(0, 1)
            )}
          </span>
          <span className="avatar-edit">
            <Camera size={13} />
          </span>
        </button>
        <div className="profile-actions">
          {editing ? (
            <>
              <button className="primary" disabled={busy} onClick={save}>
                <Check size={14} />
                {busy ? "Saving…" : "Save"}
              </button>
              <button
                className="secondary"
                disabled={busy}
                onClick={() => {
                  setDraft(profile);
                  setEditing(false);
                  setError("");
                }}
              >
                <X size={14} />
                Cancel
              </button>
            </>
          ) : (
            <>
              <button
                className="secondary"
                onClick={() => {
                  setDraft(profile);
                  setEditing(true);
                }}
              >
                <Pencil size={14} />
                Edit profile
              </button>
              <button className="secondary" onClick={onConnections}>
                <Link2 size={14} />
                Connections
              </button>
            </>
          )}
        </div>
      </div>
      <div className="profile-identity">
        {editing ? (
          <input
            className="profile-name-input"
            aria-label="Display name"
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            required
            maxLength={60}
          />
        ) : (
          <h1>{profile.name}</h1>
        )}
        {editing ? (
          <textarea
            className="profile-bio-input"
            aria-label="Bio"
            rows={3}
            value={draft.bio}
            onChange={(e) => setDraft({ ...draft, bio: e.target.value })}
            maxLength={2000}
          />
        ) : (
          <p className="profile-bio">
            {profile.bio || "Add a little about yourself."}
          </p>
        )}
        {editing && (
          <small className="muted">
            Save updates your profile for accepted friends. Cancel discards this
            draft.
          </small>
        )}
        {error && (
          <p className="alert error" role="alert">
            {error}
          </p>
        )}
        <div className="profile-links">
          {Object.entries(connections).map(
            ([provider, value]: [string, any]) => (
              <button
                key={provider}
                className="text-button"
                title={
                  value.authenticated
                    ? "Authenticated on this device"
                    : "Manual profile link"
                }
                onClick={() =>
                  api("orbit_connection_open", { provider }).catch((e) =>
                    setError(errorText(e)),
                  )
                }
              >
                {provider} · {value.name}
              </button>
            ),
          )}
        </div>
        <button
          className="identity-copy"
          title="Copy public Orbit identity"
          onClick={() =>
            navigator.clipboard
              .writeText(profile.npub)
              .catch((e) => setError(errorText(e)))
          }
        >
          <Copy size={12} />
          {profile.npub.slice(0, 18)}…
        </button>
      </div>
      <div className="profile-counts">
        <button onClick={onFriends}>
          <strong>{count}</strong>
          <span>Friends</span>
        </button>
        <button onClick={onConnections}>
          <strong>{Object.keys(connections).length}</strong>
          <span>Connections</span>
        </button>
        <span className={`profile-sync ${sync?.state || "pending"}`}>
          {sync?.label || "Saved locally"}
          {sync?.state === "failed" && (
            <button className="text-button" onClick={onRetry}>
              <RefreshCw size={12} />
              Retry
            </button>
          )}
        </span>
      </div>
    </section>
  );
}
