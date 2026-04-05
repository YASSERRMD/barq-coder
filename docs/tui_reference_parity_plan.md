## Reference TUI Parity Plan

Reference repo:
- `/Users/mdyasser/Downloads/claude-code-source-code-main`

Reference files driving the interactive TUI:
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/screens/REPL.tsx`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/components/PromptInput/PromptInput.tsx`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/components/PromptInput/useMaybeTruncateInput.ts`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/hooks/usePasteHandler.ts`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/components/VirtualMessageList.tsx`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/components/permissions/PermissionPrompt.tsx`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/components/permissions/PermissionRequest.tsx`
- `/Users/mdyasser/Downloads/claude-code-source-code-main/src/components/permissions/FallbackPermissionRequest.tsx`

Goal:
- Keep the Rust TUI visually distinct from the reference.
- Match the reference interaction model and missing functionality in phased Rust ports.

Phase 3 scope:
- Port transcript and composer behavior first.
- Keep manual scroll stable while new output streams.
- Add a sticky prompt context when the transcript is scrolled away from the bottom.
- Keep large pasted prompts visible in the composer viewport.
- Preserve the distinct BarqCoder visual design rather than copying the TypeScript layout.

Next phases:
- Phase 4: structured permission prompt options, tool-specific preview behavior, and approval affordances.
- Phase 5: session/task panes, richer tool activity, and remaining transcript interactions.
- Phase 6: final reference parity pass, cleanup, and regression coverage.
