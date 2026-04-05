# TUI Rebuild Plan

This rewrite replaces the current TUI implementation instead of incrementally patching it.

## Goal

Build a new terminal UI for BarqCoder that matches the interaction model of the provided Claude Code reference as closely as possible, while wiring it to BarqCoder's orchestrator, tools, permissions, sessions, and diff workflow.

## Constraint

The reference repository currently lives at:

`/Users/mdyasser/Downloads/claude-code-source-code-main`

The Codex app cannot read that path yet because macOS is returning `Operation not permitted`. Exact parity work is blocked until that repository is moved into an accessible location or the app is granted access to `Downloads`.

## Principles

1. Do not reuse the current TUI structure as the base design.
2. Rebuild the event loop, rendering, and permission UX from clean interfaces.
3. Keep each phase small enough to verify and commit independently.
4. Push each phase branch separately.
5. Do not touch the dirty source checkout directly.

## Phase 0

Branch: `codex/phase_0-rebuild-plan`

Tasks:

1. Create a clean isolated clone for the rebuild.
2. Record the rebuild plan and branch structure.
3. Confirm git identity and workflow rules.

Acceptance:

1. The rebuild happens in an isolated workspace.
2. There is a committed plan document for the remaining phases.

## Phase 1

Branch: `codex/phase_1-tui-runtime`

Tasks:

1. Read the reference implementation and extract its app loop, state model, and screen layout.
2. Define a new BarqCoder TUI state model with explicit app state, focus state, pending actions, and streaming state.
3. Replace the current TUI entry path with a clean runtime shell.
4. Implement the main layout and rendering skeleton without reusing the current rendering flow.

Acceptance:

1. TUI starts cleanly.
2. Tabs and pane focus work.
3. Rendering remains responsive while idle and while streaming tokens.

## Phase 2

Branch: `codex/phase_2-chat-and-tools`

Tasks:

1. Rebuild chat rendering and streaming message updates.
2. Rebuild tool call activity rendering with bounded payload summaries.
3. Rebuild diff capture and session event display.
4. Add tests for large tool payloads and long chat output.

Acceptance:

1. Streaming does not duplicate messages.
2. Large tool payloads do not freeze the UI.
3. Diff and tool activity update in real time.

## Phase 3

Branch: `codex/phase_3-permissions`

Tasks:

1. Rebuild permission requests around an explicit modal or inline approval controller modeled on the reference.
2. Ensure permission prompts interrupt tool execution without hanging input handling.
3. Implement remembered approvals and queue behavior.
4. Add regression tests for sequential approvals and long previews.

Acceptance:

1. Tool calls pause for approval without freezing the UI.
2. `Y`, `A`, `N`, and `Esc` work reliably.
3. Large edit previews remain responsive.

## Phase 4

Branch: `codex/phase_4-sidebar-sessions`

Tasks:

1. Rebuild workspace sidebar behavior.
2. Rebuild session browser and replay flow.
3. Rebuild file preview and session preview panes.
4. Add final smoke tests and documentation updates.

Acceptance:

1. Sidebar navigation is stable.
2. Session replay works from the rebuilt UI.
3. File and session previews do not interfere with chat and permissions.

## Verification Per Phase

1. `cargo check`
2. Focused unit tests for the phase
3. `cargo run` TTY smoke launch and exit

## Git Workflow

1. Complete one atomic task.
2. Commit immediately.
3. Push the phase branch.
4. Open a PR for that phase branch.
5. Continue with the next phase on a new branch.
