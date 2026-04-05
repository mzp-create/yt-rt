# yt-dlp-rs Development Guide

## Build & Test
- `cargo check` — fast type checking
- `cargo test` — run all tests  
- `cargo run -p yt-dlp-cli -- [args]` — run the binary
- `cargo clippy` — lint checks

## Architecture
7-crate workspace:
- `core` — types, config, format selection, output templates, progress, archive, filters
- `cli` — clap argument parsing, app orchestration, completions, update
- `networking` — HTTP client, cookies
- `extractors` — InfoExtractor trait, YouTube + 10 native extractors, JS plugin system, generic extractor
- `downloaders` — HTTP/HLS/DASH/fragment downloaders, rate limiter, retry, external delegation
- `postprocessors` — FFmpeg wrapper, audio extract, remux, metadata/subtitle/thumbnail/chapter embedding, SponsorBlock
- `jsinterp` — Boa JS engine wrapper for YouTube signature decryption

## Conventions
- Edition 2024 (Rust 2024)
- Async via tokio, object-safe traits via `Pin<Box<dyn Future>>`
- Error handling: `thiserror` for library errors, `anyhow` for application
- New extractors: implement `InfoExtractor` trait, register in `create_default_registry()`
- JS plugins go in `~/.config/yt-dlp-rs/plugins/extractors/`
