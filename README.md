# kindle-to-markdown

Convert Kindle `My Clippings.txt` exports into clean, readable Markdown.

> A small Rust CLI for turning highlights, notes, and bookmarks into something you can keep in a notes app, publish, or version in git.

## What It Does

| Input | Output |
| --- | --- |
| Highlights | Markdown blockquotes |
| Notes | Bold note callouts |
| Bookmarks | Explicit bookmark markers |
| Mixed books | Grouped sections by title and author |
| Kindle metadata | Preserved location and added date |

## Example

Input:

```text
Clean Code (Robert C. Martin) - Your Highlight on Location 1234-1237 | Added on Saturday, August 10, 2024 9:45:00 AM

Functions should do one thing. They should do it well. They should do it only.

==========
```

Output:

```md
# Clean Code by Robert C. Martin

> Functions should do one thing. They should do it well. They should do it only.

*Location: 1234-1237 | Added: Saturday, August 10, 2024 9:45:00 AM*
```

## Quick Start

```bash
cargo run -- -i sample_clippings.txt
```

Write to a file:

```bash
cargo run -- -i "My Clippings.txt" -o highlights.md
```

Build a release binary:

```bash
cargo build --release
./target/release/kindle-to-markdown -i "My Clippings.txt" -o highlights.md
```

## Development Workflow

This repository ships with local and CI quality gates:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
pre-commit install
pre-commit run --all-files
```

## Project Layout

```text
src/lib.rs      Parsing and Markdown rendering logic
src/main.rs     CLI entry point
sample_clippings.txt
.github/workflows/ci.yml
.pre-commit-config.yaml
```

## Why This Repo Is Ready for Public Use

- Stable Rust toolchain pinned with `rustfmt` and `clippy`
- GitHub Actions CI for format, lint, and test checks
- Pre-commit hooks for local enforcement
- Unit tests covering parse and render behavior
- Contributor guidance in `AGENTS.md`
