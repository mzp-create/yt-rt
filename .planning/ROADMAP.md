# yt-dlp-rs Roadmap

## Milestone 1: Foundation & Core

### Phase 1: Core Types & CLI Foundation
**Goal**: Working binary that parses all CLI args, loads config, and prints help.

#### Tasks:
1. Core types crate (InfoDict, Format, Protocol, etc.) ✅
2. Error types with thiserror ✅
3. Config system (TOML-based) ✅
4. Format selection parser & evaluator ✅
5. Output template engine ✅
6. Progress reporting (indicatif) ✅
7. CLI argument parsing (clap) - all 14 option groups
8. Config file loading & CLI override merging
9. Application orchestration skeleton
10. Initial test suite

**Success criteria**: `yt-dlp-rs --help` shows all options, `yt-dlp-rs --version` works, config loads from file.

### Phase 2: Networking & Download Engine
**Goal**: Download files via HTTP with resume, rate limiting, cookies.

#### Tasks:
1. HTTP client wrapper (reqwest) with proxy, headers, cookies
2. Cookie file loading (Netscape format)
3. Browser cookie extraction (Chrome, Firefox, Safari)
4. HTTP downloader with resume and progress
5. Rate limiter
6. Retry logic with exponential backoff
7. HLS/m3u8 downloader (m3u8-rs)
8. DASH/MPD downloader (dash-mpd)
9. Fragment-based download manager
10. Concurrent segment downloading
11. WebSocket transport
12. External downloader delegation (aria2c, curl)

**Success criteria**: Can download a file from a direct URL with progress bar, resume on interruption, respect rate limits.

## Milestone 2: YouTube MVP

### Phase 3: JS Engine & YouTube Extractor
**Goal**: Extract and download YouTube videos.

#### Tasks:
1. Boa JS engine integration
2. YouTube signature decryption (nsig, sig)
3. YouTube player extraction
4. YouTube innertube API client
5. InfoExtractor trait implementation
6. ExtractorRegistry with URL matching
7. YouTube video extractor (single videos)
8. YouTube playlist extractor
9. YouTube channel/tab extractor
10. YouTube search extractor
11. Format selection integration
12. Output template integration
13. Age-gate and geo-restriction handling

**Success criteria**: `yt-dlp-rs "https://youtube.com/watch?v=..."` downloads a video with correct filename.

### Phase 4: Post-Processing & FFmpeg
**Goal**: Merge audio/video, extract audio, embed metadata.

#### Tasks:
1. FFmpeg binary detection and version checking
2. FFmpeg subprocess wrapper (async)
3. Audio/video stream merging
4. Audio extraction (-x)
5. Remuxing (container conversion)
6. Transcoding (codec conversion)
7. Metadata embedding (title, artist, etc.)
8. Subtitle embedding
9. Thumbnail embedding
10. Chapter embedding
11. SponsorBlock integration
12. File renaming/moving post-processor
13. Info JSON writing
14. Thumbnail downloading and conversion

**Success criteria**: `yt-dlp-rs -x --audio-format mp3 URL` extracts audio. `yt-dlp-rs -f "bestvideo+bestaudio" URL` merges streams.

## Milestone 3: Multi-Site & Polish

### Phase 5: Plugin System & Top 50 Extractors
**Goal**: Support 50+ sites with a JS plugin system.

#### Tasks:
1. JS-based extractor plugin API design
2. Plugin discovery (config dir scanning)
3. Plugin sandboxing (Boa context isolation)
4. Port top 10 extractors natively (YouTube, Twitter/X, Instagram, TikTok, Reddit, Twitch, Vimeo, Dailymotion, Bilibili, Facebook)
5. Generic extractor (og:video, twitter:player, etc.)
6. Port next 40 extractors as JS plugins
7. Playlist/channel pagination
8. Live stream support
9. DRM detection and warning

**Success criteria**: 50+ sites working, JS plugins load from disk, generic extractor catches simple embeds.

### Phase 6: Parity & Polish
**Goal**: Feature parity with yt-dlp for common use cases.

#### Tasks:
1. Download archive (--download-archive)
2. Match filters (--match-filter)
3. Date-based filtering
4. Netrc authentication
5. Client certificate support
6. Geo-restriction bypass helpers
7. Self-update mechanism
8. Shell completions (bash, zsh, fish)
9. Man page generation
10. Cross-platform binary builds (CI)
11. Comprehensive test suite
12. Performance benchmarks vs yt-dlp
13. Migration guide from yt-dlp

**Success criteria**: Common yt-dlp workflows work identically. Binary available for Linux/macOS/Windows.
