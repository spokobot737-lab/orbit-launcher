import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { Mic, MicOff, Phone, PhoneOff } from "lucide-react";
import { api, native } from "./api";
import "./voice.css";

export function microphoneError(error: unknown) {
  const name = (error as DOMException)?.name;
  if (name === "NotAllowedError" || name === "SecurityError")
    return "Microphone permission was denied. Allow microphone access in Windows and Orbit, then try again.";
  if (name === "NotFoundError" || name === "OverconstrainedError")
    return "The selected microphone is unavailable. Connect or choose another microphone.";
  return "The microphone could not start. Check your device and try again.";
}

type SignalType =
  "invite" | "accept" | "decline" | "offer" | "answer" | "ice" | "end";
type Signal = {
  peer: string;
  call: string;
  type: SignalType;
  data: unknown;
  expires: number;
};
type Friend = { id: string; name: string };
type Phase =
  "incoming" | "calling" | "connecting" | "connected" | "reconnecting";
type Call = {
  id: string;
  peer: string;
  name: string;
  outgoing: boolean;
  phase: Phase;
  pc?: RTCPeerConnection;
  stream?: MediaStream;
  timer?: number;
  iceTimer?: number;
  remoteIce: RTCIceCandidateInit[];
  localIce: RTCIceCandidateInit[];
  muted: boolean;
  ready: boolean;
  changing?: boolean;
};
type Voice = {
  start: (friend: Friend) => void;
  active: boolean;
  messagePeer: string;
  device: string;
};
const Context = createContext<Voice>({
  start: () => {},
  active: false,
  messagePeer: "",
  device: "",
});
export const useVoice = () => useContext(Context);

export function VoiceProvider({ children }: { children: ReactNode }) {
  const current = useRef<Call | undefined>(undefined);
  const ended = useRef(new Map<string, number>());
  const audio = useRef<HTMLAudioElement>(null);
  const [call, show] = useState<Call>();
  const [failure, setFailure] = useState<{ peer: string; message: string }>();
  const [messagePeer, setMessagePeer] = useState("");
  const [device, setDevice] = useState("");
  const [devices, setDevices] = useState<MediaDeviceInfo[]>([]);
  const [volume, setVolume] = useState(0.8);
  const [closing, setClosing] = useState(false);
  const [ready, setReady] = useState(false);
  const send = (c: Call, type: SignalType, data?: unknown) =>
    api("orbit_voice_send", { peer: c.peer, call: c.id, type, data });
  const live = (c: Call) => current.current === c;
  const update = (c: Call, phase: Phase) => {
    if (live(c)) {
      c.phase = phase;
      show({ ...c });
    }
  };
  function remember(peer: string, id: string) {
    const now = Date.now();
    for (const [key, until] of ended.current)
      if (until <= now) ended.current.delete(key);
    if (ended.current.size >= 2048)
      ended.current.delete(ended.current.keys().next().value!);
    ended.current.set(peer + id, now + 70_000);
  }
  function finish(c: Call, message?: string) {
    if (!live(c)) return;
    remember(c.peer, c.id);
    current.current = undefined;
    window.clearTimeout(c.timer);
    window.clearTimeout(c.iceTimer);
    c.remoteIce.length = c.localIce.length = 0;
    c.pc?.close();
    c.stream?.getTracks().forEach((t) => t.stop());
    if (audio.current) audio.current.srcObject = null;
    show(undefined);
    setClosing(false);
    void api("orbit_voice_active", { active: false }).catch(() => {});
    if (message) setFailure({ peer: c.peer, message });
  }
  function hangup(c: Call, message?: string, type: SignalType = "end") {
    finish(c, message);
    return send(c, type).catch(() => {});
  }
  function timeout(c: Call, ms: number, message: string) {
    window.clearTimeout(c.timer);
    c.timer = window.setTimeout(() => {
      if (live(c)) void hangup(c, message);
    }, ms);
  }
  function begin(peer: string, name: string, id: string, outgoing: boolean) {
    const c: Call = {
      id,
      peer,
      name,
      outgoing,
      phase: outgoing ? "calling" : "incoming",
      remoteIce: [],
      localIce: [],
      muted: false,
      ready: false,
    };
    current.current = c;
    setFailure(undefined);
    setMessagePeer("");
    show({ ...c });
    void api("orbit_voice_active", { active: true }).catch(() => {});
    return c;
  }
  async function refreshDevices() {
    try {
      setDevices(
        (await navigator.mediaDevices.enumerateDevices()).filter(
          (d) => d.kind === "audioinput",
        ),
      );
    } catch {
      setDevices([]);
    }
  }
  function watchTrack(c: Call, track: MediaStreamTrack) {
    track.onended = () => {
      if (live(c) && c.stream?.getAudioTracks()[0] === track)
        void hangup(
          c,
          "The microphone disconnected. Choose a microphone and call again.",
        );
    };
  }
  function flushIce(c: Call) {
    window.clearTimeout(c.iceTimer);
    if (!live(c) || !c.ready || !c.localIce.length) return;
    const batch = c.localIce.splice(0, 12);
    void send(c, "ice", batch).catch(() => {});
    if (c.localIce.length)
      c.iceTimer = window.setTimeout(() => flushIce(c), 150);
  }
  async function setup(c: Call, microphone: string) {
    let stream: MediaStream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: microphone ? { deviceId: { exact: microphone } } : true,
      });
    } catch (error) {
      if (live(c)) void hangup(c, microphoneError(error));
      return;
    }
    if (!live(c)) {
      stream.getTracks().forEach((t) => t.stop());
      return;
    }
    c.stream = stream;
    const track = stream.getAudioTracks()[0];
    watchTrack(c, track);
    void refreshDevices();
    const pc = new RTCPeerConnection({
      iceServers: [{ urls: "stun:stun.cloudflare.com:3478" }],
    });
    c.pc = pc;
    pc.addTrack(track, stream);
    pc.ontrack = (event) => {
      if (live(c) && audio.current) {
        audio.current.srcObject = new MediaStream([event.track]);
        void audio.current.play().catch(() => {
          if (live(c))
            setFailure({
              peer: c.peer,
              message: "Audio playback is paused. Click Play call audio.",
            });
        });
      }
    };
    pc.onicecandidate = ({ candidate }) => {
      if (!live(c) || !candidate || c.localIce.length >= 96) return;
      c.localIce.push(candidate.toJSON());
      if (c.ready) {
        window.clearTimeout(c.iceTimer);
        c.iceTimer = window.setTimeout(() => flushIce(c), 120);
      }
    };
    pc.onconnectionstatechange = () => {
      if (!live(c)) return;
      if (pc.connectionState === "connected") {
        window.clearTimeout(c.timer);
        update(c, "connected");
      } else if (pc.connectionState === "disconnected") {
        update(c, "reconnecting");
        timeout(
          c,
          12_000,
          "The direct audio connection was lost. Call again or send a voice message.",
        );
      } else if (pc.connectionState === "failed") {
        void hangup(
          c,
          "A direct connection could not be made. Your networks may require a relay. Send a voice message instead.",
        );
      }
    };
    return pc;
  }
  async function applyIce(c: Call) {
    if (!c.pc?.remoteDescription) return;
    for (const candidate of c.remoteIce.splice(0)) {
      if (!live(c)) return;
      await c.pc.addIceCandidate(candidate).catch(() => {});
    }
  }
  async function description(c: Call, type: "offer" | "answer") {
    const pc = c.pc!;
    const desc =
      type === "offer" ? await pc.createOffer() : await pc.createAnswer();
    if (!live(c)) return;
    await pc.setLocalDescription(desc);
    if (!live(c)) return;
    await send(c, type, desc.sdp);
    c.ready = true;
    flushIce(c);
  }
  const incoming = useRef<(s: Signal) => Promise<void>>(async () => {});
  incoming.current = async (signal) => {
    if (signal.expires * 1000 <= Date.now()) return;
    let c = current.current;
    if (signal.type === "invite") {
      if (ended.current.get(signal.peer + signal.call)! > Date.now()) return;
      if (c?.id === signal.call && c.peer === signal.peer) return;
      if (c) {
        if (
          c.peer !== signal.peer ||
          c.phase !== "calling" ||
          c.id < signal.call
        ) {
          remember(signal.peer, signal.call);
          return;
        }
        void hangup(c);
      }
      let name = signal.peer.slice(0, 12);
      try {
        const friends = await api<Friend[]>("orbit_contacts");
        name = friends.find((f) => f.id === signal.peer)?.name || name;
      } catch {
        /* use the authenticated key if the local name is unavailable */
      }
      if (
        current.current ||
        signal.expires * 1000 <= Date.now() ||
        ended.current.get(signal.peer + signal.call)! > Date.now()
      )
        return;
      c = begin(signal.peer, name, signal.call, false);
      timeout(
        c,
        Math.min(30_000, signal.expires * 1000 - Date.now()),
        "Missed call.",
      );
      return;
    }
    if (signal.type === "end" || signal.type === "decline")
      remember(signal.peer, signal.call);
    if (!c || c.id !== signal.call || c.peer !== signal.peer) return;
    if (
      (signal.type === "offer" || signal.type === "answer") &&
      (typeof signal.data !== "string" ||
        signal.data.match(/^m=audio /gm)?.length !== 1 ||
        /^m=(?!audio )/m.test(signal.data))
    ) {
      void hangup(c, "This call requires an audio-only connection.");
      return;
    }
    try {
      if (signal.type === "end" || signal.type === "decline") {
        finish(
          c,
          signal.type === "decline"
            ? "Your friend declined the call."
            : undefined,
        );
      } else if (
        signal.type === "accept" &&
        c.outgoing &&
        c.phase === "calling"
      ) {
        update(c, "connecting");
        timeout(
          c,
          30_000,
          "The call could not connect. Send a voice message instead.",
        );
        // The outgoing microphone was opened on the Call click.
        if (c.pc) await description(c, "offer");
      } else if (signal.type === "ice" && c.phase !== "incoming") {
        if (Array.isArray(signal.data))
          c.remoteIce.push(...signal.data.slice(0, 96 - c.remoteIce.length));
        await applyIce(c);
      } else if (
        signal.type === "offer" &&
        !c.outgoing &&
        c.phase === "connecting" &&
        c.pc?.signalingState === "stable"
      ) {
        await c.pc.setRemoteDescription({
          type: "offer",
          sdp: signal.data as string,
        });
        await applyIce(c);
        if (live(c)) await description(c, "answer");
      } else if (
        signal.type === "answer" &&
        c.outgoing &&
        c.pc?.signalingState === "have-local-offer"
      ) {
        await c.pc.setRemoteDescription({
          type: "answer",
          sdp: signal.data as string,
        });
        await applyIce(c);
      }
    } catch {
      if (live(c))
        void hangup(
          c,
          "The call could not connect. Send a voice message instead.",
        );
    }
  };
  async function start(friend: Friend) {
    if (!native || !ready || current.current) return;
    const c = begin(friend.id, friend.name, crypto.randomUUID(), true);
    timeout(c, 30_000, "No answer. Send a voice message instead.");
    try {
      const pc = await setup(c, device);
      if (pc && live(c)) await send(c, "invite");
    } catch {
      if (live(c))
        void hangup(
          c,
          "Could not place the call. Check your relay connection and try again.",
        );
    }
  }
  async function answer(c: Call) {
    if (!live(c) || c.phase !== "incoming") return;
    update(c, "connecting");
    timeout(
      c,
      30_000,
      "The call could not connect. Send a voice message instead.",
    );
    try {
      if ((await setup(c, device)) && live(c)) await send(c, "accept");
    } catch {
      if (live(c))
        void hangup(
          c,
          "Could not answer the call. Check your relay connection.",
        );
    }
  }
  async function changeDevice(value: string) {
    const c = current.current;
    if (!c?.pc || !c.stream) {
      setDevice(value);
      return;
    }
    if (c.changing) return;
    c.changing = true;
    let replacement: MediaStream | undefined;
    try {
      replacement = await navigator.mediaDevices.getUserMedia({
        audio: value ? { deviceId: { exact: value } } : true,
      });
      if (!live(c)) return;
      const track = replacement.getAudioTracks()[0];
      track.enabled = !c.muted;
      const sender = c.pc.getSenders().find((s) => s.track?.kind === "audio");
      if (!sender) return;
      await sender.replaceTrack(track);
      if (!live(c)) return;
      const previous = c.stream;
      c.stream = replacement;
      replacement = undefined;
      watchTrack(c, track);
      previous.getTracks().forEach((t) => t.stop());
      setDevice(value);
      void refreshDevices();
    } catch (error) {
      if (live(c))
        setFailure({ peer: c.peer, message: microphoneError(error) });
    } finally {
      replacement?.getTracks().forEach((t) => t.stop());
      c.changing = false;
    }
  }
  const terminate = useRef<(message?: string) => void>(() => {});
  terminate.current = (message) => {
    if (current.current) void hangup(current.current, message);
  };
  useEffect(() => {
    if (!native) return;
    let cancelled = false;
    // Process offers/answers in order. ICE and terminal signals can be handled
    // immediately, including cancellation that arrives during device setup.
    let descriptions = Promise.resolve();
    const signal = listen<Signal>("voice-signal", ({ payload }) => {
      if (payload.type === "offer" || payload.type === "answer") {
        descriptions = descriptions
          .then(() => incoming.current?.(payload))
          .catch(() => {});
      } else void incoming.current?.(payload).catch(() => {});
    });
    void signal
      .then(() => {
        if (!cancelled) setReady(true);
      })
      .catch(() => {});
    const close = listen("voice-close", () => setClosing(true));
    const offline = () =>
      terminate.current?.("The network disconnected. Call again when online.");
    window.addEventListener("offline", offline);
    const devicesChanged = () => void refreshDevices();
    navigator.mediaDevices?.addEventListener("devicechange", devicesChanged);
    void refreshDevices();
    let checking = false;
    const check = async () => {
      const c = current.current;
      if (!c || checking) return;
      checking = true;
      try {
        if (
          !(await api<boolean>("orbit_voice_health", { peer: c.peer })) &&
          current.current === c
        )
          terminate.current?.(
            "This call ended because Orbit is offline or the contact is no longer a friend.",
          );
      } catch {
        if (current.current === c)
          terminate.current?.("The contact is no longer available.");
      } finally {
        checking = false;
      }
    };
    const health = window.setInterval(check, 3000);
    const changes = listen("social-changed", check);
    return () => {
      cancelled = true;
      window.removeEventListener("offline", offline);
      navigator.mediaDevices?.removeEventListener(
        "devicechange",
        devicesChanged,
      );
      window.clearInterval(health);
      for (const sub of [signal, close, changes])
        void sub.then((f) => f()).catch(() => {});
      terminate.current?.();
    };
  }, []);
  useEffect(() => {
    if (audio.current) audio.current.volume = volume;
  }, [volume]);
  return (
    <Context.Provider
      value={{ start, active: !!call || !ready, messagePeer, device }}
    >
      {children}
      <audio ref={audio} autoPlay />
      {(call || failure) && (
        <aside className="orbit-call" aria-label="Voice call">
          {call && (
            <div className="orbit-call-controls">
              <Phone size={17} />
              <div>
                <strong>{call.name}</strong>
                <small>
                  {
                    {
                      incoming: "Incoming call",
                      calling: "Calling…",
                      connecting: "Connecting…",
                      connected: "Connected",
                      reconnecting: "Reconnecting…",
                    }[call.phase]
                  }
                </small>
              </div>
              <select
                aria-label="Call microphone"
                value={device}
                onChange={(e) => void changeDevice(e.target.value)}
              >
                <option value="">Default microphone</option>
                {devices
                  .filter((d) => d.deviceId && d.deviceId !== "default")
                  .map((d, i) => (
                    <option value={d.deviceId} key={d.deviceId}>
                      {d.label || `Microphone ${i + 1}`}
                    </option>
                  ))}
              </select>
              {call.phase === "incoming" ? (
                <>
                  <button
                    className="primary"
                    onClick={() =>
                      current.current && void answer(current.current)
                    }
                  >
                    Answer
                  </button>
                  <button
                    className="secondary"
                    onClick={() =>
                      current.current &&
                      void hangup(current.current, undefined, "decline")
                    }
                  >
                    Decline
                  </button>
                </>
              ) : (
                <>
                  <button
                    className="icon-btn"
                    title={call.muted ? "Unmute" : "Mute"}
                    aria-pressed={call.muted}
                    onClick={() => {
                      const c = current.current;
                      if (!c?.stream) return;
                      c.muted = !c.muted;
                      c.stream.getAudioTracks().forEach((t) => {
                        t.enabled = !c.muted;
                      });
                      show({ ...c });
                    }}
                  >
                    {call.muted ? <MicOff size={17} /> : <Mic size={17} />}
                  </button>
                  <label className="orbit-call-volume">
                    Volume{" "}
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step=".05"
                      value={volume}
                      onChange={(e) => setVolume(+e.target.value)}
                    />
                  </label>
                  <button
                    className="secondary danger"
                    onClick={() =>
                      current.current && void hangup(current.current)
                    }
                  >
                    <PhoneOff size={16} /> Hang up
                  </button>
                </>
              )}
            </div>
          )}
          {failure && (
            <div className="orbit-call-controls" role="status">
              <span>{failure.message}</span>
              {call ? (
                <button
                  className="secondary"
                  onClick={() =>
                    void audio.current
                      ?.play()
                      .then(() => setFailure(undefined))
                      .catch(() => {})
                  }
                >
                  Play call audio
                </button>
              ) : (
                <button
                  className="secondary"
                  onClick={() => {
                    setMessagePeer(failure.peer);
                    window.dispatchEvent(
                      new CustomEvent("orbit-voice-message", {
                        detail: failure.peer,
                      }),
                    );
                    setFailure(undefined);
                  }}
                >
                  Send a voice message
                </button>
              )}
              <button
                className="text-button"
                onClick={() => setFailure(undefined)}
              >
                Dismiss
              </button>
            </div>
          )}
        </aside>
      )}
      {closing && call && (
        <div className="modal-backdrop">
          <div className="modal" role="dialog" aria-modal="true">
            <h2>Exit Orbit?</h2>
            <p>Exiting Orbit ends your voice call.</p>
            <div className="button-row">
              <button className="secondary" onClick={() => setClosing(false)}>
                Keep call
              </button>
              <button
                className="primary"
                onClick={() => {
                  const c = current.current;
                  const end = c ? hangup(c) : Promise.resolve();
                  void Promise.race([
                    end,
                    new Promise((resolve) => window.setTimeout(resolve, 1200)),
                  ]).then(() => api("orbit_voice_exit"));
                }}
              >
                Hang up and exit
              </button>
            </div>
          </div>
        </div>
      )}
    </Context.Provider>
  );
}
