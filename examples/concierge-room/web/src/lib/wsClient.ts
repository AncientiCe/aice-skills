import type { VizEvent } from "./types";

export type Listener = (event: VizEvent) => void;
export type StatusListener = (connected: boolean) => void;

export function connectViz(
  onEvent: Listener,
  onStatus: StatusListener = () => {},
): () => void {
  let closed = false;
  let socket: WebSocket | null = null;

  const open = () => {
    if (closed) return;
    const url = `ws://${location.host}/viz`;
    socket = new WebSocket(url);
    socket.addEventListener("open", () => onStatus(true));
    socket.addEventListener("close", () => {
      onStatus(false);
      if (!closed) setTimeout(open, 1500);
    });
    socket.addEventListener("error", () => socket?.close());
    socket.addEventListener("message", (msg) => {
      try {
        const ev = JSON.parse(msg.data) as VizEvent;
        onEvent(ev);
      } catch (error) {
        console.warn("bad viz event", error);
      }
    });
  };
  open();

  return () => {
    closed = true;
    socket?.close();
  };
}
