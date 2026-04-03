# Barq Coder

**Barq Coder** is an autonomous, local-first coding agent built in Rust. It combines semantic code indexing, a multi-agent execution swarm, and a rich terminal interface to let you describe what you want and have the agent plan, implement, test, and review the code — entirely on your machine.

---

## Overview

Barq Coder runs a ReAct (Reasoning + Acting) orchestrator loop against any model served by [Ollama](https://ollama.com). It indexes your codebase into a semantic vector database (BarqDB), decomposes complex goals into a dependency graph, and dispatches sub-tasks to specialized agents in parallel — all while surfacing results in a multi-pane terminal UI.

```
barqcoder --workspace ./my-project
> Add a JWT authentication middleware with tests
```

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   CLI / TUI (ratatui)                │
└────────────────────────┬────────────────────────────┘
                         │
              ┌──────────▼──────────┐
              │   ReAct Orchestrator │  ← context budget, auto-compact
              └──────────┬──────────┘
                         │
         ┌───────────────▼───────────────┐
         │        Tool Registry (20+)     │
         │  ReadFile  GlobTool  GrepTool  │
         │  EditFile  ShellExec  WebFetch │
         │  delegate_task  BarqSearch … │
         └───────────────┬───────────────┘
                         │
          ┌──────────────▼──────────────┐
          │     Multi-Agent Swarm (DAG)  │
          │  Planner → Coder ┐           │
          │               Tester ┐       │
          │                  Reviewer ←─┘│
          └──────────────────────────────┘
```

---

## Key Features

**Autonomous Agent Loop**
A streaming ReAct orchestrator calls Ollama, parses tool invocations, executes them, and feeds results back — looping until a final answer is reached. Context is automatically compacted when the token budget is exceeded.

**Multi-Agent Swarm**
Complex goals are decomposed by a `PlannerAgent` into a dependency DAG. Steps whose dependencies are satisfied are launched concurrently via `tokio::spawn` + `FuturesUnordered`. `CoderAgent`, `TesterAgent`, and `ReviewerAgent` each run independently with their own LLM context.

**Semantic Code Index (BarqDB)**
Workspace files are indexed into a vector database for semantic search. The orchestrator automatically injects relevant code snippets into the system prompt before each turn.

**Permission System**
Every tool call is checked against a path sandbox and allowed/denied list. Destructive operations pause the agent and prompt the user interactively in the TUI (`[Y]es / [N]o`). CI environments can bypass with `--dangerously-skip-permissions`.

**Project Memory**
Agent-relevant instructions are persisted in `.barqcoder.md` at the workspace root. These are loaded automatically and injected into every system prompt — similar to Claude Code's `CLAUDE.md`.

**Session Persistence**
Sessions are stored as append-only JSONL files. Use `--continue` to resume the last session or `--resume <id>` to restore a specific one.

**Headless / SDK Mode**
Run a single prompt without the TUI and pipe the output:
```bash
barqcoder print "refactor the auth module" --json | jq .response
```

**VS Code + LSP**
A Language Server Protocol implementation is included. Launch with `barqcoder --lsp` and configure the provided VS Code extension.

---

## Installation

**Prerequisites:** [Rust](https://rustup.rs) 1.75+, [Ollama](https://ollama.com) running locally

```bash
git clone https://github.com/YASSERRMD/barq-coder
cd barq-coder
cargo build --release
cp target/release/barqcoder /usr/local/bin/
```

Verify the setup:
```bash
barqcoder doctor
```

---

## Configuration

On first run, Barq Coder reads `Config.toml` from the current directory, falling back to built-in defaults.

```toml
# Config.toml
ollama_base_url = "http://localhost:11434"
ollama_model    = "qwen2.5-coder:7b"
workspace_root  = "./"
max_iterations  = 10
token_limit     = 32768
```

All values can be overridden at runtime via CLI flags or environment variables.

---

## Usage

### Interactive mode (default)

```bash
barqcoder                           # TUI with default config
barqcoder --workspace ./my-project  # specify workspace
barqcoder --model qwen2.5-coder:14b # override model
barqcoder --continue                # resume last session
barqcoder --resume session_1712345  # resume specific session
```

### Headless mode

```bash
barqcoder print "add error handling to the database layer"
barqcoder print "explain the auth flow" --json
```

### Subcommands

| Command | Description |
|---|---|
| `barqcoder index [path]` | Index a directory into BarqDB |
| `barqcoder sessions` | List saved sessions |
| `barqcoder sessions --show <id>` | Replay a session's events |
| `barqcoder memory` | Show project memory |
| `barqcoder memory --add "always use anyhow for errors"` | Add a memory entry |
| `barqcoder doctor` | Check Ollama connectivity |

### Slash commands (inside TUI)

| Command | Description |
|---|---|
| `/compact` | Compress conversation history to reclaim context space |
| `/plan` | Enter plan mode — agent outlines steps before acting |
| `/review` | Show all file edits made this session |
| `/memory [show]` | Display project memory |
| `/memory add <note>` | Add a note to `.barqcoder.md` |
| `/model <name>` | Switch the active model mid-session |
| `/status` | Show token usage, turn count, tool calls |
| `/goal <text>` | Dispatch a goal to the full multi-agent swarm |
| `/clear` | Clear chat history and conversation context |
| `/help` | List all commands and key bindings |

---

## TUI Key Bindings

| Key | Action |
|---|---|
| `Enter` | Send message |
| `↑ / ↓` | Navigate input history |
| `PageUp / PageDown` | Scroll chat |
| `Tab / Shift+Tab` | Switch tabs (Chat / Diff / Sessions) |
| `Alt+S` | Toggle file sidebar |
| `F1` | Focus sidebar |
| `Y / N` | Approve or deny a tool permission request |
| `Esc` | Quit |

---

## Tool Registry

| Tool | Description |
|---|---|
| `read_file` | Read a file with optional line range |
| `edit_file` | Apply string-replace or unified diff edits |
| `create_file` | Create a new file |
| `list_files` | List directory contents |
| `glob` | Search files by path pattern |
| `grep` | Search file contents (ripgrep-accelerated) |
| `shell` | Execute shell commands |
| `git` | Run git operations |
| `cargo_check` | Build and type-check Rust projects |
| `web_fetch` | Fetch and extract text from a URL |
| `barq_search` | Semantic code search across the indexed workspace |
| `delegate_task` | Dispatch a goal to the multi-agent swarm |
| `file_history` | Undo/redo file edits |
| `tool_search` | Search the tool registry by keyword |

---

## Recommended Models

| Model | Size | Strength |
|---|---|---|
| `qwen2.5-coder:7b` | 4.7 GB | General coding, fast |
| `qwen2.5-coder:14b` | 9.0 GB | Higher accuracy |
| `deepseek-coder-v2` | 8.9 GB | Strong reasoning |
| `codellama:13b` | 7.4 GB | Function calling |

Pull a model: `ollama pull qwen2.5-coder:7b`

---

## Project Memory

Create `.barqcoder.md` at the root of your workspace. Its contents are injected into every system prompt:

```markdown
# My Project

- This is a Rust/Axum backend. Prefer anyhow for error handling.
- Always write integration tests for new endpoints.
- Database migrations use sqlx and live in `migrations/`.
```

Add notes interactively: `/memory add always use snake_case for DB columns`

---

## Containerization (Docker)

Barq Coder provides an optimized multi-stage `Dockerfile` and builds into a tiny, non-root Debian image.

```bash
docker pull barqcoder:latest
# or build locally:
docker build -t barqcoder:local .

# Run Barq Coder, mounting your workspace and propagating network to reach Ollama
docker run -it --rm \
  --network host \
  -v $(pwd):/workspace \
  barqcoder:local --workspace /workspace
```

---

## Production & CI Deployment

Barq Coder is fully configurable for CI pipelines using the `--dangerously-skip-permissions` and `--print` flags.

Example GitHub Actions step using Barq Coder as an autonomous code reviewer:
```yaml
- name: Review Code
  run: |
    barqcoder print "Review the latest changes and suggest optimizations" \
      --dangerously-skip-permissions \
      --workspace .
```

---

## License

MIT License. See [LICENSE](LICENSE).
