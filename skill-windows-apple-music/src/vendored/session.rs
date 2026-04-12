use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use aes_gcm::{
    aead::{generic_array::GenericArray, Aead},
    Aes256Gcm, KeyInit, Nonce,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use keyring::Entry;
use rand::RngCore;
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "apple_music_player";
const KEYRING_USER: &str = "session_master_key";
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionSnapshot {
    pub storefront: Option<String>,
    pub developer_token: Option<String>,
    pub media_user_token: Option<String>,
    pub raw_cookie_header: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl SessionSnapshot {
    pub fn is_configured(&self) -> bool {
        self.developer_token.is_some() && self.media_user_token.is_some()
    }

    pub fn merge(&mut self, capture: SessionCapture) {
        if let Some(v) = capture.storefront {
            self.storefront = Some(v);
        }
        if let Some(v) = capture.developer_token {
            self.developer_token = Some(v);
        }
        if let Some(v) = capture.media_user_token {
            self.media_user_token = Some(v);
        }
        if let Some(v) = capture.raw_cookie_header {
            self.raw_cookie_header = Some(v);
        }
        self.updated_at = Some(Utc::now());
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionCapture {
    pub storefront: Option<String>,
    pub developer_token: Option<String>,
    pub media_user_token: Option<String>,
    pub raw_cookie_header: Option<String>,
}

impl SessionCapture {
    pub fn capture_from_embedded_login() -> Result<Self> {
        println!("Opening embedded Apple Music login window.");
        println!("Sign in, wait for the browse page to settle, then close the window.");
        crate::webview_capture::capture_session_with_webview(std::time::Duration::from_secs(180))
    }

    pub fn capture_from_browser_prompt() -> Result<Self> {
        println!("Paste captured values from browser devtools/network.");
        println!("Tip: inspect an amp-api request from music.apple.com.");

        let developer_token =
            prompt("Authorization bearer token (without 'Bearer ', blank to skip): ")?;
        let media_user_token = prompt("media-user-token header value (blank to skip): ")?;
        let raw_cookie_header = prompt("Cookie header value (blank to skip): ")?;
        let storefront = prompt("Storefront (default us): ")?;

        Ok(Self {
            storefront: normalize_opt(storefront).or_else(|| Some("us".to_string())),
            developer_token: normalize_opt(developer_token),
            media_user_token: normalize_opt(media_user_token),
            raw_cookie_header: normalize_opt(raw_cookie_header),
        })
    }
}

pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let path = app_data_dir()?.join("session.enc");
        Ok(Self { path })
    }

    pub fn save(&self, session: &SessionSnapshot) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).context("failed to create data directory")?;
        }

        let plaintext =
            serde_json::to_vec_pretty(session).context("failed to serialize session")?;
        let encrypted = encrypt(&plaintext)?;
        fs::write(&self.path, encrypted).context("failed to write session file")?;
        Ok(())
    }

    pub fn load(&self) -> Result<SessionSnapshot> {
        let bytes = fs::read(&self.path).context("failed to read session file")?;
        let decrypted = decrypt(&bytes)?;
        let session = serde_json::from_slice::<SessionSnapshot>(&decrypted)
            .context("invalid session data")?;
        Ok(session)
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path).context("failed to remove session file")?;
        }
        Ok(())
    }
}

pub fn open_music_browser() -> Result<()> {
    webbrowser::open("https://music.apple.com/us/browse")
        .map(|_| ())
        .context("failed to open browser")
}

pub fn app_data_dir() -> Result<PathBuf> {
    let base_dir = dirs::data_local_dir().context("failed to find local data directory")?;
    Ok(base_dir.join("apple_music_player"))
}

fn prompt(message: &str) -> Result<String> {
    print!("{message}");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut text = String::new();
    io::stdin()
        .read_line(&mut text)
        .context("failed to read input")?;
    Ok(text.trim().to_string())
}

fn normalize_opt(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>> {
    let key = session_key()?;
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&key));
    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt(blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() <= NONCE_LEN {
        bail!("encrypted session payload is too short");
    }

    let key = session_key()?;
    let cipher = Aes256Gcm::new(GenericArray::from_slice(&key));
    let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    Ok(plaintext)
}

fn session_key() -> Result<[u8; 32]> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER).context("failed to access keyring")?;
    let key_b64 = match entry.get_password() {
        Ok(value) => value,
        Err(_) => {
            let mut bytes = [0u8; 32];
            rand::rng().fill_bytes(&mut bytes);
            let encoded = BASE64.encode(bytes);
            entry
                .set_password(&encoded)
                .context("failed to store key in keyring")?;
            encoded
        }
    };

    let key_bytes = BASE64
        .decode(key_b64)
        .context("failed to decode keyring key")?;
    if key_bytes.len() != 32 {
        bail!("keyring key has invalid length");
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}
