import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
export default function HostingClose() {
  const [show, setShow] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    const un = listen("hosting-close", () => setShow(true));
    return () => {
      un.then((f) => f());
    };
  }, []);
  if (!show) return null;
  return (
    <div className="modal-backdrop">
      <section
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Stop hosting before exiting"
      >
        <h2>Your server is still running</h2>
        <p>
          Closing Orbit must save and stop the server. To keep hosting, minimize
          Orbit instead. Your computer must stay powered on, connected and
          awake.
        </p>
        {error && <p className="alert error">{error}</p>}
        <div className="button-row">
          <button
            disabled={busy}
            className="primary"
            onClick={async () => {
              setBusy(true);
              try {
                await api("server_exit");
              } catch (e) {
                setError(errorText(e));
                setBusy(false);
              }
            }}
          >
            {busy ? "Saving and stopping…" : "Save and stop, then exit"}
          </button>
          <button
            disabled={busy}
            className="secondary"
            onClick={async () => {
              try {
                await api("server_minimize");
                setShow(false);
              } catch (e) {
                setError(errorText(e));
              }
            }}
          >
            Keep hosting · minimize
          </button>
          <button
            disabled={busy}
            className="secondary"
            onClick={() => setShow(false)}
          >
            Cancel
          </button>
        </div>
      </section>
    </div>
  );
}
