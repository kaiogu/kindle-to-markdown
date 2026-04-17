# kindle-to-markdown

[![CI](https://github.com/kaiogu/kindle-to-markdown/actions/workflows/ci.yml/badge.svg)](https://github.com/kaiogu/kindle-to-markdown/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/rust-1.89%2B-f46623)
![CLI](https://img.shields.io/badge/interface-CLI-2f855a)
![License](https://img.shields.io/badge/license-Apache--2.0-blue)

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
cargo run -- sample_clippings.txt
```

Read from stdin and write Markdown to stdout:

```bash
cat sample_clippings.txt | cargo run --
```

Read from a connected Kindle:

```bash
cargo run -- --discover
```

Write one Markdown file per book:

```bash
cargo run -- sample_clippings.txt --layout by-book
```

Save the raw Kindle file alongside the export, keeping its original filename:

```bash
cargo run -- --discover --save-raw
```

Write a single Markdown file to an explicit path:

```bash
cargo run -- sample_clippings.txt --output my-notes/highlights.md
```

Suppress the default per-book statistics output:

```bash
cargo run -- sample_clippings.txt --no-stats
```

Build a release binary:

```bash
cargo build --release
./target/release/kindle-to-markdown "My Clippings.txt" --output highlights.md
```

## Input Rules

The CLI accepts exactly one input source:

- piped `stdin`
- a positional file path
- `--discover`

If more than one input source is present, it exits with an error instead of trying to merge them implicitly.

## Device Discovery

`--discover` looks for `My Clippings.txt` in the common device mount locations for:

- macOS: `/Volumes/<Kindle>/`
- Linux: `/run/media/<user>/...`, `/media/<user>/...`, and `/mnt/...`
- Windows: removable drive roots like `E:\`
- WSL: Windows drive mounts like `/mnt/e/`

It checks both the device root and `documents/My Clippings.txt`.

Defaults:

- positional file or `stdin`, `single` layout, no `--output`: write Markdown to `stdout`
- `--discover`, `single` layout, no `--output`: write `clippings/clippings.md`
- `by-book` layout, no `--output`: write multiple files under `clippings/`

If you pass `--save-raw`, the raw file keeps its original name:

- single-file export: next to the Markdown file
- directory export: inside the output directory

If `stdout` is the Markdown destination, the raw file is saved to `clippings/My Clippings.txt`.

The tool prints per-book counts by default to `stderr`. Use `--no-stats` to silence the statistics summary.

## Development Workflow

This repository ships with local and CI quality gates:

```bash
uv run --with pre-commit pre-commit install --hook-type pre-commit --hook-type commit-msg
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
uv run --with pre-commit pre-commit run --all-files
```

## Versioning and Releases

This project stays on `0.x` until the CLI surface feels stable. Release automation is driven by Conventional Commits and `release-plz`:

- `fix:` and `perf:` create a patch release.
- `feat:` also stays patch-level while the project is below `1.0.0`.
- `feat!:` or any `!` breaking change creates the next minor release in `0.x`.

Examples:

```text
fix: handle empty bookmark content
feat(cli): add --stdout alias
feat!: rename output headings
```

`release-plz` opens a release PR from commits on `main`, updates `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`, then creates a Git tag and GitHub release when that PR is merged. If you want the normal CI workflow to run on release PRs too, add a `RELEASE_PLZ_TOKEN` repository secret and give it `contents` and `pull requests` write access.

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
- Conventional commit enforcement for commit messages and PR titles
- Automated SemVer release PRs with `release-plz`
- Unit tests covering parse and render behavior
- Contributor guidance in `AGENTS.md`
- Permissive `Apache-2.0` license with an explicit patent grant
