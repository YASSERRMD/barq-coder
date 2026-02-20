# BarqCoder ⚡

The Lightning Fast Agentic AI Coder built in Rust. 
BarqCoder is a blazing fast, local-first AI coding assistant that combines traditional code analysis with cutting-edge Local LLMs (via Ollama) and Google Gemini.

![BarqCoder Demo](demo.gif)

## Features
- **Local First**: Powered by Ollama (`qwen2.5-coder:1.5b`) for zero-latency, private code generation.
- **Graph Database Context**: Uses a custom `BarqDB` and GraphDB to map your codebase and provide intelligent semantic context.
- **Multi-Agent Orchestration**: Specialized agents (Planner, Coder, Tester, Reviewer) work together to decompose and solve complex tasks.
- **Symbolic Verifier**: A built-in AST walker that lints for unused code, type bounds, and excessive cloning inside loops before giving code to the AI.
- **VSCode LSP Support**: Fully functional Language Server Protocol integration.
- **Docker Ready**: One-command setup via Docker Compose.

## Installation
```bash
curl -fsSL https://raw.githubusercontent.com/YASSERRMD/barq-coder/main/scripts/install.sh | bash
```

## Quick Start
```bash
barqcoder --workspace /path/to/my-rust-project
> /index
> Add a JWT authentication endpoint with tests
```
