# yt-dlp-rs: Rust Rewrite of yt-dlp

## Vision
A complete, high-performance Rust rewrite of yt-dlp — the world's most popular media downloader. Native binary with zero Python dependency, async I/O, and a plugin system for community-maintained extractors.

## Architecture
- **Workspace**: 7 crates (core, cli, networking, extractors, downloaders, postprocessors, jsinterp)
- **Async runtime**: Tokio
- **JS engine**: Boa (pure Rust, 94% ES conformance)
- **HTTP**: reqwest with cookie store, proxy, TLS impersonation
- **Plugin system**: JS-based extractors via Boa for rapid community updates

## Key Metrics
- ~450,000 LOC Python to rewrite
- 1,800+ site extractors (bulk of codebase)
- 14 CLI option groups, ~300 flags
- 18 download protocol handlers

## Milestones
1. Foundation & Core (Phases 1-2)
2. YouTube MVP (Phases 3-4) 
3. Multi-site & Plugins (Phases 5-6)

## Status
- [x] Research complete
- [x] Workspace initialized
- [ ] Phase 1: Core types & CLI
- [ ] Phase 2: Networking & download engine
- [ ] Phase 3: JS engine & YouTube extractor
- [ ] Phase 4: Post-processing & FFmpeg
- [ ] Phase 5: Plugin system & top 50 extractors
- [ ] Phase 6: Parity & polish
