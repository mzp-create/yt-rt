use clap::CommandFactory;
use clap_mangen::Man;
use std::io;

/// Generate a man page (roff format) and write it to stdout.
pub fn generate_manpage() -> anyhow::Result<()> {
    let cmd = super::args::Cli::command();
    let man = Man::new(cmd);
    man.render(&mut io::stdout())?;
    Ok(())
}
