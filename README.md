# Building

Download `rustup` from https://rustup.rs.

Then run `cargo run --` from the root of the repository.
To see debugging output, set the environmental variable `RUST_LOG`, e.g.
```
RUST_LOG=debug cargo run --
```

To get help with the frontend's command-line arguments, give the `-h` or `--help` argument (this also works for subcommands).

**NOTE**: due to a quirk in the way the Rust build system (`cargo`) works, we need to put arguments for `telos` _after_ the `--`; arguments before it are treated as arguments to `cargo`.
If you properly installed `telos`, e.g. with `cargo install`, then the `--` would not be included.
