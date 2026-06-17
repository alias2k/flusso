//! `flusso build` — compile a config and its schemas into a single portable
//! artifact. Needs no database and no secret to be set.

use std::path::PathBuf;

use anyhow::Context;
use clap::Args;

use crate::commands::print;
use crate::{DEFAULT_CONFIG, DEFAULT_LOCK};

#[derive(Debug, Args)]
pub(crate) struct BuildArgs {
    /// Path to the configuration file.
    #[arg(short, long, env = "FLUSSO_CONFIG", default_value = DEFAULT_CONFIG)]
    config: PathBuf,

    /// Where to write the compiled artifact.
    #[arg(short, long, env = "FLUSSO_OUT", default_value = DEFAULT_LOCK)]
    out: PathBuf,
}

pub(crate) fn execute(args: BuildArgs) -> anyhow::Result<()> {
    let compiled = schema::compile(&args.config)
        .with_context(|| format!("compiling config from {}", args.config.display()))?;
    schema::write(&compiled, &args.out)
        .with_context(|| format!("writing compiled artifact to {}", args.out.display()))?;

    let mut out = std::io::stdout().lock();
    let pen = print::Pen::detect();
    print::success(
        &mut out,
        pen,
        &format!(
            "compiled {} index(es) → {}",
            compiled.config.indexes.len(),
            args.out.display()
        ),
    )?;
    Ok(())
}
