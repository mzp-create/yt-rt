use anyhow::Result;

mod app;
mod args;
mod completions;
mod manpage;
mod update;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = args::Cli::parse_args();
    app::run(cli).await
}
