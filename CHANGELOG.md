# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/kaiogu/kindle-to-markdown/compare/v0.1.0...v0.1.1) - 2026-04-17

### Fixed

- *(deps)* refresh clap terminal support stack
- *(release)* normalize changelog header handling

## [0.1.0](https://github.com/kaiogu/kindle-to-markdown/releases/tag/v0.1.0) - 2026-04-17

### Added

- *(settings)* write default values in init config
- *(cli)* add explicit merge mode
- *(stats)* add totals and json modes
- *(config)* add init-config and path override
- *(export)* improve per-book slug generation
- *(cli)* add sorting and dedupe options
- *(config)* add xdg-compatible settings
- *(cli)* add copy-raw option
- *(cli)* support stdin and discover flag
- *(cli)* print clipping statistics
- *(cli)* preserve raw clippings filename
- *(cli)* add export layouts
- *(cli)* add Kindle pull command

### Fixed

- *(cli)* validate explicit raw copy paths
- *(render)* quote every line of multiline highlights
- *(sort)* parse kindle timestamps for date order
- *(cli)* treat explicit single output as file
- *(export)* group by-book output by title
- *(parser)* support two-line Kindle headers

### Other

- split pre-commit and pre-push hooks
- *(deps)* bump softprops/action-gh-release from 2 to 3
- *(deps)* fix dependabot commit prefixes
- *(release)* cancel superseded release-plz runs
- *(deps)* bump actions/checkout from 4 to 6
- *(deps)* align dependabot commit messages
- *(readme)* add install and release guidance
- *(cli)* improve help text and errors
- *(discover)* improve WSL troubleshooting
- *(readme)* split usage config and contributor sections
- *(release)* add GitHub release binaries
- *(deps)* refresh clap regex and anyhow
- *(parser)* add fixture matrix coverage
- *(cli)* add end-to-end binary coverage
- *(git)* ignore generated clippings
- add conventional release automation
- Rename default branch and add Apache license
- Polish README presentation
- Fix bookmark parsing and README workflow
- Set up CI and contributor workflow
