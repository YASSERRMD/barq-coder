# BarqCoder Tools

BarqCoder agents have access to a rich set of system tools:
- `CargoCheck`: Runs `cargo check` and returns the output to identify compilation errors.
- `EditFile`: Writes or modifies a target file.
- `BarqSearch`: Performs semantic search against `BarqDB` to find relevant project context.
- `ShellExec`: Runs an arbitrary shell command (sandboxed).
- `WorkspaceTool`: Allows agents to query and switch between multiple open repositories.
- `CargoBench`: Runs `cargo bench` and generates performance comparisons to help optimize code.
