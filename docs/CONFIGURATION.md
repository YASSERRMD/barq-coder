# Configuration

BarqCoder relies on a `Config.toml` (or `.barqcoder.toml` natively) for configuration:

```toml
[models]
# Which model to use for core coding tasks
coder = "qwen2.5-coder:1.5b"
# Which model to use for planning and reasoning
planner = "qwen2.5-coder:7b"

[ollama]
base_url = "http://localhost:11434"

[gemini]
api_key = "ENV_VAR_OR_KEY"

[indexer]
# Automatically re-index on save
watch = true

[verifier]
# Run syn AST symbolic verification before tests
enable_symbolic_checks = true
```
