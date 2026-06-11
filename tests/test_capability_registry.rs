use barqcoder::providers::capabilities::{
    CapabilityOverride, ProviderCapabilities, ToolSupportLevel,
};
use barqcoder::providers::registry::CapabilityRegistry;
use std::collections::HashMap;

fn ollama_base() -> ProviderCapabilities {
    ProviderCapabilities::ollama_default()
}

fn openai_base() -> ProviderCapabilities {
    ProviderCapabilities::openai_default()
}

// ─── CapabilityOverride::apply_to ────────────────────────────────────────────

#[test]
fn override_applies_individual_fields() {
    let mut cap = CapabilityOverride::default();
    cap.supports_vision = Some(true);
    cap.tool_support = Some(ToolSupportLevel::Prompted);

    let result = cap.apply_to(ollama_base());
    assert!(result.supports_vision);
    assert!(matches!(result.tool_support, ToolSupportLevel::Prompted));
    // Unset fields preserve the base
    assert!(result.supports_streaming);
    assert!(!result.supports_reasoning);
}

#[test]
fn empty_override_is_identity() {
    let cap = CapabilityOverride::default();
    assert!(!cap.has_any());

    let base = ollama_base();
    let result = cap.apply_to(base.clone());
    assert_eq!(result.max_context_tokens, base.max_context_tokens);
    assert_eq!(result.supports_vision, base.supports_vision);
}

// ─── CapabilityRegistry built-in model knowledge ─────────────────────────────

#[test]
fn ollama_vision_model_detected_by_name() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("ollama", "llava:13b", ollama_base());
    assert!(caps.supports_vision, "llava:13b should have vision detected");
}

#[test]
fn ollama_bakllava_detected() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("ollama", "bakllava", ollama_base());
    assert!(caps.supports_vision);
}

#[test]
fn ollama_deepseek_r1_is_reasoning() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("ollama", "deepseek-r1:14b", ollama_base());
    assert!(caps.supports_reasoning);
    assert!(matches!(caps.tool_support, ToolSupportLevel::Prompted));
}

#[test]
fn ollama_qwq_is_reasoning() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("ollama", "qwq:32b", ollama_base());
    assert!(caps.supports_reasoning);
}

#[test]
fn ollama_standard_model_unchanged() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("ollama", "llama3.1:8b", ollama_base());
    assert!(!caps.supports_vision);
    assert!(!caps.supports_reasoning);
    assert!(matches!(caps.tool_support, ToolSupportLevel::Native));
}

#[test]
fn openai_o1_has_no_system_message() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("openai", "o1-preview", openai_base());
    assert!(caps.supports_reasoning);
    assert!(!caps.supports_system_message);
    assert!(!caps.supports_parallel_tool_calls);
}

#[test]
fn openai_o3_mini_detected() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("openai", "o3-mini", openai_base());
    assert!(caps.supports_reasoning);
    assert!(!caps.supports_system_message);
}

#[test]
fn openai_gpt4o_has_vision() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("openai", "gpt-4o", openai_base());
    assert!(caps.supports_vision);
}

#[test]
fn openai_gpt4o_mini_has_vision() {
    let reg = CapabilityRegistry::new();
    let caps = reg.lookup("openai", "gpt-4o-mini", openai_base());
    assert!(caps.supports_vision);
}

// ─── User overrides take highest precedence ───────────────────────────────────

#[test]
fn user_override_wins_over_built_in() {
    // Built-in says deepseek-r1 uses Prompted; user forces Native
    let mut overrides = HashMap::new();
    let mut user_cap = CapabilityOverride::default();
    user_cap.tool_support = Some(ToolSupportLevel::Native);
    overrides.insert("ollama:deepseek-r1:14b".to_string(), user_cap);

    let reg = CapabilityRegistry::from_config_overrides(&overrides);
    let caps = reg.lookup("ollama", "deepseek-r1:14b", ollama_base());
    // Built-in would set Prompted; user overrides to Native
    assert!(matches!(caps.tool_support, ToolSupportLevel::Native));
    // But built-in reasoning flag should still come through
    assert!(caps.supports_reasoning);
}

#[test]
fn user_override_can_add_vision_to_unknown_model() {
    let mut overrides = HashMap::new();
    let mut user_cap = CapabilityOverride::default();
    user_cap.supports_vision = Some(true);
    user_cap.max_context_tokens = Some(200_000);
    overrides.insert("ollama:my-custom-model".to_string(), user_cap);

    let reg = CapabilityRegistry::from_config_overrides(&overrides);
    let caps = reg.lookup("ollama", "my-custom-model", ollama_base());
    assert!(caps.supports_vision);
    assert_eq!(caps.max_context_tokens, 200_000);
}

#[test]
fn unknown_provider_gets_default_only() {
    let reg = CapabilityRegistry::new();
    let base = ProviderCapabilities::minimal();
    let caps = reg.lookup("custom-provider", "some-model", base.clone());
    assert_eq!(caps.max_context_tokens, base.max_context_tokens);
    assert_eq!(caps.supports_vision, base.supports_vision);
}

// ─── TrustTier enforcement ────────────────────────────────────────────────────

#[test]
fn trust_tier_permits_correct_tools() {
    use barqcoder::providers::TrustTier;

    assert!(TrustTier::Full.permits_tool("shell_exec"));
    assert!(TrustTier::Shell.permits_tool("shell_exec"));
    assert!(!TrustTier::CodeModify.permits_tool("shell_exec"));
    assert!(!TrustTier::ReadOnly.permits_tool("shell_exec"));

    assert!(TrustTier::CodeModify.permits_tool("edit_file"));
    assert!(!TrustTier::ReadOnly.permits_tool("edit_file"));

    assert!(TrustTier::Full.permits_tool("delegate_task"));
    assert!(!TrustTier::Shell.permits_tool("delegate_task"));

    assert!(TrustTier::ReadOnly.permits_tool("barq_search"));
    assert!(TrustTier::ReadOnly.permits_tool("read_file"));
}

// ─── PermissionManager::check_trust_tier ─────────────────────────────────────

#[test]
fn check_trust_tier_allows_permitted_tool() {
    use barqcoder::permissions::PermissionManager;
    use barqcoder::providers::TrustTier;
    use barqcoder::tools::PermissionResult;

    let mgr = PermissionManager::new("/tmp");
    let result = mgr.check_trust_tier(TrustTier::Full, "shell_exec");
    assert!(matches!(result, PermissionResult::Allow));
}

#[test]
fn check_trust_tier_denies_unpermitted_tool() {
    use barqcoder::permissions::PermissionManager;
    use barqcoder::providers::TrustTier;
    use barqcoder::tools::PermissionResult;

    let mgr = PermissionManager::new("/tmp");
    let result = mgr.check_trust_tier(TrustTier::ReadOnly, "shell_exec");
    assert!(matches!(result, PermissionResult::Deny(_)));
}

#[test]
fn check_trust_tier_denies_code_modify_at_readonly() {
    use barqcoder::permissions::PermissionManager;
    use barqcoder::providers::TrustTier;
    use barqcoder::tools::PermissionResult;

    let mgr = PermissionManager::new("/tmp");
    let result = mgr.check_trust_tier(TrustTier::ReadOnly, "edit_file");
    assert!(matches!(result, PermissionResult::Deny(_)));
}
