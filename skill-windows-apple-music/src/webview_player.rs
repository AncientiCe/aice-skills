use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use wry::{
    application::{
        dpi::LogicalSize,
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
        platform::run_return::EventLoopExtRunReturn,
        window::WindowBuilder,
    },
    webview::{WebContext, WebViewBuilder},
};

use crate::session::SessionSnapshot;

const PLAYER_BOOTSTRAP_SCRIPT_TEMPLATE: &str = r#"
(() => {
  if (window.__amp_native_player_installed) return;
  window.__amp_native_player_installed = true;
  const preset = __AMP_PRESET__;

  try {
    const sf = preset.storefront || "us";
    localStorage.setItem("music.ampwebplay.storefront", sf);
    localStorage.setItem("storefront", sf);
    sessionStorage.setItem("music.ampwebplay.storefront", sf);
  } catch (_) {}

  try {
    if (preset.developerToken) {
      const bearer = "Bearer " + String(preset.developerToken);
      localStorage.setItem("music.ampwebplay.authorization", bearer);
      sessionStorage.setItem("music.ampwebplay.authorization", bearer);
    }
    if (preset.mediaUserToken) {
      localStorage.setItem("music.ampwebplay.media-user-token", String(preset.mediaUserToken));
      sessionStorage.setItem("music.ampwebplay.media-user-token", String(preset.mediaUserToken));
    }
  } catch (_) {}

  const post = (kind, payload) => {
    try { window.ipc.postMessage(JSON.stringify({ kind, payload })); } catch (_) {}
  };
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const storefront = String((preset && preset.storefront) || "us");

  async function waitForMusicKit(timeoutMs = 45000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        if (window.MusicKit && typeof window.MusicKit.getInstance === "function") {
          const mk = window.MusicKit.getInstance();
          if (mk) return mk;
        }
      } catch (_) {}
      await sleep(250);
    }
    throw new Error("MusicKit was not ready in the embedded player window.");
  }

  async function ensurePlayer() {
    const mk = await waitForMusicKit();
    return mk;
  }

  function markSessionReady(source, status) {
    post("session-ready", { source, status: Number(status || 0), ts: Date.now() });
  }

  function maybeMarkSessionReady(url, status) {
    try {
      const u = String(url || "");
      if (u.includes("/v1/me/") && Number(status) >= 200 && Number(status) < 300) {
        markSessionReady(u, status);
      }
    } catch (_) {}
  }

  function installSessionHooks() {
    if (window.__amp_session_hooks_installed) return;
    window.__amp_session_hooks_installed = true;

    const ofetch = window.fetch.bind(window);
    window.fetch = async (...args) => {
      const input = args[0];
      const url = typeof input === "string" ? input : ((input && input.url) || "");
      const res = await ofetch(...args);
      try { maybeMarkSessionReady(res.url || url, res.status); } catch (_) {}
      return res;
    };

    const xOpen = XMLHttpRequest.prototype.open;
    const xSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function(method, url, ...rest) {
      this.__amp_url = String(url || "");
      return xOpen.call(this, method, url, ...rest);
    };
    XMLHttpRequest.prototype.send = function(body) {
      this.addEventListener("loadend", () => {
        try { maybeMarkSessionReady(this.responseURL || this.__amp_url || "", this.status); } catch (_) {}
      });
      return xSend.call(this, body);
    };
  }

  async function warmupSession() {
    try {
      const res = await fetch("https://amp-api.music.apple.com/v1/me/account?meta=subscription", {
        method: "GET",
        credentials: "include",
        headers: { "accept": "application/json" }
      });
      maybeMarkSessionReady("https://amp-api.music.apple.com/v1/me/account?meta=subscription", res.status);
    } catch (err) {
      post("session-warmup-error", { error: String(err && err.message ? err.message : err) });
    }
  }

  function isAuthorized(mk) {
    try {
      if (mk && mk.isAuthorized === true) return true;
      if (mk && mk.authorizationStatus === 3) return true;
      if (mk && mk.authorizationStatus === "authorized") return true;
    } catch (_) {}
    return false;
  }

  async function tryQueueAndPlay(mk, request) {
    await mk.setQueue(request);
    await mk.play();
  }

  function pickAssetUrl(payload) {
    const seen = new Set();
    const stack = [payload];
    while (stack.length > 0) {
      const node = stack.pop();
      if (!node || typeof node !== "object") continue;
      if (seen.has(node)) continue;
      seen.add(node);
      if (Array.isArray(node)) {
        for (const item of node) stack.push(item);
        continue;
      }
      for (const [k, v] of Object.entries(node)) {
        if (typeof v === "string") {
          const lower = v.toLowerCase();
          if (lower.includes(".m3u8") || lower.includes("aod-ssl") || lower.includes("hls")) {
            return v;
          }
          if ((k.toLowerCase() === "url" || k === "URL") && lower.startsWith("http")) {
            return v;
          }
        } else if (v && typeof v === "object") {
          stack.push(v);
        }
      }
    }
    return "";
  }

  function isPreviewAssetUrl(url) {
    const u = String(url || "").toLowerCase();
    if (!u) return true;
    if (u.includes("audiopreview")) return true;
    if (u.includes("/preview/")) return true;
    if (u.includes("preview")) return true;
    return false;
  }

  async function preflightLicensedAsset(catalogId, libraryId) {
    const endpoint = "https://play.music.apple.com/WebObjects/MZPlay.woa/wa/webPlayback";
    const payloads = [];
    if (catalogId) {
      payloads.push({ language: "en-US", salableAdamId: String(catalogId), storefrontId: storefront });
      payloads.push({ adamId: String(catalogId), language: "en-US", storefrontId: storefront });
    }
    if (catalogId && libraryId) {
      payloads.push({
        catalogId: String(catalogId),
        id: String(libraryId),
        isLibrary: true,
        kind: "song",
        reportingId: String(catalogId),
        language: "en-US",
        storefrontId: storefront
      });
    }

    const diagnostics = [];
    const deadline = Date.now() + 30000;
    let sawSessionEnded = false;
    let firstSessionEndedAt = 0;

    while (Date.now() < deadline) {
      sawSessionEnded = false;
      for (const payload of payloads) {
        try {
          const res = await fetch(endpoint, {
            method: "POST",
            headers: {
              "accept": "application/json, text/plain, */*",
              "content-type": "application/json"
            },
            body: JSON.stringify(payload),
            credentials: "include"
          });
          let body = {};
          try { body = await res.json(); } catch (_) {}
          const asset = pickAssetUrl(body);
          const failureType = body && body.failureType ? String(body.failureType) : "";
          const customerMessage = body && body.customerMessage ? String(body.customerMessage) : "";
          diagnostics.push({
            status: res.status,
            payload,
            asset,
            failureType,
            customerMessage,
            bodyPreview: JSON.stringify(body).slice(0, 1200)
          });

          if (failureType === "2002" || customerMessage.toLowerCase().includes("session has ended")) {
            sawSessionEnded = true;
            if (!firstSessionEndedAt) firstSessionEndedAt = Date.now();
            continue;
          }

          if (!res.ok) continue;
          if (!asset) continue;
          if (isPreviewAssetUrl(asset)) {
            throw new Error("Apple returned preview-only asset for this track.");
          }
          return { asset, payload };
        } catch (err) {
          diagnostics.push({
            payload,
            error: String(err && err.message ? err.message : err)
          });
        }
      }

      // Session bootstrap can lag behind initial page load in embedded webviews.
      if (sawSessionEnded) {
        if (firstSessionEndedAt && Date.now() - firstSessionEndedAt > 8000) {
          try { location.reload(); } catch (_) {}
        }
      }
      await sleep(1200);
    }

    throw new Error("No licensed full asset from webPlayback after session wait. " + JSON.stringify(diagnostics.slice(-8)));
  }

  window.__amp_native_player = {
    ensurePlayer: ensurePlayer,
    playByIds: async function(catalogId, libraryId) {
      const mk = await ensurePlayer();
      const attempts = [];
      const requests = [];

      if (libraryId) {
        requests.push({ librarySong: String(libraryId) });
        requests.push({ librarySongs: [String(libraryId)] });
      }
      if (catalogId) {
        requests.push({ song: String(catalogId) });
        requests.push({ songs: [String(catalogId)] });
      }

      for (const request of requests) {
        try {
          await tryQueueAndPlay(mk, request);
          const nowPlayingId = mk.nowPlayingItem && mk.nowPlayingItem.id ? String(mk.nowPlayingItem.id) : "";
          const playbackMode = mk.nowPlayingItem && mk.nowPlayingItem.type ? String(mk.nowPlayingItem.type) : "";
          post("state", { ok: true, action: "play", request, nowPlayingId, playbackMode });
          return;
        } catch (error) {
          attempts.push({
            request,
            error: String(error && error.message ? error.message : error)
          });
        }
      }

      throw new Error("Unable to queue/play track via MusicKit (likely unauthenticated or no playable entitlement for this request): " + JSON.stringify(attempts));
    },
    pause: async function() {
      const mk = await ensurePlayer();
      await mk.pause();
      post("state", { ok: true, action: "pause" });
    },
    resume: async function() {
      const mk = await ensurePlayer();
      await mk.play();
      post("state", { ok: true, action: "resume" });
    },
    stop: async function() {
      const mk = await ensurePlayer();
      if (typeof mk.stop === "function") await mk.stop();
      else await mk.pause();
      post("state", { ok: true, action: "stop" });
    }
  };

  installSessionHooks();
  warmupSession();
  post("ready", { href: location.href, title: document.title || "" });
})();
"#;

#[derive(Debug)]
enum PlayerCommand {
    Play {
        catalog_id: String,
        library_id: Option<String>,
    },
    Pause,
    Resume,
    Stop,
    Next,
    Previous,
    ShuffleOn,
    ShuffleOff,
    Shutdown,
}

#[derive(Debug, Clone)]
enum PlayerEvent {
    Ready,
    SessionReady,
    State {
        action: String,
        ok: bool,
        error: Option<String>,
    },
    Error(String),
}

pub struct WebPlaybackController {
    storefront: String,
    tx: Sender<PlayerCommand>,
    rx: Receiver<PlayerEvent>,
    join: Option<thread::JoinHandle<()>>,
}

impl WebPlaybackController {
    pub fn new(session: &SessionSnapshot) -> Result<Self> {
        let storefront = session
            .storefront
            .clone()
            .unwrap_or_else(|| "us".to_string());
        let init_script = build_player_bootstrap_script(
            session.developer_token.as_deref(),
            session.media_user_token.as_deref(),
            &storefront,
        )?;
        let (tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
        let (evt_tx, rx) = mpsc::channel::<PlayerEvent>();
        let storefront_for_thread = storefront.clone();
        let init_script_for_thread = init_script.clone();
        let join = thread::Builder::new()
            .name("embedded-web-player".to_string())
            .spawn(move || {
                if let Err(err) = run_player_thread(
                    &storefront_for_thread,
                    &init_script_for_thread,
                    cmd_rx,
                    evt_tx.clone(),
                ) {
                    let _ = evt_tx.send(PlayerEvent::Error(format!("{err:#}")));
                }
            })
            .context("failed to spawn embedded web player thread")?;

        let controller = Self {
            storefront,
            tx,
            rx,
            join: Some(join),
        };
        controller.wait_until_ready(Duration::from_secs(60))?;
        Ok(controller)
    }

    pub fn storefront(&self) -> &str {
        &self.storefront
    }

    pub fn play(&self, catalog_id: &str, library_id: Option<&str>) -> Result<()> {
        self.tx
            .send(PlayerCommand::Play {
                catalog_id: catalog_id.to_string(),
                library_id: library_id.map(ToString::to_string),
            })
            .context("embedded player command channel closed")
    }

    pub fn pause(&self) -> Result<()> {
        self.send_and_wait(PlayerCommand::Pause, "pause", Duration::from_secs(10))
    }

    pub fn resume(&self) -> Result<()> {
        self.send_and_wait(PlayerCommand::Resume, "resume", Duration::from_secs(10))
    }

    pub fn stop(&self) -> Result<()> {
        self.send_and_wait(PlayerCommand::Stop, "stop", Duration::from_secs(10))
    }

    pub fn next(&self) -> Result<()> {
        self.send_and_wait(PlayerCommand::Next, "next", Duration::from_secs(10))
    }

    pub fn previous(&self) -> Result<()> {
        self.send_and_wait(PlayerCommand::Previous, "previous", Duration::from_secs(10))
    }

    pub fn shuffle(&self, enabled: bool) -> Result<()> {
        if enabled {
            self.send_and_wait(
                PlayerCommand::ShuffleOn,
                "shuffle_on",
                Duration::from_secs(10),
            )
        } else {
            self.send_and_wait(
                PlayerCommand::ShuffleOff,
                "shuffle_off",
                Duration::from_secs(10),
            )
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.tx.send(PlayerCommand::Shutdown);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }

    fn wait_until_ready(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut page_ready = false;
        let mut session_ready = false;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("embedded player did not finish session warmup before timeout");
            }
            match self
                .rx
                .recv_timeout(remaining.min(Duration::from_millis(500)))
            {
                Ok(PlayerEvent::Ready) => {
                    page_ready = true;
                    if page_ready && session_ready {
                        return Ok(());
                    }
                }
                Ok(PlayerEvent::SessionReady) => {
                    session_ready = true;
                    if page_ready && session_ready {
                        return Ok(());
                    }
                }
                Ok(PlayerEvent::Error(msg)) => return Err(anyhow!(msg)),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("embedded player event channel closed")
                }
            }
        }
    }

    fn send_and_wait(&self, cmd: PlayerCommand, action: &str, timeout: Duration) -> Result<()> {
        self.tx
            .send(cmd)
            .context("embedded player command channel closed")?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("embedded player command '{action}' timed out");
            }
            match self
                .rx
                .recv_timeout(remaining.min(Duration::from_millis(750)))
            {
                Ok(PlayerEvent::State {
                    action: got_action,
                    ok,
                    error,
                }) if got_action == action => {
                    if ok {
                        return Ok(());
                    }
                    return Err(anyhow!(
                        error.unwrap_or_else(|| format!("player action '{action}' failed"))
                    ));
                }
                Ok(PlayerEvent::Error(msg)) => return Err(anyhow!(msg)),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("embedded player event channel closed")
                }
            }
        }
    }
}

impl Drop for WebPlaybackController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_player_thread(
    storefront: &str,
    init_script: &str,
    cmd_rx: Receiver<PlayerCommand>,
    evt_tx: Sender<PlayerEvent>,
) -> Result<()> {
    #[cfg(target_os = "windows")]
    use wry::application::platform::windows::EventLoopExtWindows;
    #[cfg(target_os = "windows")]
    let mut event_loop = EventLoop::<()>::new_any_thread();
    #[cfg(not(target_os = "windows"))]
    let mut event_loop = EventLoop::<()>::new();
    let url = format!("https://music.apple.com/{storefront}/browse");
    let profile_dir = crate::session::app_data_dir()?.join("webview_profile");
    std::fs::create_dir_all(&profile_dir).context("failed to create webview profile directory")?;
    let mut web_context = WebContext::new(Some(profile_dir));

    let window = WindowBuilder::new()
        .with_title("Apple Music Tiny Player")
        .with_inner_size(LogicalSize::new(460.0, 140.0))
        .with_visible(false)
        .build(&event_loop)
        .context("failed to create embedded player window")?;

    let evt_tx_ipc = evt_tx.clone();
    let webview = WebViewBuilder::new(window)
        .context("failed to create embedded player webview builder")?
        .with_url(&url)
        .context("failed to set embedded player URL")?
        .with_initialization_script(init_script)
        .with_ipc_handler(move |_window, msg| {
            if let Some(evt) = parse_ipc_event(&msg) {
                let _ = evt_tx_ipc.send(evt);
            }
        })
        .with_web_context(&mut web_context)
        .build()
        .context("failed to build embedded player webview")?;

    let mut shutting_down = false;
    while !shutting_down {
        event_loop.run_return(|event, _, control_flow| {
            *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(120));

            match event {
                Event::MainEventsCleared => {
                    let _ = webview.evaluate_script(init_script);
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        if matches!(cmd, PlayerCommand::Shutdown) {
                            shutting_down = true;
                            *control_flow = ControlFlow::Exit;
                            break;
                        }
                        if let Err(err) = execute_player_command(&webview, cmd) {
                            let _ = evt_tx.send(PlayerEvent::Error(format!("{err:#}")));
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    let _ = evt_tx.send(PlayerEvent::Error(
                        "embedded player window was closed".to_string(),
                    ));
                    shutting_down = true;
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
        });
    }

    Ok(())
}

fn build_player_bootstrap_script(
    developer_token: Option<&str>,
    media_user_token: Option<&str>,
    storefront: &str,
) -> Result<String> {
    let preset = serde_json::json!({
        "developerToken": developer_token.unwrap_or(""),
        "mediaUserToken": media_user_token.unwrap_or(""),
        "storefront": storefront
    });
    let preset_json = serde_json::to_string(&preset)?;
    Ok(PLAYER_BOOTSTRAP_SCRIPT_TEMPLATE.replace("__AMP_PRESET__", &preset_json))
}

fn execute_player_command(webview: &wry::webview::WebView, cmd: PlayerCommand) -> Result<()> {
    let js = match cmd {
        PlayerCommand::Play {
            catalog_id,
            library_id,
        } => {
            let catalog = serde_json::to_string(&catalog_id)?;
            let library = serde_json::to_string(&library_id.unwrap_or_default())?;
            format!(
                r#"(async()=>{{try{{await window.__amp_native_player.playByIds({catalog}, {library});window.ipc.postMessage(JSON.stringify({{kind:"state",payload:{{action:"play",ok:true}}}}));}}catch(e){{window.ipc.postMessage(JSON.stringify({{kind:"state",payload:{{action:"play",ok:false,error:String(e&&e.message?e.message:e)}}}}));}}}})();"#,
            )
        }
        PlayerCommand::Pause => r#"(async()=>{try{await window.__amp_native_player.pause();window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"pause",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"pause",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::Resume => r#"(async()=>{try{await window.__amp_native_player.resume();window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"resume",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"resume",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::Stop => r#"(async()=>{try{await window.__amp_native_player.stop();window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"stop",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"stop",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::Next => r#"(async()=>{try{const mk=await (window.__amp_native_player&&window.__amp_native_player.ensurePlayer?window.__amp_native_player.ensurePlayer():window.MusicKit.getInstance());if(mk&&typeof mk.skipToNextItem==="function"){await mk.skipToNextItem();}window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"next",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"next",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::Previous => r#"(async()=>{try{const mk=await (window.__amp_native_player&&window.__amp_native_player.ensurePlayer?window.__amp_native_player.ensurePlayer():window.MusicKit.getInstance());if(mk&&typeof mk.skipToPreviousItem==="function"){await mk.skipToPreviousItem();}window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"previous",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"previous",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::ShuffleOn => r#"(async()=>{try{const mk=await (window.__amp_native_player&&window.__amp_native_player.ensurePlayer?window.__amp_native_player.ensurePlayer():window.MusicKit.getInstance());if(mk){if(typeof mk.setShuffleMode==="function"){await mk.setShuffleMode(1);}else if("shuffleMode" in mk){mk.shuffleMode=1;}}window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"shuffle_on",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"shuffle_on",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::ShuffleOff => r#"(async()=>{try{const mk=await (window.__amp_native_player&&window.__amp_native_player.ensurePlayer?window.__amp_native_player.ensurePlayer():window.MusicKit.getInstance());if(mk){if(typeof mk.setShuffleMode==="function"){await mk.setShuffleMode(0);}else if("shuffleMode" in mk){mk.shuffleMode=0;}}window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"shuffle_off",ok:true}}));}catch(e){window.ipc.postMessage(JSON.stringify({kind:"state",payload:{action:"shuffle_off",ok:false,error:String(e&&e.message?e.message:e)}}));}})();"#.to_string(),
        PlayerCommand::Shutdown => return Ok(()),
    };

    webview
        .evaluate_script(&js)
        .context("failed to evaluate player command script")
}

fn parse_ipc_event(msg: &str) -> Option<PlayerEvent> {
    let value = serde_json::from_str::<Value>(msg).ok()?;
    let kind = value.get("kind").and_then(Value::as_str)?;
    let payload = value.get("payload");
    match kind {
        "ready" => Some(PlayerEvent::Ready),
        "session-ready" => Some(PlayerEvent::SessionReady),
        "state" => {
            let action = payload
                .and_then(|v| v.get("action"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let ok = payload
                .and_then(|v| v.get("ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let error = payload
                .and_then(|v| v.get("error"))
                .and_then(Value::as_str)
                .map(ToString::to_string);
            Some(PlayerEvent::State { action, ok, error })
        }
        _ => None,
    }
}
