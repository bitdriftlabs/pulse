# Agent Guidelines for Pulse

## Code Style
- Do not add overly verbose method comments including obvious parameters. Just use a
  a simple summary.
- Always write professional comments. Go back and clean up comments before completing.

## Build/Test Commands
- Build: `cargo build --workspace`
- Test (all): `cargo test` or `cargo nextest run`
- Test (single): `cargo test test_name` or `cargo nextest run test_name`
- Test (specific crate): `cargo test -p crate-name`
- Clippy: `cargo clippy --workspace --bins --examples --tests -- --no-deps`
- Format: `cargo +nightly fmt`
- Do NOT pipe output unless absolutely necessary so the user can see what is happening.

## Important Notes
- Do NOT prefix cargo commands with `SKIP_PROTO_GEN=1` unless specifically needed
- Run cargo commands directly without environment variable overrides by default

## Recent Changes Verification
- After changes, run tests in all effected crates.
- Then run clippy and format per the above instructions.

