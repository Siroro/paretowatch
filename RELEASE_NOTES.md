# ParetoWatch v0.11.0

## Highlights

- Synchronized the HTTP user-agent with the compiled package version instead of a stale hard-coded release number.
- Kept release metadata and network identity aligned automatically for future version bumps.
- Preserved the v0.10 history persistence and compatibility behavior while tightening release maintenance.

## Validation

The v0.11.0 maintenance changes are validated with the repository's formatting, locked check, locked test, Clippy, and locked build commands. GitHub release packages are produced for Windows and Linux by the `v0.11.0` tag workflow.
