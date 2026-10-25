use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyNotation { #[default] Musical, Camelot }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode { BestMatch, ShowResults }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchPriority { Pools, Soundcloud, #[default] Youtube, Qobuz }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolCredentialRefs {
    pub username_ref: String,
    pub password_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolsConfig {
    pub bpmsupreme: PoolCredentialRefs,
    pub clubkillers: PoolCredentialRefs,
    pub livedjservice: PoolCredentialRefs,
    pub qobuz: PoolCredentialRefs,
    #[serde(default)]
    pub soulseek: PoolCredentialRefs,
}

impl Default for PoolsConfig {
    fn default() -> Self {
        Self {
            bpmsupreme: Default::default(),
            clubkillers: Default::default(),
            livedjservice: Default::default(),
            qobuz: Default::default(),
            soulseek: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub output_dir: String,
    pub auto_download: bool,
    pub search_mode: SearchMode,
    pub search_priority: SearchPriority,
    pub pools: PoolsConfig,
    #[serde(default)]
    pub bpm_range_min: Option<u32>,
    #[serde(default)]
    pub bpm_range_max: Option<u32>,
    #[serde(default)]
    pub key_notation: KeyNotation,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: dirs::audio_dir()
                .unwrap_or_default()
                .join("djdrop")
                .to_string_lossy()
                .into_owned(),
            auto_download: true,
            search_mode: SearchMode::BestMatch,
            search_priority: SearchPriority::Youtube,
            pools: Default::default(),
            bpm_range_min: None,
            bpm_range_max: None,
            key_notation: KeyNotation::Musical,
        }
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf> {
    let dir = app.path().app_config_dir().context("no config dir")?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("config.toml"))
}

pub fn read(app: &tauri::AppHandle) -> Result<Config> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&path)?;
    toml::from_str(&text).context("invalid config.toml")
}

pub fn write(app: &tauri::AppHandle, config: &Config) -> Result<()> {
    let path = config_path(app)?;
    let text = toml::to_string_pretty(config).context("serialize config")?;
    fs::write(path, text).context("write config.toml")
}

