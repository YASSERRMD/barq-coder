use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use crate::providers::ProviderKind;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Which LLM provider to use. Defaults to Ollama.
    #[serde(default)]
    pub provider: ProviderKind,

    // ── Ollama settings ───────────────────────────────────────────────────────
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,

    // ── OpenAI-compatible settings ────────────────────────────────────────────
    /// Base URL for OpenAI-compatible API (defaults to https://api.openai.com).
    #[serde(default)]
    pub openai_base_url: Option<String>,
    /// Model name when using the OpenAI provider (defaults to gpt-4o).
    #[serde(default)]
    pub openai_model: Option<String>,
    /// API key for OpenAI-compatible providers; can also be set via OPENAI_API_KEY.
    #[serde(default)]
    pub openai_api_key: Option<String>,

    // ── Storage ───────────────────────────────────────────────────────────────
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
    /// Optional hard budget cap in USD. None = unlimited.
    #[serde(default)]
    pub budget_cap_usd: Option<f64>,
}

fn default_ollama_base_url() -> String { "http://localhost:11434".to_string() }
fn default_ollama_model() -> String { "minimax-m2.7:cloud".to_string() }
fn default_barqdb_url() -> String { "localhost:6333".to_string() }
fn default_barqgraph_url() -> String { "localhost:6334".to_string() }
fn default_workspace_root() -> String { "./".to_string() }
fn default_max_iterations() -> u8 { 5 }
fn default_token_limit() -> u32 { 4096 }

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: ProviderKind::default(),
            ollama_base_url: default_ollama_base_url(),
            ollama_model: default_ollama_model(),
            openai_base_url: None,
            openai_model: None,
            openai_api_key: None,
            barqdb_url: default_barqdb_url(),
            barqgraph_url: default_barqgraph_url(),
            workspace_root: default_workspace_root(),
            max_iterations: default_max_iterations(),
            token_limit: default_token_limit(),
            budget_cap_usd: None,
        }
    }
}

impl Config {
    /// Load configuration with the following precedence (highest → lowest):
    /// 1. Environment variables (`BARQ_*`, `OPENAI_*`)
    /// 2. `Config.toml` file values
    /// 3. Built-in defaults
    pub fn load() -> Self {
        let mut config = if Path::new("Config.toml").exists() {
            match fs::read_to_string("Config.toml") {
                Ok(content) => match toml::from_str::<Config>(&content) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Warning: Config.toml is malformed ({e}); using defaults.");
                        Self::default()
                    }
                },
                Err(e) => {
                    eprintln!("Warning: could not read Config.toml ({e}); using defaults.");
                    Self::default()
                }
            }
        } else {
            Self::default()
        };

        config.apply_env_overrides();
        config
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("BARQ_PROVIDER") {
            self.provider = match v.to_lowercase().as_str() {
                "openai" => ProviderKind::OpenAi,
                _ => ProviderKind::Ollama,
            };
        }
        if let Ok(v) = std::env::var("BARQ_OLLAMA_URL") {
            self.ollama_base_url = v;
        }
        if let Ok(v) = std::env::var("BARQ_MODEL") {
            self.ollama_model = v;
        }
        if let Ok(v) = std::env::var("OPENAI_API_KEY") {
            self.openai_api_key = Some(v);
        }
        if let Ok(v) = std::env::var("OPENAI_BASE_URL") {
            self.openai_base_url = Some(v);
        }
        if let Ok(v) = std::env::var("OPENAI_MODEL") {
            self.openai_model = Some(v);
        }
        if let Ok(v) = std::env::var("BARQ_BARQDB_URL") {
            self.barqdb_url = v;
        }
        if let Ok(v) = std::env::var("BARQ_BARQGRAPH_URL") {
            self.barqgraph_url = v;
        }
        if let Ok(v) = std::env::var("BARQ_WORKSPACE") {
            self.workspace_root = v;
        }
        if let Ok(v) = std::env::var("BARQ_MAX_ITERATIONS") {
            if let Ok(n) = v.parse::<u8>() {
                self.max_iterations = n;
            } else {
                eprintln!("Warning: BARQ_MAX_ITERATIONS='{v}' is not a valid u8; ignoring.");
            }
        }
        if let Ok(v) = std::env::var("BARQ_TOKEN_LIMIT") {
            if let Ok(n) = v.parse::<u32>() {
                self.token_limit = n;
            } else {
                eprintln!("Warning: BARQ_TOKEN_LIMIT='{v}' is not a valid u32; ignoring.");
            }
        }
        if let Ok(v) = std::env::var("BARQ_BUDGET_CAP_USD") {
            if let Ok(f) = v.parse::<f64>() {
                self.budget_cap_usd = Some(f);
            } else {
                eprintln!("Warning: BARQ_BUDGET_CAP_USD='{v}' is not a valid f64; ignoring.");
            }
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        match self.provider {
            ProviderKind::Ollama => {
                if self.ollama_base_url.is_empty() {
                    errors.push("ollama_base_url must not be empty".to_string());
                }
                if self.ollama_model.is_empty() {
                    errors.push("ollama_model must not be empty".to_string());
                }
            }
            ProviderKind::OpenAi => {
                if self.openai_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "openai_api_key is required when provider = openai (or set OPENAI_API_KEY)"
                            .to_string(),
                    );
                }
            }
        }

        if self.max_iterations == 0 {
            errors.push("max_iterations must be at least 1".to_string());
        }
        if self.token_limit < 256 {
            errors.push(format!(
                "token_limit ({}) is dangerously low; minimum recommended is 256",
                self.token_limit
            ));
        }
        if let Some(cap) = self.budget_cap_usd {
            if cap <= 0.0 {
                errors.push(format!("budget_cap_usd ({cap}) must be a positive value"));
            }
        }

        errors
    }

    /// Human-readable active model label for TUI display.
    pub fn active_model_label(&self) -> String {
        match self.provider {
            ProviderKind::Ollama => format!("ollama:{}", self.ollama_model),
            ProviderKind::OpenAi => format!(
                "openai:{}",
                self.openai_model.as_deref().unwrap_or("gpt-4o")
            ),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write("Config.toml", content)?;
        Ok(())
    }
}
