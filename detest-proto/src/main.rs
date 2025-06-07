#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_variables)]

use std::collections::VecDeque;

fn main() {
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .or_else(|_| tracing_subscriber::EnvFilter::try_new("info"))
                .unwrap(),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    let mut dispatch = VecDeque::from(vec![(0, Dispatch::Start(()))]);
    while let Some(item) = dispatch.pop_front() {
        dispatcher(item)
            .into_iter()
            .for_each(|item| dispatch.push_back(item));
    }

    tracing::error!("reached end of dispatch queue without exit!");
}

type AnnotatedDispatch = (usize, Dispatch);

type Start_Args = ();
type Fragment1_Args = ();
type Exit_Args = ();

#[derive(Debug)]
enum Dispatch {
    Start(Start_Args),
    Fragment1(Fragment1_Args),
    Exit(Exit_Args),
}

fn dispatcher((depth, dispatch): AnnotatedDispatch) -> Vec<AnnotatedDispatch> {
    tracing::debug!("Dispatching {dispatch:?}");
    let out = match dispatch {
        Dispatch::Start(args) => fragment0_start(args),
        Dispatch::Fragment1(args) => fragment1(args),
        Dispatch::Exit(args) => {
            tracing::info!("Computed exit value: {args:?} at max-depth={depth}");
            std::process::exit(0);
        }
    };
    out.into_iter()
        .map(|dispatch| (depth + 1, dispatch))
        .collect()
}

fn fragment0_start(args: Start_Args) -> Vec<Dispatch> {
    let computed = ();
    vec![Dispatch::Fragment1(computed)]
}

fn fragment1(args: Fragment1_Args) -> Vec<Dispatch> {
    let computed = ();
    vec![Dispatch::Exit(computed)]
}
