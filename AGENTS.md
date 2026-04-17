# Repository Guidelines

## Project Structure & Module Organization
This repository is a small Rust CLI. Core logic lives in `src/main.rs`; there are no internal modules yet. Package metadata and dependencies are defined in `Cargo.toml`, and the lockfile is tracked in `Cargo.lock`. Use `sample_clippings.txt` as a local fixture when validating parsing behavior. Keep new parsing helpers or output formatting code close to the CLI entry point until the file becomes large enough to split into modules under `src/`.

## Build, Test, and Development Commands
- `cargo run -- -i sample_clippings.txt` runs the converter against the bundled sample input.
- `cargo run -- -i path/to/My\ Clippings.txt -o highlights.md` writes Markdown to a file.
- `cargo build --release` creates the optimized binary at `target/release/kindle-to-markdown`.
- `cargo fmt -- --check` verifies Rust formatting; run `cargo fmt` before opening a PR.
- `cargo test` runs the test suite. There are no committed tests yet, so add them as part of behavior changes.
- `cargo clippy --all-targets --all-features -- -D warnings` checks for lints before review.

## Coding Style & Naming Conventions
Follow standard Rust style with 4-space indentation and `rustfmt` output as the source of truth. Prefer `snake_case` for functions and variables, `CamelCase` for structs, and descriptive names such as `parse_kindle_clippings`. Keep CLI flags defined with `clap` attributes near the `Args` struct. Avoid adding dependencies unless they materially simplify parsing or error handling.

## Testing Guidelines
Add unit tests in `src/main.rs` with `#[cfg(test)]` for pure parsing and Markdown rendering helpers, or move code into `src/lib.rs` if tests start to grow. Name tests after behavior, for example `parses_highlight_entries` or `renders_bookmarks_with_metadata`. Use `sample_clippings.txt` patterns to cover highlights, notes, bookmarks, and malformed entries.

## Commit & Pull Request Guidelines
There is no established commit history yet, so use short imperative commit subjects such as `Add note formatting test`. Keep commits focused on one change. PRs should describe the input cases affected, list commands run (`cargo fmt`, `cargo test`, `cargo clippy`), and include sample Markdown output when parser behavior changes.

## Security & Configuration Tips
Treat clipping files as personal data. Do not commit real Kindle exports containing private notes or highlights; use sanitized fixtures instead.
