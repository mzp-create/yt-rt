use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

/// Generate shell completions for the given shell and write them to stdout.
pub fn generate_completions(shell: Shell) {
    let mut cmd = super::args::Cli::command();
    generate(shell, &mut cmd, "yt-dlp-rs", &mut io::stdout());
}
