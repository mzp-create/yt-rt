# yt-dlp-rs

A fast, native Rust rewrite of [yt-dlp](https://github.com/yt-dlp/yt-dlp) -- download audio and video from thousands of sites.

## Features

- **Native extractors** for YouTube, Vimeo, Twitch, Twitter/X, Reddit, Instagram, SoundCloud, Bandcamp, Dailymotion, Bilibili, PeerTube, and more
- **Protocol support** for HTTP, HLS (m3u8), DASH (mpd), and fragmented downloads
- **Post-processing** via FFmpeg: audio extraction, remuxing, metadata/subtitle/thumbnail/chapter embedding, SponsorBlock segment removal
- **JavaScript plugin system** for writing custom extractors without recompiling
- **Generic extractor** fallback for sites without dedicated support
- **Output templates** compatible with yt-dlp's `-o` syntax
- **Download archive** to skip previously downloaded videos
- **Format selection** with yt-dlp-compatible `-f` filter expressions
- **Cookie support** from browser profiles and Netscape cookie files
- **Rate limiting**, retry logic, and resumable downloads
- **Cross-platform** -- Linux, macOS, and Windows

## Installation

### Pre-built binaries

Download the latest release for your platform from the [Releases](https://github.com/nicholasgasior/yt-dlp-rs/releases) page.

### From source via Cargo

```bash
cargo install yt-dlp-rs
```

### Build from source

```bash
git clone https://github.com/nicholasgasior/yt-dlp-rs.git
cd yt-dlp-rs
cargo build --release
# Binary is at target/release/yt-dlp-rs
```

## Quick start

```bash
# Download a video in best quality
yt-dlp-rs "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

# Download audio only as mp3
yt-dlp-rs -x --audio-format mp3 "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

# Select a specific format
yt-dlp-rs -f "bestvideo[height<=720]+bestaudio/best[height<=720]" URL

# List available formats
yt-dlp-rs -F URL

# Download with output template
yt-dlp-rs -o "%(title)s.%(ext)s" URL

# Download a playlist
yt-dlp-rs --yes-playlist "https://www.youtube.com/playlist?list=PLxxxxxxxx"

# Embed subtitles and metadata
yt-dlp-rs --embed-subs --embed-metadata URL

# Use cookies from your browser
yt-dlp-rs --cookies-from-browser firefox URL

# Skip previously downloaded videos
yt-dlp-rs --download-archive archive.txt URL
```

## Supported sites

yt-dlp-rs ships with native extractors for these sites:

| Site | Extractor |
|------|-----------|
| YouTube (videos, playlists, channels) | `youtube` |
| Vimeo | `vimeo` |
| Twitch (streams, VODs, clips) | `twitch` |
| Twitter / X | `twitter` |
| Reddit | `reddit` |
| Instagram | `instagram` |
| SoundCloud | `soundcloud` |
| Bandcamp | `bandcamp` |
| Dailymotion | `dailymotion` |
| Bilibili | `bilibili` |
| PeerTube | `peertube` |
| Generic (og:video, JSON-LD, etc.) | `generic` |

Additional sites can be supported through JavaScript plugins or the generic extractor.

## Architecture

yt-dlp-rs is organized as a 7-crate Cargo workspace:

```
crates/
  core/          -- types, config, format selection, output templates, progress, archive, filters
  cli/           -- clap argument parsing, app orchestration, completions, update
  networking/    -- HTTP client, cookies
  extractors/    -- InfoExtractor trait, native extractors, JS plugin system, generic extractor
  downloaders/   -- HTTP/HLS/DASH/fragment downloaders, rate limiter, retry, external delegation
  postprocessors/-- FFmpeg wrapper, audio extract, remux, metadata/subtitle/thumbnail/chapter embedding, SponsorBlock
  jsinterp/      -- Boa JS engine wrapper for YouTube signature decryption
```

## Plugin development

yt-dlp-rs supports JavaScript extractor plugins. Place `.js` files in:

```
~/.config/yt-dlp-rs/plugins/extractors/
```

A plugin exports a single extractor object:

```javascript
({
    name: "MyExtractor",
    suitable: function(url) {
        return /example\.com\/video\//.test(url);
    },
    extract: function(url) {
        // Fetch the page, parse video info, and return an object with:
        // { id, title, formats: [{ url, ext, ... }], ... }
    }
})
```

Plugins are loaded at startup and checked before the generic extractor.

## Requirements

- **FFmpeg** (optional) -- required for post-processing operations such as audio extraction, remuxing, and embedding metadata/subtitles/thumbnails

## License

This project is released under the [Unlicense](https://unlicense.org/). You are free to use, modify, and distribute it without restriction.
