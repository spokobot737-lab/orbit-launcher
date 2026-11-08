import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { api, errorText } from "./api";
export default function NetworkSettings({
  config,
  status,
  onSave,
  onReconnect,
}: {
  config: any;
  status: any;
  onSave: (v: any) => Promise<any>;
  onReconnect: () => Promise<any>;
}) {
  const [draft, setDraft] = useState(config),
    [relays, setRelays] = useState((config.relays || []).join("\n")),
    [advanced, setAdvanced] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    setDraft(config);
    setRelays((config.relays || []).join("\n"));
  }, [JSON.stringify(config)]);
  async function save(v: any) {
    setBusy(true);
    setError("");
    try {
      await onSave(v);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="social-panel network-settings">
      <div className="panel-heading">
        <div>
          <h2>Network</h2>
          <p>{status.phase || "Connecting"}</p>
        </div>
        <span className={`network-badge ${(status.phase || "").toLowerCase()}`}>
          {status.phase || "Connecting"}
        </span>
      </div>
      <p>
        Automatic connections are ready for invitations, messages and supported
        files. Your launcher keeps working during outages.
      </p>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={config.enabled !== true}
          disabled={busy}
          onChange={(e) => save({ ...config, enabled: !e.target.checked })}
        />
        Offline — pause all social connections and uploads
      </label>
      <label>
        Connection mode
        <select
          value={draft.mode || "custom"}
          disabled={busy}
          onChange={(e) => {
            const mode = e.target.value;
            setDraft({ ...draft, mode });
            if (mode === "automatic")
              void save({ ...config, mode, enabled: config.enabled });
            else setAdvanced(true);
          }}
        >
          <option value="automatic">Automatic (recommended)</option>
          <option value="custom">Custom</option>
        </select>
      </label>
      <p className="muted">
        External operators: {(status.operators?.relays || []).join(", ")} for
        encrypted messages; {(status.operators?.storage || []).join(", ")} for
        encrypted files. Requests go directly to these services. Retention and
        limits can change.
      </p>
      <p className="small-warning">
        Automatic attachments were tested up to 25 MiB per original file. This
        is a tested local limit, not a permanent free quota or a promise of
        larger uploads.
      </p>
      <button
        className="text-button"
        aria-expanded={advanced}
        onClick={() => setAdvanced(!advanced)}
      >
        Advanced {advanced ? "▴" : "▾"}
      </button>
      {advanced && (
        <div className="network-advanced">
          <label>
            Custom inbox endpoints (one per line)
            <textarea
              rows={3}
              value={relays}
              onChange={(e) => setRelays(e.target.value)}
              disabled={draft.mode !== "custom"}
            />
          </label>
          <label>
            Custom Blossom storage endpoint
            <input
              value={draft.mediaServer || ""}
              onChange={(e) =>
                setDraft({ ...draft, mediaServer: e.target.value })
              }
              disabled={draft.mode !== "custom"}
            />
          </label>
          <label>
            Local file limit (MiB)
            <input
              type="number"
              min={1}
              max={2048}
              value={Math.round((draft.maxFileBytes || 26214400) / 1048576)}
              onChange={(e) =>
                setDraft({ ...draft, maxFileBytes: +e.target.value * 1048576 })
              }
              disabled={draft.mode !== "custom"}
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={!!draft.mediaConsent}
              onChange={(e) =>
                setDraft({ ...draft, mediaConsent: e.target.checked })
              }
              disabled={draft.mode !== "custom"}
            />
            Allow encrypted uploads when sending to the custom service
          </label>
          <div className="button-row">
            <button
              className="primary"
              disabled={busy || draft.mode !== "custom"}
              onClick={() =>
                save({
                  ...draft,
                  enabled: config.enabled,
                  mode: "custom",
                  relays: relays
                    .split("\n")
                    .map((s: string) => s.trim())
                    .filter(Boolean),
                })
              }
            >
              Save custom configuration
            </button>
            <button
              className="secondary"
              disabled={busy}
              onClick={() =>
                save({ ...config, mode: "automatic", restoreDefaults: true })
              }
            >
              Restore defaults
            </button>
            <button
              className="secondary"
              disabled={busy || !config.enabled}
              onClick={() => onReconnect().catch((e) => setError(errorText(e)))}
            >
              <RefreshCw size={13} />
              Reconnect
            </button>
          </div>
          <details>
            <summary>Connection diagnostics</summary>
            <pre>
              {JSON.stringify(
                {
                  connections: status.diagnostics,
                  lastAcknowledgment: status.lastAcknowledgment,
                },
                null,
                2,
              )}
            </pre>
          </details>
          <button
            className="text-button"
            disabled={busy}
            onClick={() =>
              api("orbit_media_clear_cache").catch((e) =>
                setError(errorText(e)),
              )
            }
          >
            Clear downloaded media cache
          </button>
          <small>
            Removes local previews and staged copies. Saved profile images,
            messages and remote copies remain.
          </small>
        </div>
      )}
      {error && (
        <p className="alert error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
