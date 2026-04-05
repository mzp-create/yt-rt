# Architecture Decisions

## ADR-001: Workspace Structure
**Decision**: 7-crate workspace (core, cli, networking, extractors, downloaders, postprocessors, jsinterp)
**Rationale**: Clean separation of concerns, parallel compilation, independent testing. Core crate has zero async deps for fast compilation.

## ADR-002: JS Engine Choice — Boa
**Decision**: Use Boa (pure Rust) as primary JS engine
**Rationale**: No native dependencies, 94% ES conformance, embeddable. Fallback to rquickjs if gaps found.
**Risk**: Some complex YouTube JS may fail. Mitigation: test against real YouTube signatures.

## ADR-003: Extractor Plugin System — JS via Boa
**Decision**: Allow extractors to be written in JavaScript and executed via Boa
**Rationale**: yt-dlp's extractors change constantly (sites update). JS plugins allow rapid community updates without recompilation. Native Rust extractors for top 10 sites (performance), JS for the long tail.

## ADR-004: Async Runtime — Tokio
**Decision**: Use Tokio as the async runtime
**Rationale**: Industry standard, reqwest requires it, excellent ecosystem.

## ADR-005: FFmpeg Integration — Subprocess
**Decision**: Call FFmpeg as a subprocess, not link to libav*
**Rationale**: Matches yt-dlp's approach. Simpler distribution (no C deps). Users install FFmpeg separately.

## ADR-006: Error Handling — thiserror + anyhow
**Decision**: thiserror for library errors (core, networking), anyhow for application errors (cli)
**Rationale**: Standard Rust pattern. Library consumers get typed errors, CLI gets easy error chains.
