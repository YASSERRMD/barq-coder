use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    #[serde(default = "default_barqdb_url")]
    pub barqdb_url: String,
    #[serde(default = "default_barqgraph_url")]
    pub barqgraph_url: String,
    #[serde(default = "default_workspace_root")]
    pub workspace_root: String,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u8,
    #[serde(default = "default_token_limit")]
    pub token_limit: u32,
}

fn default_ollama_base_url() -> String { "http://localhost:11434".to_string() }
fn default_ollama_model() -> String { "gemini-pro-3.1".to_string() }
fn default_barqdb_url() -> String { "localhost:6333".to_string() }
fn default_barqgraph_url() -> String { "localhost:6334".to_string() }
fn default_workspace_root() -> String { "./".to_string() }
fn default_max_iterations() -> u8 { 5 }
fn default_token_limit() -> u32 { 4096 }

impl Default for Config {
    fn default() -> Self {
        Self {
            ollama_base_url: default_ollama_base_url(),
            ollama_model: default_ollama_model(),
            barqdb_url: default_barqdb_url(),
            barqgraph_url: default_barqgraph_url(),
            workspace_root: default_workspace_root(),
            max_iterations: default_max_iterations(),
            token_limit: default_token_limit(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        if Path::new("Config.toml").exists() {
            if let Ok(content) = fs::read_to_string("Config.toml") {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write("Config.toml", content)?;
        Ok(())
    }
}
