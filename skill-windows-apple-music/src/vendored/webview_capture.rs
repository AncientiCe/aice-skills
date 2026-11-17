use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use wry::{
    application::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    },
    webview::{WebContext, WebViewBuilder},
};

use crate::session::SessionCapture;

const CAPTURE_SCRIPT: &str = r#"
(() => {
  if (window.__am_capture_installed) return;
  window.__am_capture_installed = true;
  window.__am_capture = {
    developerToken: "",
    mediaUserToken: "",
    cookieHeader: "",
    storefront: "",
    lastSeenAt: Date.now()
  };

  const post = (kind, payload) => {
    try {
      window.ipc.postMessage(JSON.stringify({ kind, payload }));
    } catch (_) {}
  };

  const trimBearer = (value) => {
    if (!value) return "";
    return String(value).replace(/^Bearer\s+/i, "").trim();
  };

  const updateFromHeaders = (headers) => {
    try {
      if (!headers) return;
      const auth = headers.get ? headers.get("authorization") : (headers["authorization"] || headers["Authorization"]);
      const media = headers.get ? headers.get("media-user-token") : (headers["media-user-token"] || headers["Media-User-Token"]);
      if (auth) window.__am_capture.developerToken = trimBearer(auth);
      if (media) window.__am_capture.mediaUserToken = String(media).trim();
      window.__am_capture.lastSeenAt = Date.now();
    } catch (_) {}
  };

  const wrappedFetch = window.fetch.bind(window);
  window.fetch = async (...args) => {
    try {
      const init = args[1] || {};
      updateFromHeaders(init.headers || {});
    } catch (_) {}
    return wrappedFetch(...args);
  };

  const originalOpen = XMLHttpRequest.prototype.open;
  const originalSetHeader = XMLHttpRequest.prototype.setRequestHeader;
  const originalSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url, ...rest) {
    this.__am_headers = {};
    return originalOpen.call(this, method, url, ...rest);
  };
  XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
    this.__am_headers = this.__am_headers || {};
    this.__am_headers[name] = value;
    return originalSetHeader.call(this, name, value);
  };
  XMLHttpRequest.prototype.send = function(body) {
    try {
      updateFromHeaders(this.__am_headers || {});
    } catch (_) {}
    return originalSend.call(this, body);
  };

  const snapshot = () => {
    try {
      window.__am_capture.cookieHeader = document.cookie || window.__am_capture.cookieHeader || "";
      if (!window.__am_capture.storefront) {
        const fromStorage = localStorage.getItem("music.ampwebplay.storefront")
          || localStorage.getItem("storefront")
          || localStorage.getItem("amp-storefront");
        if (fromStorage) window.__am_capture.storefront = String(fromStorage).replace(/["']/g, "").trim();
      }
      if (!window.__am_capture.developerToken) {
        const lsAuth = localStorage.getItem("music.ampwebplay.authorization")
          || sessionStorage.getItem("music.ampwebplay.authorization");
        if (lsAuth) window.__am_capture.developerToken = trimBearer(lsAuth);
      }
      if (!window.__am_capture.mediaUserToken) {
        const lsMedia = localStorage.getItem("music.ampwebplay.media-user-token")
          || sessionStorage.getItem("music.ampwebplay.media-user-token");
        if (lsMedia) window.__am_capture.mediaUserToken = String(lsMedia).trim();
      }
      post("snapshot", {
        developerToken: window.__am_capture.developerToken || "",
        mediaUserToken: window.__am_capture.mediaUserToken || "",
        rawCookieHeader: window.__am_capture.cookieHeader || "",
        storefront: window.__am_capture.storefront || ""
      });
    } catch (_) {}
  };

  setInterval(snapshot, 1500);
  snapshot();
})();
"#;

const TRACE_SCRIPT: &str = r#"
(() => {
  if (window.__am_trace_installed) return;
  window.__am_trace_installed = true;

  const post = (payload) => {
    try { window.ipc.postMessage(JSON.stringify({ kind: "trace", payload })); } catch (_) {}
  };
  const now = () => new Date().toISOString();
  const trim = (v, n = 800) => {
    const s = String(v ?? "");
    return s.length > n ? s.slice(0, n) + "...[truncated]" : s;
  };
  const hdrs = (h) => {
    const out = {};
    try {
      if (!h) return out;
      if (h.entries) for (const [k, v] of h.entries()) out[k] = trim(v, 300);
      else for (const k of Object.keys(h)) out[k] = trim(h[k], 300);
    } catch (_) {}
    return out;
  };

  const ofetch = window.fetch.bind(window);
  window.fetch = async (...args) => {
    const input = args[0];
    const init = args[1] || {};
    const url = typeof input === "string" ? input : ((input && input.url) || "");
    const method = (init.method || "GET").toUpperCase();
    post({ ts: now(), channel: "fetch", phase: "request", url, method, headers: hdrs(init.headers || {}), bodyPreview: trim(init.body || "") });
    try {
      const res = await ofetch(...args);
      let bodyPreview = "";
      try { bodyPreview = trim(await res.clone().text(), 1600); } catch (_) {}
      post({ ts: now(), channel: "fetch", phase: "response", url: res.url || url, method, status: res.status, contentType: res.headers.get("content-type") || "", bodyPreview });
      return res;
    } catch (err) {
      post({ ts: now(), channel: "fetch", phase: "error", url, method, error: trim(err && err.message ? err.message : String(err)) });
      throw err;
    }
  };

  const xOpen = XMLHttpRequest.prototype.open;
  const xSet = XMLHttpRequest.prototype.setRequestHeader;
  const xSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url, ...rest) {
    this.__trace = { method: String(method || "GET").toUpperCase(), url: String(url || ""), headers: {}, body: "" };
    return xOpen.call(this, method, url, ...rest);
  };
  XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
    this.__trace = this.__trace || { headers: {} };
    this.__trace.headers[name] = trim(value, 300);
    return xSet.call(this, name, value);
  };
  XMLHttpRequest.prototype.send = function(body) {
    this.__trace = this.__trace || { headers: {} };
    this.__trace.body = trim(body || "");
    post({ ts: now(), channel: "xhr", phase: "request", url: this.__trace.url || "", method: this.__trace.method || "GET", headers: this.__trace.headers || {}, bodyPreview: this.__trace.body || "" });
    this.addEventListener("loadend", () => {
      post({ ts: now(), channel: "xhr", phase: "response", url: this.responseURL || this.__trace.url || "", method: this.__trace.method || "GET", status: this.status, contentType: this.getResponseHeader("content-type") || "", bodyPreview: trim(this.responseText || "", 1600) });
    });
    return xSend.call(this, body);
  };

  post({ ts: now(), channel: "trace", phase: "started", note: "Trace hooks installed" });
})();
"#;

#[derive(Debug, Deserialize)]
struct IpcEnvelope {
    kind: String,
    payload: IpcPayload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IpcPayload {
    developer_token: Option<String>,
    media_user_token: Option<String>,
    raw_cookie_header: Option<String>,
    storefront: Option<String>,
}

pub fn capture_session_with_webview(timeout: Duration) -> Result<SessionCapture> {
    run_capture_window(timeout)
}

pub fn trace_playback_flow(timeout: Duration) -> Result<PathBuf> {
    let mut event_loop = EventLoop::<()>::new();
    let window = WindowBuilder::new()
        .with_title("Apple Music Playback Trace")
        .build(&event_loop)
        .context("failed to create trace window")?;

    let events: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let events_for_ipc = Arc::clone(&events);

    let profile_dir = crate::session::app_data_dir()?.join("webview_profile");
    fs::create_dir_all(&profile_dir).context("failed to create webview profile directory")?;
    let mut web_context = WebContext::new(Some(profile_dir));

    let webview = WebViewBuilder::new(window)
        .context("failed to create trace webview builder")?
        .with_url("https://music.apple.com/us/browse")
        .context("failed to set trace webview URL")?
        .with_initialization_script(TRACE_SCRIPT)
        .with_ipc_handler(move |_window, msg| {
            if let Ok(value) = serde_json::from_str::<Value>(&msg) {
                if value.get("kind").and_then(Value::as_str) == Some("trace") {
                    if let Some(payload) = value.get("payload") {
                        if let Ok(mut guard) = events_for_ipc.lock() {
                            guard.push(payload.clone());
                        }
                    }
                }
            }
        })
        .with_web_context(&mut web_context)
        .build()
        .context("failed to build trace webview")?;

    let start = Instant::now();
    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));
        match event {
            Event::MainEventsCleared => {
                let _ = webview.evaluate_script(TRACE_SCRIPT);
                if start.elapsed() >= timeout {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });

    let path = trace_output_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create trace directory")?;
    }

    let mut lines = Vec::new();
    if let Ok(events) = events.lock() {
        for event in events.iter() {
            lines.push(serde_json::to_string(event)?);
        }
    }
    fs::write(&path, lines.join("\n")).context("failed to write trace file")?;
    Ok(path)
}

fn run_capture_window(timeout: Duration) -> Result<SessionCapture> {
    let mut event_loop = EventLoop::<()>::new();
    let window = WindowBuilder::new()
        .with_title("Apple Music Login Capture")
        .build(&event_loop)
        .context("failed to create login window")?;

    let capture = Arc::new(Mutex::new(SessionCapture::default()));
    let capture_for_ipc = Arc::clone(&capture);

    let profile_dir = crate::session::app_data_dir()?.join("webview_profile");
    fs::create_dir_all(&profile_dir).context("failed to create webview profile directory")?;
    let mut web_context = WebContext::new(Some(profile_dir));

    let webview = WebViewBuilder::new(window)
        .context("failed to create webview builder")?
        .with_url("https://music.apple.com/us/browse")
        .context("failed to set webview URL")?
        .with_initialization_script(CAPTURE_SCRIPT)
        .with_ipc_handler(move |_window, msg| {
            if let Ok(parsed) = serde_json::from_str::<IpcEnvelope>(&msg) {
                if parsed.kind == "snapshot" {
                    if let Ok(mut state) = capture_for_ipc.lock() {
                        merge_payload(&mut state, parsed.payload);
                    }
                }
            }
        })
        .with_web_context(&mut web_context)
        .build()
        .context("failed to build webview")?;

    let start = Instant::now();
    let result: Arc<Mutex<Option<Result<SessionCapture>>>> = Arc::new(Mutex::new(None));
    let result_for_loop = Arc::clone(&result);

    event_loop.run_return(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(250));

        match event {
            Event::MainEventsCleared => {
                let _ = webview.evaluate_script(CAPTURE_SCRIPT);
                if start.elapsed() >= timeout {
                    let maybe = capture.lock().ok().map(|c| c.clone()).unwrap_or_default();
                    if let Ok(mut slot) = result_for_loop.lock() {
                        *slot = Some(finalize_capture(maybe));
                    }
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                let maybe = capture.lock().ok().map(|c| c.clone()).unwrap_or_default();
                if let Ok(mut slot) = result_for_loop.lock() {
                    *slot = Some(finalize_capture(maybe));
                }
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });

    match result.lock().ok().and_then(|mut slot| slot.take()) {
        Some(v) => v,
        None => Err(anyhow!("capture ended without session data")),
    }
}

fn merge_payload(target: &mut SessionCapture, payload: IpcPayload) {
    if let Some(value) = payload.storefront.and_then(normalize_opt) {
        target.storefront = Some(value);
    }
    if let Some(value) = payload.developer_token.and_then(normalize_opt) {
        target.developer_token = Some(value);
    }
    if let Some(value) = payload.media_user_token.and_then(normalize_opt) {
        target.media_user_token = Some(value);
    }
    if let Some(value) = payload.raw_cookie_header.and_then(normalize_opt) {
        target.raw_cookie_header = Some(value);
    }
}

fn finalize_capture(mut capture: SessionCapture) -> Result<SessionCapture> {
    if capture.storefront.is_none() {
        capture.storefront = Some("us".to_string());
    }
    if capture.developer_token.is_none() || capture.media_user_token.is_none() {
        bail!("webview capture did not collect both developer and media-user tokens");
    }
    Ok(capture)
}

fn normalize_opt(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn trace_output_path() -> Result<PathBuf> {
    let base_dir = dirs::data_local_dir().context("failed to find local data directory")?;
    let ts = jiff::Timestamp::now().strftime("%Y%m%d_%H%M%S");
    Ok(base_dir
        .join("apple_music_player")
        .join("traces")
        .join(format!("playback_trace_{ts}.jsonl")))
}
