use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub developer_token: Option<String>,
    pub media_user_token: Option<String>,
    pub cookie_header: Option<String>,
    pub storefront: Option<String>,
}

impl SessionSnapshot {
    pub fn is_configured(&self) -> bool {
        self.developer_token.is_some() && self.media_user_token.is_some()
    }
}

pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn new() -> Result<Self> {
        let path = app_data_dir()?.join("session.json");
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<SessionSnapshot> {
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read session file at {}", self.path.display()))?;
        let parsed: SessionSnapshot =
            serde_json::from_str(&raw).context("failed to parse session json")?;
        Ok(parsed)
    }
}

pub fn app_data_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir().context("local app data directory unavailable")?;
    let path = base.join("embedded-apple-music");
    fs::create_dir_all(&path)
        .with_context(|| format!("failed to create app data directory {}", path.display()))?;
    Ok(path)
}
