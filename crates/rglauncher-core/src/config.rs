use std::{
    collections::HashMap, env, ops::Deref, path::PathBuf, str::FromStr
};

use chin_tools::{aanyhow, AResult};
use serde::Deserialize;

fn default_terminal_cmd() -> String {
    "foot".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommonConfig {
    pub icon_paths: Vec<String>,
    /// Terminal binary used for `Terminal=true` desktop entries (default: `foot`).
    /// Invoked as: `{terminal_cmd} -e <command>`.
    #[serde(default = "default_terminal_cmd")]
    pub terminal_cmd: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub db: DatabaseConfig,
    pub ui: Option<UI>,
    pub common: CommonConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub db_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UI {
    pub dark_mode: Option<bool>,
    pub icon_config: Option<String>,
    /// Window translucency. Default: true (transparent).
    #[serde(default)]
    pub transparent: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct IconConfig {
    pub paths: Vec<String>,
    pub alias: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ParsedConfig {
    pub config: Config,
    pub icon: Option<IconConfig>,
}

impl Deref for ParsedConfig {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

impl Config {
    /// Whether the window uses a translucent background. Default true.
    pub fn transparent(&self) -> bool {
        self.ui
            .as_ref()
            .and_then(|ui| ui.transparent)
            .unwrap_or(true)
    }

    pub fn read_from_toml_file(filepath: Option<&String>) -> AResult<ParsedConfig> {
        let config_path = match filepath {
            Some(fp) => PathBuf::from_str(fp.as_str())?,
            None => {
                let config_dir = match env::var("XDG_CONFIG_PATH") {
                    Ok(path) => path,
                    Err(_) => format!("{}/.config", env::var("HOME").unwrap()),
                };
                let config_path = format!("{}/rgui/rglauncher.toml", config_dir);
                PathBuf::from(config_path)
            }
        };

        let config_content = std::fs::read_to_string(&config_path)?;
        let config: Self = toml::from_str(&config_content.as_str())?;

        let icon_config = match config.ui.as_ref().and_then(|e| e.icon_config.as_ref()) {
            Some(icon_path) => {
                let icon_path = if PathBuf::from(icon_path.as_str()).is_absolute() {
                    icon_path.clone().into()
                } else {
                    config_path
                        .parent()
                        .ok_or(aanyhow!("Parent dir is none"))?
                        .join(&icon_path)
                };
                let config_content = std::fs::read_to_string(&icon_path)?;
                let icon_config = toml::from_str(&config_content)?;
                Some(icon_config)
            }
            None => None,
        };

        Ok(ParsedConfig {
            config,
            icon: icon_config,
        })
    }
}
