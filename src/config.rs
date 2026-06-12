use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::providers::ProviderKind;
use crate::providers::capabilities::CapabilityOverride;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// Which LLM provider to use. Defaults to Ollama.
    #[serde(default)]
    pub provider: ProviderKind,

    // ── Ollama (local) ────────────────────────────────────────────────────────
    #[serde(default = "default_ollama_base_url")]
    pub ollama_base_url: String,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,

    // ── OpenAI ────────────────────────────────────────────────────────────────
    #[serde(default)]
    pub openai_base_url: Option<String>,
    #[serde(default)]
    pub openai_model: Option<String>,
    #[serde(default)]
    pub openai_api_key: Option<String>,

    // ── Anthropic ─────────────────────────────────────────────────────────────
    #[serde(default)]
    pub anthropic_api_key: Option<String>,
    #[serde(default)]
    pub anthropic_model: Option<String>,

    // ── Google Gemini ─────────────────────────────────────────────────────────
    #[serde(default)]
    pub gemini_api_key: Option<String>,
    #[serde(default)]
    pub gemini_model: Option<String>,
    #[serde(default)]
    pub gemini_base_url: Option<String>,

    // ── Mistral AI ────────────────────────────────────────────────────────────
    #[serde(default)]
    pub mistral_api_key: Option<String>,
    #[serde(default)]
    pub mistral_model: Option<String>,
    #[serde(default)]
    pub mistral_base_url: Option<String>,

    // ── Groq ──────────────────────────────────────────────────────────────────
    #[serde(default)]
    pub groq_api_key: Option<String>,
    #[serde(default)]
    pub groq_model: Option<String>,
    #[serde(default)]
    pub groq_base_url: Option<String>,

    // ── Together AI ───────────────────────────────────────────────────────────
    #[serde(default)]
    pub together_api_key: Option<String>,
    #[serde(default)]
    pub together_model: Option<String>,
    #[serde(default)]
    pub together_base_url: Option<String>,

    // ── DeepSeek ──────────────────────────────────────────────────────────────
    #[serde(default)]
    pub deepseek_api_key: Option<String>,
    #[serde(default)]
    pub deepseek_model: Option<String>,
    #[serde(default)]
    pub deepseek_base_url: Option<String>,

    // ── xAI Grok ─────────────────────────────────────────────────────────────
    #[serde(default)]
    pub xai_api_key: Option<String>,
    #[serde(default)]
    pub xai_model: Option<String>,
    #[serde(default)]
    pub xai_base_url: Option<String>,

    // ── Perplexity ────────────────────────────────────────────────────────────
    #[serde(default)]
    pub perplexity_api_key: Option<String>,
    #[serde(default)]
    pub perplexity_model: Option<String>,
    #[serde(default)]
    pub perplexity_base_url: Option<String>,

    // ── Cohere ────────────────────────────────────────────────────────────────
    #[serde(default)]
    pub cohere_api_key: Option<String>,
    #[serde(default)]
    pub cohere_model: Option<String>,
    #[serde(default)]
    pub cohere_base_url: Option<String>,

    // ── Fireworks AI ──────────────────────────────────────────────────────────
    #[serde(default)]
    pub fireworks_api_key: Option<String>,
    #[serde(default)]
    pub fireworks_model: Option<String>,
    #[serde(default)]
    pub fireworks_base_url: Option<String>,

    // ── Cerebras ─────────────────────────────────────────────────────────────
    #[serde(default)]
    pub cerebras_api_key: Option<String>,
    #[serde(default)]
    pub cerebras_model: Option<String>,
    #[serde(default)]
    pub cerebras_base_url: Option<String>,

    // ── Per-model capability overrides ────────────────────────────────────────
    /// User-supplied capability patches keyed by "provider_id:model_id".
    ///
    /// Example in Config.toml:
    /// ```toml
    /// [model_capability_overrides."ollama:llava:13b"]
    /// supports_vision = true
    ///
    /// [model_capability_overrides."openai:o3-mini"]
    /// supports_reasoning = true
    /// supports_system_message = false
    /// ```
    #[serde(default)]
    pub model_capability_overrides: HashMap<String, CapabilityOverride>,

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
            anthropic_api_key: None,
            anthropic_model: None,
            gemini_api_key: None,
            gemini_model: None,
            gemini_base_url: None,
            mistral_api_key: None,
            mistral_model: None,
            mistral_base_url: None,
            groq_api_key: None,
            groq_model: None,
            groq_base_url: None,
            together_api_key: None,
            together_model: None,
            together_base_url: None,
            deepseek_api_key: None,
            deepseek_model: None,
            deepseek_base_url: None,
            xai_api_key: None,
            xai_model: None,
            xai_base_url: None,
            perplexity_api_key: None,
            perplexity_model: None,
            perplexity_base_url: None,
            cohere_api_key: None,
            cohere_model: None,
            cohere_base_url: None,
            fireworks_api_key: None,
            fireworks_model: None,
            fireworks_base_url: None,
            cerebras_api_key: None,
            cerebras_model: None,
            cerebras_base_url: None,
            model_capability_overrides: HashMap::new(),
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
    /// 1. Environment variables
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
                "anthropic" => ProviderKind::Anthropic,
                "gemini" => ProviderKind::Gemini,
                "mistral" => ProviderKind::Mistral,
                "groq" => ProviderKind::Groq,
                "together" => ProviderKind::Together,
                "deepseek" => ProviderKind::DeepSeek,
                "xai" => ProviderKind::Xai,
                "perplexity" => ProviderKind::Perplexity,
                "cohere" => ProviderKind::Cohere,
                "fireworks" => ProviderKind::Fireworks,
                "cerebras" => ProviderKind::Cerebras,
                _ => ProviderKind::Ollama,
            };
        }
        // Ollama
        if let Ok(v) = std::env::var("BARQ_OLLAMA_URL") { self.ollama_base_url = v; }
        if let Ok(v) = std::env::var("BARQ_MODEL") { self.ollama_model = v; }
        // OpenAI
        if let Ok(v) = std::env::var("OPENAI_API_KEY") { self.openai_api_key = Some(v); }
        if let Ok(v) = std::env::var("OPENAI_BASE_URL") { self.openai_base_url = Some(v); }
        if let Ok(v) = std::env::var("OPENAI_MODEL") { self.openai_model = Some(v); }
        // Anthropic
        if let Ok(v) = std::env::var("ANTHROPIC_API_KEY") { self.anthropic_api_key = Some(v); }
        if let Ok(v) = std::env::var("ANTHROPIC_MODEL") { self.anthropic_model = Some(v); }
        // Gemini
        if let Ok(v) = std::env::var("GEMINI_API_KEY") { self.gemini_api_key = Some(v); }
        if let Ok(v) = std::env::var("GEMINI_MODEL") { self.gemini_model = Some(v); }
        if let Ok(v) = std::env::var("GEMINI_BASE_URL") { self.gemini_base_url = Some(v); }
        // Mistral
        if let Ok(v) = std::env::var("MISTRAL_API_KEY") { self.mistral_api_key = Some(v); }
        if let Ok(v) = std::env::var("MISTRAL_MODEL") { self.mistral_model = Some(v); }
        // Groq
        if let Ok(v) = std::env::var("GROQ_API_KEY") { self.groq_api_key = Some(v); }
        if let Ok(v) = std::env::var("GROQ_MODEL") { self.groq_model = Some(v); }
        // Together AI
        if let Ok(v) = std::env::var("TOGETHER_API_KEY") { self.together_api_key = Some(v); }
        if let Ok(v) = std::env::var("TOGETHER_MODEL") { self.together_model = Some(v); }
        // DeepSeek
        if let Ok(v) = std::env::var("DEEPSEEK_API_KEY") { self.deepseek_api_key = Some(v); }
        if let Ok(v) = std::env::var("DEEPSEEK_MODEL") { self.deepseek_model = Some(v); }
        // xAI
        if let Ok(v) = std::env::var("XAI_API_KEY") { self.xai_api_key = Some(v); }
        if let Ok(v) = std::env::var("XAI_MODEL") { self.xai_model = Some(v); }
        // Perplexity
        if let Ok(v) = std::env::var("PERPLEXITY_API_KEY") { self.perplexity_api_key = Some(v); }
        if let Ok(v) = std::env::var("PERPLEXITY_MODEL") { self.perplexity_model = Some(v); }
        // Cohere
        if let Ok(v) = std::env::var("COHERE_API_KEY") { self.cohere_api_key = Some(v); }
        if let Ok(v) = std::env::var("COHERE_MODEL") { self.cohere_model = Some(v); }
        // Fireworks
        if let Ok(v) = std::env::var("FIREWORKS_API_KEY") { self.fireworks_api_key = Some(v); }
        if let Ok(v) = std::env::var("FIREWORKS_MODEL") { self.fireworks_model = Some(v); }
        // Cerebras
        if let Ok(v) = std::env::var("CEREBRAS_API_KEY") { self.cerebras_api_key = Some(v); }
        if let Ok(v) = std::env::var("CEREBRAS_MODEL") { self.cerebras_model = Some(v); }
        // Shared infra
        if let Ok(v) = std::env::var("BARQ_BARQDB_URL") { self.barqdb_url = v; }
        if let Ok(v) = std::env::var("BARQ_BARQGRAPH_URL") { self.barqgraph_url = v; }
        if let Ok(v) = std::env::var("BARQ_WORKSPACE") { self.workspace_root = v; }
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
            ProviderKind::Anthropic => {
                if self.anthropic_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "anthropic_api_key is required (or set ANTHROPIC_API_KEY)".to_string(),
                    );
                }
            }
            ProviderKind::Gemini => {
                if self.gemini_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push("gemini_api_key is required (or set GEMINI_API_KEY)".to_string());
                }
            }
            ProviderKind::Mistral => {
                if self.mistral_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push("mistral_api_key is required (or set MISTRAL_API_KEY)".to_string());
                }
            }
            ProviderKind::Groq => {
                if self.groq_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push("groq_api_key is required (or set GROQ_API_KEY)".to_string());
                }
            }
            ProviderKind::Together => {
                if self.together_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "together_api_key is required (or set TOGETHER_API_KEY)".to_string(),
                    );
                }
            }
            ProviderKind::DeepSeek => {
                if self.deepseek_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "deepseek_api_key is required (or set DEEPSEEK_API_KEY)".to_string(),
                    );
                }
            }
            ProviderKind::Xai => {
                if self.xai_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push("xai_api_key is required (or set XAI_API_KEY)".to_string());
                }
            }
            ProviderKind::Perplexity => {
                if self.perplexity_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "perplexity_api_key is required (or set PERPLEXITY_API_KEY)".to_string(),
                    );
                }
            }
            ProviderKind::Cohere => {
                if self.cohere_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push("cohere_api_key is required (or set COHERE_API_KEY)".to_string());
                }
            }
            ProviderKind::Fireworks => {
                if self.fireworks_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "fireworks_api_key is required (or set FIREWORKS_API_KEY)".to_string(),
                    );
                }
            }
            ProviderKind::Cerebras => {
                if self.cerebras_api_key.as_deref().unwrap_or("").is_empty() {
                    errors.push(
                        "cerebras_api_key is required (or set CEREBRAS_API_KEY)".to_string(),
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
        match &self.provider {
            ProviderKind::Ollama => format!("ollama:{}", self.ollama_model),
            ProviderKind::OpenAi => format!(
                "openai:{}", self.openai_model.as_deref().unwrap_or("gpt-4o")
            ),
            ProviderKind::Anthropic => format!(
                "anthropic:{}", self.anthropic_model.as_deref().unwrap_or("claude-sonnet-4-6")
            ),
            ProviderKind::Gemini => format!(
                "gemini:{}", self.gemini_model.as_deref().unwrap_or("gemini-2.5-pro")
            ),
            ProviderKind::Mistral => format!(
                "mistral:{}", self.mistral_model.as_deref().unwrap_or("mistral-large-latest")
            ),
            ProviderKind::Groq => format!(
                "groq:{}", self.groq_model.as_deref().unwrap_or("llama-3.3-70b-versatile")
            ),
            ProviderKind::Together => format!(
                "together:{}",
                self.together_model.as_deref()
                    .unwrap_or("meta-llama/Llama-3.3-70B-Instruct-Turbo")
            ),
            ProviderKind::DeepSeek => format!(
                "deepseek:{}", self.deepseek_model.as_deref().unwrap_or("deepseek-chat")
            ),
            ProviderKind::Xai => format!(
                "xai:{}", self.xai_model.as_deref().unwrap_or("grok-3-latest")
            ),
            ProviderKind::Perplexity => format!(
                "perplexity:{}", self.perplexity_model.as_deref().unwrap_or("sonar-pro")
            ),
            ProviderKind::Cohere => format!(
                "cohere:{}", self.cohere_model.as_deref().unwrap_or("command-r-plus")
            ),
            ProviderKind::Fireworks => format!(
                "fireworks:{}",
                self.fireworks_model.as_deref()
                    .unwrap_or("accounts/fireworks/models/llama-v3p3-70b-instruct")
            ),
            ProviderKind::Cerebras => format!(
                "cerebras:{}", self.cerebras_model.as_deref().unwrap_or("llama3.1-70b")
            ),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        fs::write("Config.toml", content)?;
        Ok(())
    }
}
