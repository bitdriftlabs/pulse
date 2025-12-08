# Agent Guidelines for Pulse

## Build/Test Commands
- Build: `cargo build --workspace`
- Test (all): `cargo test` or `cargo nextest run`
- Test (single): `cargo test test_name` or `cargo nextest run test_name`
- Test (specific crate): `cargo test -p crate-name`
- Clippy: `cargo clippy --workspace --bins --examples --tests -- --no-deps`

## Important Notes
- Do NOT prefix cargo commands with `SKIP_PROTO_GEN=1` unless specifically needed
- Run cargo commands directly without environment variable overrides by default
