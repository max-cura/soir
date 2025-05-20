use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// SOIR compiler.
#[derive(Debug, Parser)]
#[command(version, about, long_about, propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
#[command(about)]
pub enum Command {
    /// Build a SOIR program.
    Build(BuildArgs),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CodegenLanguage {
    Rust,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    /// SOIR source files. If none are specified, standard input will be used instead.
    pub input_files: Vec<PathBuf>,

    // -- codegen options --
    /// Compile-time support library/backend.
    #[arg(short = 'C', long = "cg-csl")]
    pub cg_csl: PathBuf,
    /// Target language to use for code generation.
    #[arg(short = 'l', long = "cg-lang", value_enum, default_value_t = CodegenLanguage::Rust)]
    pub cg_lang: CodegenLanguage,
    /// File to write generated output to.
    // XXX: default value should be dependent on `cg_lang`.
    #[arg(short = 'o', long = "output", default_value = "generated.rs")]
    pub cg_output_file: PathBuf,
}

pub fn run() {
    let cli = Cli::parse();

    tracing::debug!("Running with CLI arguments: {cli:?}");

    match cli.command {
        Command::Build(build) => crate::driver::build(build),
    }
}
