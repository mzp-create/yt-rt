//! Progress reporting traits and implementations.

use indicatif::{ProgressBar, ProgressStyle};

/// The state of a download in progress.
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub fragment_index: Option<u32>,
    pub fragment_count: Option<u32>,
    pub filename: String,
}

/// Trait for reporting progress during various stages of processing.
pub trait ProgressReporter: Send + Sync {
    fn report_download_progress(&self, state: &DownloadProgress);
    fn report_extraction_progress(&self, message: &str);
    fn report_postprocessing_progress(&self, message: &str);
    fn finish(&self);
}

/// Progress reporter that uses `indicatif` to render a terminal progress bar.
pub struct IndicatifReporter {
    bar: ProgressBar,
}

impl IndicatifReporter {
    pub fn new() -> Self {
        let bar = ProgressBar::new(0);
        bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] \
                     {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
                )
                .expect("valid progress bar template")
                .progress_chars("#>-"),
        );
        Self { bar }
    }
}

impl Default for IndicatifReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for IndicatifReporter {
    fn report_download_progress(&self, state: &DownloadProgress) {
        if let Some(total) = state.total_bytes {
            self.bar.set_length(total);
        }
        self.bar.set_position(state.downloaded_bytes);

        if let Some(frag_idx) = state.fragment_index {
            let frag_total = state
                .fragment_count
                .map(|c| format!("/{c}"))
                .unwrap_or_default();
            self.bar
                .set_message(format!("{} (frag {frag_idx}{frag_total})", state.filename));
        } else {
            self.bar.set_message(state.filename.clone());
        }
    }

    fn report_extraction_progress(&self, message: &str) {
        self.bar.set_message(message.to_string());
        self.bar.tick();
    }

    fn report_postprocessing_progress(&self, message: &str) {
        self.bar.set_message(message.to_string());
        self.bar.tick();
    }

    fn finish(&self) {
        self.bar.finish_with_message("done");
    }
}

/// A silent progress reporter that produces no output.
pub struct QuietReporter;

impl ProgressReporter for QuietReporter {
    fn report_download_progress(&self, _state: &DownloadProgress) {}
    fn report_extraction_progress(&self, _message: &str) {}
    fn report_postprocessing_progress(&self, _message: &str) {}
    fn finish(&self) {}
}
