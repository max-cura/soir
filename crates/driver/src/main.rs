#![feature(decl_macro)]
#![feature(exitcode_exit_method)]
#![feature(iterator_try_collect)]
#![feature(impl_trait_in_bindings)]
#![feature(iter_intersperse)]
#![feature(array_windows)]

pub mod cli;
pub mod codegen;
pub mod driver;
pub mod passes;

pub fn main() {
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .or_else(|_| tracing_subscriber::EnvFilter::try_new("info"))
                .unwrap(),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    cli::run();
}
