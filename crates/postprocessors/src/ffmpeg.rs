use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{bail, Context};
use tokio::process::Command;
use tracing::{debug, info};

/// Wrapper around FFmpeg/FFprobe binaries.
#[derive(Debug, Clone)]
pub struct FFmpeg {
    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
}

impl FFmpeg {
    /// Auto-detect FFmpeg binary location.
    ///
    /// If `custom_location` is given it is treated as a directory containing
    /// `ffmpeg` (and optionally `ffprobe`).  Otherwise the system `PATH` is
    /// searched.
    pub fn new(custom_location: Option<&Path>) -> anyhow::Result<Self> {
        let (ffmpeg_path, ffprobe_path) = if let Some(dir) = custom_location {
            let ff = dir.join("ffmpeg");
            let fp = dir.join("ffprobe");
            if !ff.exists() {
                bail!("ffmpeg not found at {}", ff.display());
            }
            let fp = if fp.exists() { fp } else { which_bin("ffprobe")? };
            (ff, fp)
        } else {
            (which_bin("ffmpeg")?, which_bin("ffprobe")?)
        };

        info!(ffmpeg = %ffmpeg_path.display(), ffprobe = %ffprobe_path.display(), "located ffmpeg binaries");
        Ok(Self {
            ffmpeg_path,
            ffprobe_path,
        })
    }

    /// Get FFmpeg version string.
    pub async fn version(&self) -> anyhow::Result<String> {
        let output = Command::new(&self.ffmpeg_path)
            .args(["-version"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to run ffmpeg -version")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("unknown").to_string();
        Ok(first_line)
    }

    // -----------------------------------------------------------------
    // Core operations
    // -----------------------------------------------------------------

    /// Merge separate video and audio streams into a single container.
    ///
    /// This is the key operation for `bestvideo+bestaudio` format selection.
    pub async fn merge_streams(
        &self,
        video_path: &Path,
        audio_path: &Path,
        output_path: &Path,
        output_format: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut args: Vec<&str> = Vec::new();

        let video_str = path_str(video_path)?;
        let audio_str = path_str(audio_path)?;
        let output_str = path_str(output_path)?;

        args.extend(["-i", video_str, "-i", audio_str]);
        args.extend(["-c", "copy"]);
        args.extend(["-map", "0:v:0", "-map", "1:a:0"]);

        if let Some(fmt) = output_format {
            args.extend(["-f", fmt]);
        }

        args.push(output_str);
        self.run_ffmpeg(&args).await?;
        info!(output = %output_path.display(), "merged video and audio streams");
        Ok(())
    }

    /// Remux to a different container without re-encoding.
    pub async fn remux(
        &self,
        input_path: &Path,
        output_path: &Path,
        output_format: &str,
    ) -> anyhow::Result<()> {
        let input_str = path_str(input_path)?;
        let output_str = path_str(output_path)?;

        self.run_ffmpeg(&["-i", input_str, "-c", "copy", "-f", output_format, output_str])
            .await?;
        info!(output = %output_path.display(), format = output_format, "remuxed");
        Ok(())
    }

    /// Transcode to a different codec/format.
    pub async fn transcode(
        &self,
        input_path: &Path,
        output_path: &Path,
        video_codec: Option<&str>,
        audio_codec: Option<&str>,
        extra_args: &[&str],
    ) -> anyhow::Result<()> {
        let input_str = path_str(input_path)?;
        let output_str = path_str(output_path)?;

        let mut args: Vec<&str> = vec!["-i", input_str];

        if let Some(vc) = video_codec {
            args.extend(["-c:v", vc]);
        }
        if let Some(ac) = audio_codec {
            args.extend(["-c:a", ac]);
        }
        args.extend_from_slice(extra_args);
        args.push(output_str);

        self.run_ffmpeg(&args).await?;
        info!(output = %output_path.display(), "transcoded");
        Ok(())
    }

    /// Extract audio from a video file.
    pub async fn extract_audio(
        &self,
        input_path: &Path,
        output_path: &Path,
        audio_format: &str,
        audio_quality: Option<&str>,
    ) -> anyhow::Result<()> {
        let input_str = path_str(input_path)?;
        let output_str = path_str(output_path)?;

        let codec = audio_format_to_codec(audio_format);

        let mut args: Vec<&str> = vec!["-i", input_str, "-vn"];

        if codec == "copy" {
            args.extend(["-c:a", "copy"]);
        } else {
            args.extend(["-c:a", codec]);

            if let Some(q) = audio_quality {
                // If quality looks like a bitrate (e.g. "192K"), use -b:a.
                // Otherwise treat it as VBR quality level for -q:a.
                if q.ends_with('K') || q.ends_with('k') || q.contains("000") {
                    args.extend(["-b:a", q]);
                } else {
                    args.extend(["-q:a", q]);
                }
            }
        }

        args.push(output_str);
        self.run_ffmpeg(&args).await?;
        info!(output = %output_path.display(), format = audio_format, "extracted audio");
        Ok(())
    }

    /// Embed metadata tags into a media file.
    pub async fn embed_metadata(
        &self,
        input_path: &Path,
        output_path: &Path,
        metadata: &[(&str, &str)],
    ) -> anyhow::Result<()> {
        let input_str = path_str(input_path)?;
        let output_str = path_str(output_path)?;

        let mut args: Vec<String> = vec!["-i".into(), input_str.into(), "-c".into(), "copy".into()];

        for &(key, value) in metadata {
            args.push("-metadata".into());
            args.push(format!("{key}={value}"));
        }
        args.push(output_str.into());

        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_ffmpeg(&refs).await?;
        info!(output = %output_path.display(), "embedded metadata");
        Ok(())
    }

    /// Embed subtitle tracks into a video container.
    pub async fn embed_subtitles(
        &self,
        video_path: &Path,
        subtitle_paths: &[&Path],
        output_path: &Path,
    ) -> anyhow::Result<()> {
        let video_str = path_str(video_path)?;
        let output_str = path_str(output_path)?;

        let mut args: Vec<String> = vec!["-i".into(), video_str.into()];

        for sub in subtitle_paths {
            let s = path_str(sub)?;
            args.extend(["-i".into(), s.into()]);
        }

        args.extend(["-c".into(), "copy".into()]);

        // Determine subtitle codec from output extension.
        let sub_codec = match output_path.extension().and_then(|e| e.to_str()) {
            Some("mp4" | "m4v" | "mov") => "mov_text",
            _ => "srt",
        };
        args.extend(["-c:s".into(), sub_codec.into()]);

        // Map video stream, then each subtitle input.
        args.extend(["-map".into(), "0".into()]);
        for i in 0..subtitle_paths.len() {
            args.push("-map".into());
            args.push(format!("{}", i + 1));
        }

        args.push(output_str.into());

        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_ffmpeg(&refs).await?;
        info!(output = %output_path.display(), "embedded subtitles");
        Ok(())
    }

    /// Embed a thumbnail/cover art image.
    pub async fn embed_thumbnail(
        &self,
        media_path: &Path,
        thumbnail_path: &Path,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        let media_str = path_str(media_path)?;
        let thumb_str = path_str(thumbnail_path)?;
        let output_str = path_str(output_path)?;

        let ext = output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let args: Vec<&str> = match ext {
            "mp3" => vec![
                "-i", media_str, "-i", thumb_str, "-map", "0", "-map", "1", "-c", "copy",
                "-id3v2_version", "3", output_str,
            ],
            _ => vec![
                "-i", media_str, "-i", thumb_str, "-map", "0", "-map", "1", "-c", "copy",
                "-disposition:v:1", "attached_pic", output_str,
            ],
        };

        self.run_ffmpeg(&args).await?;
        info!(output = %output_path.display(), "embedded thumbnail");
        Ok(())
    }

    /// Embed chapter markers from a list of `(start_secs, end_secs, title)`.
    pub async fn embed_chapters(
        &self,
        input_path: &Path,
        output_path: &Path,
        chapters: &[(f64, Option<f64>, &str)],
    ) -> anyhow::Result<()> {
        // Write an FFMETADATA1 file.
        let mut meta = String::from(";FFMETADATA1\n");
        for (start, end, title) in chapters {
            meta.push_str("\n[CHAPTER]\n");
            meta.push_str("TIMEBASE=1/1000\n");
            meta.push_str(&format!("START={}\n", (*start * 1000.0) as i64));
            if let Some(e) = end {
                meta.push_str(&format!("END={}\n", (*e * 1000.0) as i64));
            }
            meta.push_str(&format!("title={title}\n"));
        }

        let meta_file = tempfile::Builder::new()
            .suffix(".ffmeta")
            .tempfile()
            .context("failed to create temp metadata file")?;
        std::fs::write(meta_file.path(), &meta)?;

        let input_str = path_str(input_path)?;
        let meta_str = path_str(meta_file.path())?;
        let output_str = path_str(output_path)?;

        self.run_ffmpeg(&[
            "-i", input_str, "-i", meta_str, "-map_metadata", "1", "-c", "copy", output_str,
        ])
        .await?;
        info!(output = %output_path.display(), count = chapters.len(), "embedded chapters");
        Ok(())
    }

    // -----------------------------------------------------------------
    // Probing
    // -----------------------------------------------------------------

    /// Probe a media file for format and stream information.
    pub async fn probe(&self, path: &Path) -> anyhow::Result<FFprobeResult> {
        let path_str = path_str(path)?;

        let output = Command::new(&self.ffprobe_path)
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
                path_str,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to run ffprobe")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("ffprobe failed: {stderr}");
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).context("failed to parse ffprobe JSON")?;

        let duration = json
            .pointer("/format/duration")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok());

        let format_name = json
            .pointer("/format/format_name")
            .and_then(|v| v.as_str())
            .map(String::from);

        let streams = json
            .get("streams")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|s| StreamInfo {
                        codec_type: s
                            .get("codec_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        codec_name: s
                            .get("codec_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        width: s.get("width").and_then(|v| v.as_u64()).map(|v| v as u32),
                        height: s.get("height").and_then(|v| v.as_u64()).map(|v| v as u32),
                        sample_rate: s
                            .get("sample_rate")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        channels: s
                            .get("channels")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u8),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(FFprobeResult {
            duration,
            format_name,
            streams,
        })
    }

    // -----------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------

    /// Run an FFmpeg command with standard flags (`-y`, `-hide_banner`,
    /// `-loglevel warning`).
    async fn run_ffmpeg(&self, args: &[&str]) -> anyhow::Result<()> {
        debug!(args = ?args, "running ffmpeg");
        let output = Command::new(&self.ffmpeg_path)
            .args(args)
            .args(["-y"])
            .args(["-hide_banner", "-loglevel", "warning"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("failed to run ffmpeg")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("ffmpeg failed (exit {}): {stderr}", output.status);
        }
        Ok(())
    }
}

// ------------------------------------------------------------------
// Public data types returned by probe
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FFprobeResult {
    pub duration: Option<f64>,
    pub format_name: Option<String>,
    pub streams: Vec<StreamInfo>,
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub codec_type: String,
    pub codec_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<String>,
    pub channels: Option<u8>,
}

// ------------------------------------------------------------------
// Free helpers
// ------------------------------------------------------------------

/// Locate a binary on `PATH`.
fn which_bin(name: &str) -> anyhow::Result<PathBuf> {
    // Simple PATH search without pulling in the `which` crate.
    if let Ok(val) = std::env::var("PATH") {
        for dir in val.split(':') {
            let candidate = PathBuf::from(dir).join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    bail!("{name} not found on PATH");
}

/// Convert a `&Path` to `&str`, failing with a nice message on non-UTF-8.
fn path_str(p: &Path) -> anyhow::Result<&str> {
    p.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", p.display()))
}

/// Map user-facing audio format names to FFmpeg codec names.
fn audio_format_to_codec(format: &str) -> &str {
    match format {
        "mp3" => "libmp3lame",
        "m4a" | "aac" => "aac",
        "opus" => "libopus",
        "vorbis" | "ogg" => "libvorbis",
        "flac" => "flac",
        "wav" => "pcm_s16le",
        "alac" => "alac",
        "best" | "copy" => "copy",
        _ => format, // pass through – let ffmpeg decide
    }
}
