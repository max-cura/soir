# Building

Download `rustup` from https://rustup.rs.

Then run `cargo run --` from the root of the repository.
To see debugging output, set the environmental variable `RUST_LOG`, e.g.
```
RUST_LOG=debug cargo run -- --cg-csl null path/to/example.soir
```

To get help with the frontend's command-line arguments, give the `-h` or `--help` argument (this also works for subcommands).

**NOTE**: due to a quirk in the way the Rust build system (`cargo`) works, we need to put arguments for `telos` _after_ the `--`; arguments before it are treated as arguments to `cargo`.
If you properly installed `telos`, e.g. with `cargo install`, then the `--` would not be included.

# Syntax

A fairly standard functional language with algebraic data types.

Bindings can be done with `let name = value in expr`, or alternately at the global level with `name = value`.

Functions are introduced with
```hs
\a0 a1 ... aN -> body
```
or alternatively
```hs
func_name a0 a1 ... aN = body
```
(which desugars to ```hs func-name = \a0 a1 ... aN = body```).

Function calling is the default operator, so `f a b (c d)` is equivalent to `f(a, b, c(d))` in a C-family language.

Argument evaluation is left-to-right, and the language as a whole is eagerly evaluated.

Sum type deconstruction can be done via `match` statements:
```ml
Option.unwrap_or x y =
  match x in
    | Just x2 -> x2
    | Nothing -> y
;;
```
Product type deconstruction can be done via `Type.field` notation.
