use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Tracks which videos have been downloaded to avoid re-downloading.
/// Stores `extractor_key video_id` lines in a text file, matching yt-dlp's format.
pub struct DownloadArchive {
    path: PathBuf,
    entries: HashSet<String>,
}

impl DownloadArchive {
    /// Load or create an archive file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let mut entries = HashSet::new();
        if path.exists() {
            let file = std::fs::File::open(path)?;
            for line in std::io::BufReader::new(file).lines() {
                let line = line?;
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    entries.insert(trimmed);
                }
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            entries,
        })
    }

    /// Check if a video is already in the archive.
    pub fn contains(&self, extractor_key: &str, video_id: &str) -> bool {
        let key = format!("{} {}", extractor_key.to_lowercase(), video_id);
        self.entries.contains(&key)
    }

    /// Record a downloaded video in the archive.
    pub fn record(&mut self, extractor_key: &str, video_id: &str) -> anyhow::Result<()> {
        let key = format!("{} {}", extractor_key.to_lowercase(), video_id);
        if self.entries.insert(key.clone()) {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            writeln!(file, "{}", key)?;
        }
        Ok(())
    }

    /// Return the number of entries currently tracked.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if the archive has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.txt");
        let archive = DownloadArchive::load(&path).unwrap();
        assert!(archive.is_empty());
        assert_eq!(archive.len(), 0);
    }

    #[test]
    fn test_load_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.txt");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "youtube abc123").unwrap();
            writeln!(f, "vimeo def456").unwrap();
            writeln!(f, "").unwrap(); // blank line should be ignored
            writeln!(f, "  ").unwrap(); // whitespace-only should be ignored
        }
        let archive = DownloadArchive::load(&path).unwrap();
        assert_eq!(archive.len(), 2);
        assert!(archive.contains("youtube", "abc123"));
        assert!(archive.contains("YouTube", "abc123")); // case-insensitive extractor key
        assert!(archive.contains("vimeo", "def456"));
        assert!(!archive.contains("youtube", "xyz789"));
    }

    #[test]
    fn test_record_and_contains() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.txt");
        let mut archive = DownloadArchive::load(&path).unwrap();

        assert!(!archive.contains("youtube", "abc123"));
        archive.record("YouTube", "abc123").unwrap();
        assert!(archive.contains("youtube", "abc123"));
        assert_eq!(archive.len(), 1);

        // Recording the same entry again should not duplicate
        archive.record("YouTube", "abc123").unwrap();
        assert_eq!(archive.len(), 1);

        // Verify the file was written correctly
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.trim(), "youtube abc123");
    }

    #[test]
    fn test_record_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.txt");
        let mut archive = DownloadArchive::load(&path).unwrap();

        archive.record("YouTube", "vid1").unwrap();
        archive.record("Vimeo", "vid2").unwrap();
        archive.record("Twitch", "vid3").unwrap();
        assert_eq!(archive.len(), 3);

        // Reload from disk and verify persistence
        let reloaded = DownloadArchive::load(&path).unwrap();
        assert_eq!(reloaded.len(), 3);
        assert!(reloaded.contains("youtube", "vid1"));
        assert!(reloaded.contains("vimeo", "vid2"));
        assert!(reloaded.contains("twitch", "vid3"));
    }
}
