# yt-dlp-rs

A fast, native Rust rewrite of [yt-dlp](https://github.com/yt-dlp/yt-dlp) — download audio and video from YouTube and many other sites.

**14,000+ lines of Rust** | **134 tests** | **12 native extractors** | **JS plugin system**

---

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
- [Supported Sites](#supported-sites)
- [Format Selection](#format-selection)
- [Output Templates](#output-templates)
- [Post-Processing](#post-processing)
- [Cookies & Authentication](#cookies--authentication)
- [Download Archive & Filtering](#download-archive--filtering)
- [Plugin System](#plugin-system)
- [Configuration File](#configuration-file)
- [Shell Completions](#shell-completions)
- [Building from Source](#building-from-source)
- [Architecture](#architecture)
- [License](#license)

---

## Installation

### Pre-built Binaries

Download the latest release for your platform from the [Releases](https://github.com/user/yt-dlp-rs/releases) page:

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `yt-dlp-rs-linux-x86_64` |
| Linux ARM64 | `yt-dlp-rs-linux-aarch64` |
| macOS x86_64 | `yt-dlp-rs-darwin-x86_64` |
| macOS ARM64 (Apple Silicon) | `yt-dlp-rs-darwin-aarch64` |
| Windows x86_64 | `yt-dlp-rs-windows-x86_64.exe` |

```bash
# Example: Linux x86_64
curl -L -o yt-dlp-rs https://github.com/user/yt-dlp-rs/releases/latest/download/yt-dlp-rs-linux-x86_64
chmod +x yt-dlp-rs
sudo mv yt-dlp-rs /usr/local/bin/
```

### Install via Cargo

```bash
cargo install --git https://github.com/user/yt-dlp-rs.git
```

### Build from Source

```bash
git clone https://github.com/user/yt-dlp-rs.git
cd yt-dlp-rs
cargo build --release
# Binary is at target/release/yt-dlp-rs
```

### Requirements

- **FFmpeg** (optional but recommended) — needed for merging video+audio streams, audio extraction, remuxing, and embedding metadata/subtitles/thumbnails. Install via your package manager:
  ```bash
  # macOS
  brew install ffmpeg

  # Ubuntu/Debian
  sudo apt install ffmpeg

  # Windows (via Chocolatey)
  choco install ffmpeg
  ```

---

## Usage

### Basic Download

```bash
# Download a video in best available quality
yt-dlp-rs "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

# Download from any supported site
yt-dlp-rs "https://vimeo.com/123456789"
yt-dlp-rs "https://twitter.com/user/status/123456789"
yt-dlp-rs "https://www.reddit.com/r/videos/comments/abc123/title/"
```

### Audio Extraction

```bash
# Extract audio and convert to MP3
yt-dlp-rs -x --audio-format mp3 URL

# Extract audio as FLAC with best quality
yt-dlp-rs -x --audio-format flac --audio-quality 0 URL

# Supported audio formats: mp3, m4a, opus, flac, wav, ogg, aac
```

### Format Selection

```bash
# List all available formats (simulate, don't download)
yt-dlp-rs -s URL

# Download specific format by ID
yt-dlp-rs -f 137 URL

# Best video + best audio, merged by FFmpeg
yt-dlp-rs -f "bestvideo+bestaudio" URL

# Limit resolution to 720p
yt-dlp-rs -f "bestvideo[height<=720]+bestaudio/best[height<=720]" URL

# Prefer MP4 container
yt-dlp-rs -f "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]" URL

# Worst quality (smallest file)
yt-dlp-rs -f worst URL
```

### Output Templates

Control the filename with `-o`:

```bash
# Default: %(title)s [%(id)s].%(ext)s
yt-dlp-rs URL

# Custom filename
yt-dlp-rs -o "%(title)s.%(ext)s" URL

# Include uploader and upload date
yt-dlp-rs -o "%(uploader)s/%(upload_date)s - %(title)s.%(ext)s" URL

# Truncate long titles
yt-dlp-rs -o "%(title).50s [%(id)s].%(ext)s" URL

# Use default value for missing fields
yt-dlp-rs -o "%(artist|Unknown Artist)s - %(title)s.%(ext)s" URL

# Date formatting
yt-dlp-rs -o "%(upload_date>%Y-%m-%d)s %(title)s.%(ext)s" URL
```

Available template fields: `id`, `title`, `ext`, `uploader`, `uploader_id`, `channel`, `channel_id`, `upload_date`, `duration`, `view_count`, `like_count`, `description`, `playlist`, `playlist_index`, and more.

### Metadata & Embedding

```bash
# Embed metadata tags (title, artist, date, etc.)
yt-dlp-rs --embed-metadata URL

# Embed subtitles into the video container
yt-dlp-rs --write-subs --embed-subs URL

# Embed thumbnail as cover art
yt-dlp-rs --embed-thumbnail URL

# Embed chapter markers
yt-dlp-rs --embed-chapters URL

# All embedding options combined
yt-dlp-rs --embed-metadata --embed-subs --embed-thumbnail --embed-chapters URL

# Write info JSON alongside the video
yt-dlp-rs --write-info-json URL
```

### Subtitles

```bash
# Download subtitles
yt-dlp-rs --write-subs URL

# Download auto-generated subtitles
yt-dlp-rs --write-auto-subs URL

# Specify languages
yt-dlp-rs --write-subs --sub-langs "en,es,fr" URL

# Download all available subtitles
yt-dlp-rs --all-subs URL

# Choose subtitle format
yt-dlp-rs --write-subs --sub-format srt URL
```

### Thumbnails

```bash
# Save thumbnail image
yt-dlp-rs --write-thumbnail URL

# Save all thumbnail sizes
yt-dlp-rs --write-all-thumbnails URL

# Convert thumbnail format
yt-dlp-rs --write-thumbnail --convert-thumbnails jpg URL
```

### SponsorBlock

```bash
# Remove sponsor segments from YouTube videos
yt-dlp-rs --sponsorblock-remove sponsor URL

# Remove multiple segment types
yt-dlp-rs --sponsorblock-remove "sponsor,intro,outro,selfpromo" URL

# Mark segments as chapters instead of removing
yt-dlp-rs --sponsorblock-mark sponsor URL
```

### Playlists

```bash
# Download entire playlist
yt-dlp-rs --yes-playlist "https://www.youtube.com/playlist?list=PLxxxxxxxx"

# Download only specific items
yt-dlp-rs --playlist-items "1,3-5,7" URL

# Download range
yt-dlp-rs --playlist-start 3 --playlist-end 10 URL

# Download only the video, not the playlist
yt-dlp-rs --no-playlist URL
```

### Network Options

```bash
# Use a proxy
yt-dlp-rs --proxy "socks5://127.0.0.1:1080" URL

# Force IPv4
yt-dlp-rs -4 URL

# Set connection timeout (seconds)
yt-dlp-rs --socket-timeout 30 URL

# Limit download speed
yt-dlp-rs --limit-rate 1M URL

# Use an external downloader
yt-dlp-rs --external-downloader aria2c URL
```

### Simulation & Info

```bash
# Print video info as JSON (don't download)
yt-dlp-rs --dump-json URL

# Print specific fields
yt-dlp-rs --print "%(title)s" URL
yt-dlp-rs --print "%(id)s" --print "%(upload_date)s" URL

# Simulate (show formats without downloading)
yt-dlp-rs -s URL

# List all supported extractors
yt-dlp-rs --list-extractors
```

---

## Supported Sites

yt-dlp-rs includes 12 native extractors:

| Site | URL Patterns | Notes |
|------|-------------|-------|
| **YouTube** | `youtube.com/watch`, `youtu.be`, `/shorts/`, `/live/`, `/embed/` | Videos, playlists, channels, search. Signature decryption via Boa JS engine. |
| **Twitter / X** | `twitter.com/*/status/*`, `x.com/*/status/*` | Tweet videos via syndication API |
| **Reddit** | `reddit.com/r/*/comments/*`, `v.redd.it/*` | DASH + separate audio, MP4 fallback |
| **Vimeo** | `vimeo.com/*`, `player.vimeo.com/video/*` | Progressive, DASH, and HLS |
| **TikTok** | `tiktok.com/@*/video/*`, `vm.tiktok.com/*` | Watermarked and non-watermarked |
| **Instagram** | `instagram.com/p/*`, `/reel/*`, `/tv/*` | Posts, reels, IGTV |
| **Twitch** | `twitch.tv/videos/*`, `clips.twitch.tv/*` | VODs (HLS) and clips via GQL API |
| **Dailymotion** | `dailymotion.com/video/*`, `dai.ly/*` | HLS per quality level |
| **SoundCloud** | `soundcloud.com/*/*` | Audio: MP3, Opus. Playlists supported. |
| **Bilibili** | `bilibili.com/video/BV*`, `b23.tv/*` | DASH video+audio, FLV fallback |
| **Facebook** | `facebook.com/*/videos/*`, `fb.watch/*` | SD/HD variants |
| **Generic** | Any HTTP/HTTPS URL | Open Graph, oEmbed, Twitter cards, HTML5 `<video>`, iframe detection |

Additional sites can be supported through [JS plugins](#plugin-system).

---

## Format Selection

The `-f` flag accepts format strings compatible with yt-dlp:

| Expression | Meaning |
|-----------|---------|
| `best` | Best single file (video+audio combined) |
| `worst` | Worst quality single file |
| `bestvideo` | Best video-only stream |
| `bestaudio` | Best audio-only stream |
| `bestvideo+bestaudio` | Best video + best audio, merged by FFmpeg |
| `137` | Specific format by itag/ID |
| `bestvideo[height<=720]` | Best video at most 720p |
| `bestvideo[ext=mp4]` | Best video in MP4 container |
| `bestvideo[vcodec!=none]` | Best video with a video codec |
| `best/worst` | Try `best`, fall back to `worst` |
| `bestvideo+bestaudio/best` | Try merge, fall back to single file |

**Filter operators**: `=`, `!=`, `<`, `<=`, `>`, `>=`

**Filterable fields**: `height`, `width`, `ext`, `fps`, `vcodec`, `acodec`, `filesize`, `tbr`, `abr`, `vbr`

---

## Output Templates

Templates use `%(field)s` syntax:

| Template | Example Output |
|----------|---------------|
| `%(title)s.%(ext)s` | `My Video.mp4` |
| `%(id)s.%(ext)s` | `dQw4w9WgXcQ.mp4` |
| `%(uploader)s/%(title)s.%(ext)s` | `Rick Astley/Never Gonna Give You Up.mp4` |
| `%(title).30s.%(ext)s` | `Never Gonna Give You Up (tru.mp4` (truncated to 30 chars) |
| `%(upload_date>%Y-%m-%d)s.%(ext)s` | `2009-10-25.mp4` |
| `%(artist\|Unknown)s - %(title)s.%(ext)s` | `Unknown - My Video.mp4` (default value) |

---

## Post-Processing

Post-processing requires FFmpeg. Operations run in order after download:

| Flag | Action |
|------|--------|
| `-x` / `--extract-audio` | Extract audio track from video |
| `--audio-format FORMAT` | Convert audio to: mp3, m4a, opus, flac, wav, ogg, aac |
| `--audio-quality QUALITY` | Audio quality: 0 (best) to 10 (worst), or bitrate like `192K` |
| `--remux-video FORMAT` | Remux to: mp4, mkv, webm (no re-encoding) |
| `--recode-video FORMAT` | Re-encode to different format |
| `--embed-metadata` | Embed title, artist, date, description tags |
| `--embed-subs` | Embed subtitle tracks (mp4, mkv, webm) |
| `--embed-thumbnail` | Embed thumbnail as cover art |
| `--embed-chapters` | Embed chapter markers |
| `--sponsorblock-remove CATS` | Remove SponsorBlock segments |
| `--sponsorblock-mark CATS` | Mark SponsorBlock segments as chapters |
| `--exec CMD` | Run command after download (`{}` = filepath) |
| `--ffmpeg-location PATH` | Custom FFmpeg binary path |

---

## Cookies & Authentication

```bash
# Load cookies from a Netscape-format cookies.txt file
yt-dlp-rs --cookies cookies.txt URL

# Extract cookies from your browser
yt-dlp-rs --cookies-from-browser chrome URL
yt-dlp-rs --cookies-from-browser firefox URL

# Login with username/password
yt-dlp-rs -u USERNAME -p PASSWORD URL

# Use .netrc for authentication
yt-dlp-rs --netrc URL
```

---

## Download Archive & Filtering

### Download Archive

Keep track of downloaded videos to avoid re-downloading:

```bash
# Record downloads and skip already-downloaded videos
yt-dlp-rs --download-archive archive.txt URL

# Stop when encountering an already-downloaded video
yt-dlp-rs --download-archive archive.txt --break-on-existing URL
```

The archive file stores one `extractor_key video_id` per line.

### Video Filtering

```bash
# Only download videos matching a title pattern
yt-dlp-rs --match-title "tutorial" URL

# Skip videos matching a pattern
yt-dlp-rs --reject-title "trailer" URL

# Filter by upload date (YYYYMMDD)
yt-dlp-rs --dateafter 20240101 URL
yt-dlp-rs --datebefore 20241231 URL
yt-dlp-rs --date 20240615 URL

# Filter by field values
yt-dlp-rs --match-filter "duration > 60" URL
yt-dlp-rs --match-filter "view_count >= 1000" URL
yt-dlp-rs --match-filter "like_count > 100" URL

# Age restriction
yt-dlp-rs --age-limit 18 URL

# Stop after N downloads
yt-dlp-rs --max-downloads 5 URL

# File size limits
yt-dlp-rs --min-filesize 10M URL
yt-dlp-rs --max-filesize 500M URL
```

---

## Plugin System

yt-dlp-rs supports JavaScript extractor plugins executed via the [Boa](https://github.com/boa-dev/boa) JS engine.

### Plugin Location

Place `.js` files in:
```
~/.config/yt-dlp-rs/plugins/extractors/
```

Or set the `YT_DLP_RS_PLUGIN_PATH` environment variable:
```bash
export YT_DLP_RS_PLUGIN_PATH="/path/to/plugins:/another/path"
```

### Plugin Format

Each `.js` file must define these globals:

```javascript
var EXTRACTOR_NAME = "MySite";
var EXTRACTOR_KEY = "MySite";
var SUITABLE_URLS = [
    "https?://(?:www\\.)?mysite\\.com/video/([a-zA-Z0-9]+)"
];

function extract(url) {
    // Extract video info and return as JSON string
    var videoId = url.match(/video\/([a-zA-Z0-9]+)/)[1];
    return JSON.stringify({
        id: videoId,
        title: "Video " + videoId,
        ext: "mp4",
        extractor: "MySite",
        extractor_key: "MySite",
        formats: [{
            format_id: "best",
            url: "https://mysite.com/cdn/" + videoId + ".mp4",
            ext: "mp4"
        }]
    });
}
```

Plugins are loaded at startup and checked before the generic extractor.

---

## Configuration File

yt-dlp-rs reads a TOML configuration file from:
```
~/.config/yt-dlp-rs/config.toml
```

Example:

```toml
[general]
verbose = false
quiet = false
ignore_errors = true

[network]
proxy = "socks5://127.0.0.1:1080"
socket_timeout = 30

[download]
retries = 10
fragment_retries = 10
concurrent_fragments = 4

[output]
output_template = "%(title)s [%(id)s].%(ext)s"
restrict_filenames = true

[format_selection]
format = "bestvideo[height<=1080]+bestaudio/best"
prefer_free_formats = true
merge_output_format = "mkv"

[postprocessing]
embed_metadata = true
embed_chapters = true
ffmpeg_location = "/usr/local/bin/ffmpeg"

[subtitle]
write_subtitles = true
subtitle_languages = ["en"]
```

CLI flags override config file values.

---

## Shell Completions

Generate tab completions for your shell:

```bash
# Bash
yt-dlp-rs --generate-completions bash > ~/.local/share/bash-completion/completions/yt-dlp-rs

# Zsh
yt-dlp-rs --generate-completions zsh > ~/.zfunc/_yt-dlp-rs

# Fish
yt-dlp-rs --generate-completions fish > ~/.config/fish/completions/yt-dlp-rs.fish

# PowerShell
yt-dlp-rs --generate-completions powershell > yt-dlp-rs.ps1
```

---

## Building from Source

### Prerequisites

- Rust 1.85+ (edition 2024)
- Git

### Build

```bash
git clone https://github.com/user/yt-dlp-rs.git
cd yt-dlp-rs

# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run the binary
cargo run -p yt-dlp-cli -- --help
```

### Project Structure

```
yt-dlp-rs/
├── Cargo.toml              # Workspace root
├── crates/
│   ├── core/               # Types, config, format selection, templates, archive, filters
│   ├── cli/                # CLI args (clap), app orchestration, completions, update
│   ├── networking/         # HTTP client (reqwest), cookies, browser cookie extraction
│   ├── extractors/         # InfoExtractor trait, 12 native extractors, JS plugins, generic
│   ├── downloaders/        # HTTP/HLS/DASH downloaders, rate limiter, retry, fragments
│   ├── postprocessors/     # FFmpeg wrapper, audio/remux/metadata/subs/thumbnail/chapters
│   └── jsinterp/           # Boa JS engine for YouTube signature decryption
├── .github/workflows/      # CI and release automation
└── .planning/              # Architecture decisions and roadmap
```

---

## Architecture

```
URL
 │
 ▼
ExtractorRegistry ─── find_extractor(url)
 │                         │
 │   ┌─────────────────────┘
 ▼   ▼
InfoExtractor.extract(url) ──► InfoDict
 │                               │
 │   YouTube: innertube API      │ formats, metadata,
 │   + sig/nsig decryption       │ subtitles, thumbnails
 │                               │
 ▼                               ▼
format_selection ◄──── -f "bestvideo+bestaudio/best"
 │
 ▼
DownloadManager.download_format()
 │
 ├── HttpDownloader   (HTTP/HTTPS, resume, rate limit)
 ├── HlsDownloader    (m3u8, AES-128 decrypt)
 ├── DashDownloader   (MPD, SegmentTemplate)
 └── ExternalDownloader (aria2c, curl, wget)
 │
 ▼
PostProcessorChain.run_all()
 │
 ├── MergePostProcessor    (FFmpeg merge video+audio)
 ├── AudioExtractPP        (-x: extract audio)
 ├── RemuxPP               (container conversion)
 ├── MetadataEmbedPP       (title, artist, date tags)
 ├── SubtitleEmbedPP       (embed .srt/.ass/.vtt)
 ├── ThumbnailEmbedPP      (cover art)
 ├── ChapterEmbedPP        (chapter markers)
 ├── SponsorBlockPP        (remove/mark segments)
 ├── InfoJsonPP            (write .info.json)
 └── ExecPP                (run custom command)
 │
 ▼
Output file saved to disk
```

---

## License

This project is licensed under the [MIT License](LICENSE).
