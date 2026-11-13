import { useEffect, useRef, useState } from "react";
import { Mic, Square } from "lucide-react";
import { api, errorText, native } from "./api";
import { microphoneError, useVoice } from "./Voice";

const LIMIT = 120_000;
const MAX_BYTES = 4 * 1024 * 1024;
const FORMATS = [
  ["audio/webm;codecs=opus", "webm"],
  ["audio/ogg;codecs=opus", "ogg"],
  ["audio/mp4", "m4a"],
];
type Capture = {
  stream: MediaStream;
  recorder: MediaRecorder;
  timer: number;
  interval: number;
};

function encode(clip: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Could not read the recording."));
    reader.onload = () => resolve(String(reader.result).split(",", 2)[1]);
    reader.readAsDataURL(clip);
  });
}

export default function VoiceRecorder({
  peer,
  onSent,
  onError,
}: {
  peer: string;
  onSent: () => Promise<void>;
  onError: (message: string) => void;
}) {
  const { device, messagePeer } = useVoice();
  const mounted = useRef(true);
  const capture = useRef<Capture | undefined>(undefined);
  const draft = useRef("");
  const url = useRef("");
  const inFlight = useRef(false);
  const [opening, setOpening] = useState(false);
  const [elapsed, setElapsed] = useState<number>();
  const [preview, setPreview] = useState<{
    blob: Blob;
    format: string;
    url: string;
  }>();
  const [sending, setSending] = useState(false);
  const [failed, setFailed] = useState(false);
  const micButton = useRef<HTMLButtonElement>(null);

  function release(c: Capture) {
    window.clearTimeout(c.timer);
    window.clearInterval(c.interval);
    c.stream.getTracks().forEach((t) => t.stop());
    if (capture.current === c) capture.current = undefined;
  }
  function stop() {
    const c = capture.current;
    if (!c) return;
    if (c.recorder.state !== "inactive") c.recorder.stop();
    release(c);
  }
  async function discard() {
    if (draft.current) {
      await api("orbit_voice_discard", { id: draft.current });
      draft.current = "";
    }
    URL.revokeObjectURL(url.current);
    url.current = "";
    if (mounted.current) {
      setPreview(undefined);
      setFailed(false);
    }
  }
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      stop();
      URL.revokeObjectURL(url.current);
      if (!inFlight.current && draft.current)
        void api("orbit_voice_discard", { id: draft.current }).catch(() => {});
    };
  }, []);
  useEffect(() => {
    if (messagePeer === peer) micButton.current?.focus();
  }, [messagePeer, peer]);

  async function start() {
    if (opening || capture.current || preview) return;
    const player = document.createElement("audio");
    const format = FORMATS.find(
      ([mime]) =>
        typeof MediaRecorder !== "undefined" &&
        MediaRecorder.isTypeSupported(mime) &&
        player.canPlayType(mime),
    );
    if (!format) {
      onError(
        "This WebView2 runtime cannot record and play supported audio. Update WebView2 and try again.",
      );
      return;
    }
    setOpening(true);
    let stream: MediaStream | undefined;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: device ? { deviceId: { exact: device } } : true,
      });
      if (!mounted.current) return;
      const recorder = new MediaRecorder(stream, {
        mimeType: format[0],
        audioBitsPerSecond: 64_000,
      });
      const parts: Blob[] = [];
      let bytes = 0;
      let invalid = false;
      const began = performance.now();
      const c = { stream, recorder, timer: 0, interval: 0 };
      capture.current = c;
      recorder.ondataavailable = (event) => {
        bytes += event.data.size;
        if (bytes <= MAX_BYTES) parts.push(event.data);
        else {
          invalid = true;
          stop();
        }
      };
      recorder.onerror = () => {
        invalid = true;
        stop();
        if (mounted.current) {
          setElapsed(undefined);
          onError(
            "Recording stopped because the microphone could not be read.",
          );
        }
      };
      recorder.onstop = () => {
        release(c);
        if (!mounted.current) return;
        setElapsed(undefined);
        if (invalid || !bytes) {
          onError(
            bytes > MAX_BYTES
              ? "The voice clip reached its size limit."
              : "No audio was recorded.",
          );
          return;
        }
        const blob = new Blob(parts, { type: format[0] });
        url.current = URL.createObjectURL(blob);
        setPreview({ blob, url: url.current, format: format[1] });
      };
      stream.getAudioTracks().forEach((t) => {
        t.onended = () => {
          if (capture.current === c) {
            stop();
            if (mounted.current)
              onError(
                "The microphone disconnected. Preview the audio recorded so far.",
              );
          }
        };
      });
      recorder.start(250);
      stream = undefined;
      c.timer = window.setTimeout(stop, LIMIT);
      c.interval = window.setInterval(
        () =>
          setElapsed(
            Math.min(120, Math.floor((performance.now() - began) / 1000)),
          ),
        250,
      );
      setElapsed(0);
    } catch (error) {
      if (capture.current) release(capture.current);
      if (mounted.current) onError(microphoneError(error));
    } finally {
      stream?.getTracks().forEach((t) => t.stop());
      if (mounted.current) setOpening(false);
    }
  }

  async function send() {
    if (!preview || inFlight.current) return;
    inFlight.current = true;
    setSending(true);
    setFailed(false);
    try {
      if (!draft.current) {
        const result = await api<{ id: string }>("orbit_voice_prepare", {
          audio: await encode(preview.blob),
          format: preview.format,
        });
        draft.current = result.id;
      }
      if (!mounted.current) return;
      const file = draft.current;
      await api("orbit_media_upload", { id: file });
      if (!mounted.current) return;
      await api("orbit_send", { peer, text: "", attachments: [file] });
      draft.current = "";
      await discard();
      if (mounted.current) await onSent();
    } catch (error) {
      if (mounted.current) {
        onError(errorText(error));
        setFailed(true);
      }
    } finally {
      inFlight.current = false;
      if (mounted.current) setSending(false);
      else if (draft.current)
        void api("orbit_voice_discard", { id: draft.current }).catch(() => {});
    }
  }

  if (!native) return null;
  return (
    <div className="voice-recorder">
      {elapsed !== undefined ? (
        <>
          <button className="secondary" onClick={stop}>
            <Square size={15} /> Stop recording
          </button>
          <span role="timer">
            {Math.floor(elapsed / 60)}:{String(elapsed % 60).padStart(2, "0")} /
            2:00
          </span>
          <button
            className="text-button"
            onClick={() => {
              const c = capture.current;
              if (c) {
                c.recorder.onstop = null;
                stop();
              }
              setElapsed(undefined);
            }}
          >
            Discard
          </button>
        </>
      ) : preview ? (
        <>
          <audio
            src={preview.url}
            controls
            preload="metadata"
            aria-label="Voice message preview"
          />
          <button
            className="primary"
            disabled={sending}
            onClick={() => void send()}
          >
            {sending
              ? "Sending voice message…"
              : failed
                ? "Retry voice message"
                : "Send voice message"}
          </button>
          <button
            className="text-button"
            disabled={sending}
            onClick={() => void discard().catch((e) => onError(errorText(e)))}
          >
            Discard
          </button>
        </>
      ) : (
        <button
          ref={micButton}
          className="icon-btn"
          title="Record voice message"
          aria-label="Record voice message"
          disabled={opening}
          onClick={() => void start()}
        >
          <Mic size={19} />
        </button>
      )}
    </div>
  );
}
