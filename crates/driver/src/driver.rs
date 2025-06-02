use std::{io::IsTerminal, path::PathBuf, process::ExitCode, sync::Arc};

use la_arena::Arena;
use lasso::Rodeo;
use maplit::hashmap;
use miette::Diagnostic;
use thiserror::Error;

use crate::{cli::BuildArgs, passes::origin::Placement};
use telos_common::{
    source::{Source, SourceError, SourceId, Sources},
    span::Spanned,
};
use telos_parser::{lexer::Token, parser::Item};

/// Errors that can occur while reading [`Sources`] from inputs.
#[derive(Debug, Error, Diagnostic)]
pub enum InputError {
    /// Standard input is an interactive terminal.
    #[error("no input files provided, and standard input is an interactive terminal")]
    IsTerminal,

    /// Error occurred while reading from standard input.
    #[error("error occurred while reading from standard input: {0}")]
    StdinIo(#[source] std::io::Error),

    /// Error occurred while opening an input file.
    #[error("error occurred while opening input {0}: {1}")]
    Source(PathBuf, #[source] SourceError),
}

/// Gather a [`Sources`] from the input files in a [`Build`]. If no input files are specified, will
/// attempt to read from standard input.
///
/// # Errors
///
/// If input file paths are not valid UTF-8, don't exist, are not regular files, or cannot be read,
/// an error is returned. If no input files are specified and standard input is an interactive
/// terminal or can't be read from, an error is returned.
pub fn read_sources(build: &BuildArgs) -> Result<Sources, InputError> {
    let mut sources = Sources::new();
    if build.input_files.is_empty() {
        tracing::info!("No input files: reading from standard input");
        let stdin = std::io::stdin();
        if stdin.is_terminal() {
            return Err(InputError::IsTerminal);
        } else {
            sources.insert(Source::new(
                std::io::read_to_string(stdin).map_err(InputError::StdinIo)?,
                "<stdin>",
            ));
        }
    } else {
        for input_file in build.input_files.iter() {
            sources.insert(
                Source::from_path(input_file)
                    .map_err(|error| InputError::Source(input_file.clone(), error))?,
            );
        }
    }
    Ok(sources)
}

/// Run a build
pub fn build(build: BuildArgs) {
    tracing::debug!("Building with arguments: {build:?}");

    let sources = Arc::new(match read_sources(&build) {
        Ok(s) => s,
        Err(e) => {
            fail!(e, "Build failed:")
        }
    });

    let mut interner = Rodeo::new();

    // run lexing for all files
    let mut source_tokens: Vec<(SourceId, Vec<Spanned<Token>>)> = vec![];
    let mut lexer_errors = vec![];
    for source_id in sources.ids() {
        match telos_parser::lexer::lex_source(&sources, source_id, &mut interner) {
            Ok(tokens) => {
                source_tokens.push((source_id, tokens));
            }
            Err(errors) => {
                lexer_errors.extend_from_slice(&errors);
            }
        }
    }
    if !lexer_errors.is_empty() {
        let mut error_count = 0;
        for error in lexer_errors {
            eprintln!("{:?}", miette::Report::new(error));
            error_count += 1;
        }
        eprintln!("Encountered {error_count} errors.");
        eprintln!("Compilation failed.");
        fail()
    }

    if build.print_tokens {
        eprintln!();
        eprintln!("--print-tokens");
        eprintln!("{source_tokens:?}");
    }

    let mut expr_arena = Arena::new();
    let mut ty_arena = Arena::new();
    let mut parse_items: Vec<Spanned<Item>> = vec![];
    let mut parse_errors = vec![];
    for (source_id, tokens) in source_tokens {
        match telos_parser::parser::parse(
            &sources,
            source_id,
            &tokens,
            &mut interner,
            &mut expr_arena,
            &mut ty_arena,
        ) {
            Ok(items) => parse_items.extend(items),
            Err(errors) => parse_errors.extend_from_slice(&errors),
        }
    }
    if !parse_errors.is_empty() {
        dump_errors(parse_errors)
    }

    if build.print_initial_ast {
        eprintln!();
        eprintln!("--print-initial-ast");
        for item in &parse_items {
            let Item::Binding { name, params, body } = &item.inner else {
                continue;
            };
            eprintln!(
                "{} {} =",
                interner.resolve(&name),
                params
                    .iter()
                    .map(|p| interner.resolve(&p.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
            let mut o = String::new();
            telos_parser::parser::print_expr(body.inner, &expr_arena, &interner, &mut o, 1);
            eprintln!("{o}");
        }
    }

    // for (spur, str) in &interner {
    //     eprintln!("{spur:?} = {str}");
    // }

    if let Err(errors) = crate::passes::operators::resolve_all_operators(
        &parse_items,
        &interner,
        &sources,
        &mut expr_arena,
    ) {
        dump_errors(errors)
    }

    if build.debug_operator_parsing {
        eprintln!();
        eprintln!("--debug-operator-parsing");
        for item in &parse_items {
            let Item::Binding { name, params, body } = &item.inner else {
                continue;
            };
            eprintln!(
                "{} {} =",
                interner.resolve(&name),
                params
                    .iter()
                    .map(|p| interner.resolve(&p.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
            let mut o = String::new();
            telos_parser::parser::print_expr(body.inner, &expr_arena, &interner, &mut o, 1);
            eprintln!("{o}");
        }
    }

    // normalize
    let (k_items, mut interner, mut ex_arena) =
        crate::passes::knf::k_norm_items(parse_items, interner, expr_arena);

    if build.print_knf {
        eprintln!();
        eprintln!("--print-knf");
        for (_, name, params, body) in &k_items {
            eprintln!(
                "{} {} =",
                interner.resolve(&name),
                params
                    .iter()
                    .map(|p| interner.resolve(&p.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
            // let mut o = String::new();
            // crate::passes::knf::print_ex(*body, &ex_arena, &interner, &mut o, 1);
            // eprintln!("{o}");
            crate::passes::origin::PrintCtx {
                resolver: &interner,
                arena: &ex_arena,
                width: 120,
                ex_map: None,
            }
            .pretty_print(**body);
        }
    }

    let mut n = |n| interner.get_or_intern(n);
    let mut builtins = hashmap![];
    use crate::passes::origin::OriginExpr as OE;
    {
        let a = n("#badd0_lhs");
        let b = n("#badd0_rhs");
        builtins.insert(
            n("__builtin_add"),
            Placement {
                expr: OE::Isect(vec![OE::loc_var(a), OE::loc_var(b)]),
                quantifiers: vec![a, b],
            },
        );
    }
    {
        let k = n("#fread0_key");
        let g = n("fread");
        builtins.insert(
            n("__fake_read"),
            Placement {
                expr: OE::Concat(vec![OE::loc_var(k), OE::loc_gen(g)]),
                quantifiers: vec![k],
            },
        );
    }
    let (interner, f_map) = match crate::passes::origin::analyze_items(
        &k_items,
        interner,
        &mut ex_arena,
        &builtins,
        &sources,
    ) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("");
            eprintln!("Failed origin analysis:");
            eprintln!("{e:?}");
            fail();
        }
    };

    if build.print_origins {
        eprintln!();
        eprintln!("--print-origins");
        for (_, name, params, body) in k_items {
            eprintln!(
                "{} {} =",
                interner.resolve(&name),
                params
                    .iter()
                    .map(|p| interner.resolve(&p.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
            // let mut o = String::new();
            // crate::passes::knf::print_ex(*body, &ex_arena, &interner, &mut o, 1);
            // eprintln!("{o}");
            crate::passes::origin::PrintCtx {
                resolver: &interner,
                arena: &ex_arena,
                width: 120,
                ex_map: Some(&f_map),
            }
            .pretty_print(*body);
            eprintln!();
        }
    }
}

pub fn dump_errors<E: std::error::Error + Diagnostic + Send + Sync + 'static>(errors: Vec<E>) -> ! {
    let error_count = errors.len();
    for error in errors {
        eprintln!("{:?}", miette::Report::new(error));
    }
    eprintln!("Encountered {} errors.", error_count);
    eprintln!("Compilation failed.");
    fail()
}

/// Convenience method for printing a message, an error, and then exiting the current process with
/// a non-succesful exit code.
pub macro fail($e:expr, $fmt:literal$(, $($args:tt)*)?) {
    {
        ::std::eprintln!($fmt, $($args)*);
        ::std::eprintln!("{:?}", miette::Report::new($e));
        $crate::driver::fail()
    }
}

/// Convenience for exiting the current process with a non-successful exit code.
pub fn fail() -> ! {
    ExitCode::FAILURE.exit_process()
}
