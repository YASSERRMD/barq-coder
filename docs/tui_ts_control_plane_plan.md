# TUI Rebuild Plan: TypeScript Control Plane

## Problem

The current Rust TUI still relies on heuristic post-processing of assistant text
and pre-rendered transcript strings. That causes three recurring failures:

1. assistant replies can surface as JSON envelopes or malformed partial content
2. tool activity and permission requests collapse into raw text instead of typed UI states
3. transcript scrolling and follow mode fight with streaming updates

The reference TypeScript repo avoids this by normalizing stream events into typed
message blocks before rendering. This rebuild follows that structure while
keeping the Rust terminal shell visually distinct.

## Decision

Adopt a persistent TypeScript control-plane process, invoked from Rust over
stdin/stdout JSON messages. The control plane will own transcript
normalization. Rust will own terminal rendering, keyboard handling, session
persistence, and tool execution.

This is a control-plane bridge, not a full UI rewrite. It removes the current
Rust-side guesswork without replacing the Rust TUI runtime.

## Phases

### Phase 0: Design Lock

- define the event protocol between Rust and the TypeScript control plane
- define the normalized transcript block model used by the Rust UI
- document the phased implementation and commit boundaries

Atomic tasks:

1. add this plan document

### Phase 1: Transcript Control Plane

- add a persistent TypeScript normalizer process
- support streamed assistant tokens, final assistant messages, tool calls,
  tool results, user prompts, system notices, and errors
- return typed transcript blocks instead of pre-rendered text

Atomic tasks:

1. add the TypeScript normalizer and protocol
2. add the Rust bridge that starts the process and exchanges JSON messages
3. add focused tests for transcript normalization behavior

### Phase 2: Rust Transcript Model

- replace string-only `ChatMessage` usage with typed transcript items
- render assistant text, tool uses, tool outputs, and system/error states as
  separate cards
- remove JSON-scrubbing heuristics from the main event loop

Atomic tasks:

1. add normalized transcript data structures in Rust
2. route orchestrator events through the control plane
3. render normalized transcript items in the TUI

### Phase 3: Permission Flow Parity

- normalize permission requests and decisions through the same typed pipeline
- render readable permission cards and queue state
- stop stuffing raw payload previews into the transcript

Atomic tasks:

1. add permission event normalization to the TypeScript control plane
2. update Rust permission state to consume normalized entries
3. tighten approval queue and follow-mode behavior

### Phase 4: Verification and Publishing

- replay saved sessions through the new transcript pipeline
- verify large pasted prompts, streaming, permissions, and scrolling
- push each phase branch after its atomic commits

Atomic tasks:

1. add regression tests and smoke checks
2. push the phase branch and record the verification results

## Protocol Sketch

Rust sends newline-delimited JSON commands:

- `reset`
- `user_message`
- `assistant_token`
- `assistant_done`
- `tool_call`
- `tool_result`
- `permission_request`
- `permission_resolved`
- `system_message`
- `error_message`

The TypeScript process returns a normalized snapshot:

- `messages`: ordered transcript items with typed blocks
- `streaming`: current in-progress assistant text
- `tool_feed`: concise activity entries for the side rail

## Success Criteria

- large streamed replies never render as JSON envelopes in the transcript
- tool calls and tool outputs render as readable cards with message text, not raw JSON dumps
- permission requests remain responsive and visible during tool execution
- transcript scrolling remains stable while new events stream in
