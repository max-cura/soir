# Building & Running

Download `rustup` from https://rustup.rs.

Then run `cargo run --bin telos-driver --` from the root of the repository.
To see debugging output, set the environmental variable `RUST_LOG`, e.g.
```
RUST_LOG=debug cargo run --bin telos-driver -- build --cg-csl null path/to/example.soir
```
> NOTE: `--cg-csl` sets the compilation support library for integrating the target database into the code generation process. Currently, since code generation is incomplete, it has no effect, so we pass `null` to it.

To get help with the frontend's command-line arguments, give the `-h` or `--help` argument (this also works for subcommands).

**NOTE**: due to a quirk in the way the Rust build system (`cargo`) works, we need to put arguments for `telos` _after_ the `--`; arguments before it are treated as arguments to `cargo`.
If you properly installed `telos`, e.g. with `cargo install`, then the `--` would not be included.

# Output

At the moment, code generation is not finished. However, we can still get output from various stages of the compilation process!

Available options are:
```
  --print-tokens           Prints out the internal representation of the tokens output by the lexer
  --print-initial-ast      Prints the initial AST output by the parser
  --debug-operator-parsing Prints the AST after parsing expressions involving operators
  --print-knf              Prints the AST after K-normalization
  --print-origins          Prints the K-normalized AST with annotations for origin and location of each value
```
Additionally, the partial code generator will produce a file named `graph.dot` in the current working directory that contains the value DAG of the input program. This can be made into an image with e.g.
```
dot -Tpng -o graph.png graph.dot
```

# Syntax and Grammar

The syntax of SOIR quite similar to Haskell with some mostly superficial elements from OCaml, with only some superficial differences; the main difference is in fact that SOIR does not support user-defined data types, etc. The "database backend" in the compiler frontend is expected to inject type information derived from some separate DDL that defines the structure of the database, unrelated to the transaction language.

A brief tour of SOIR's syntax:

```hs
-- This is a comment!
```

Here, we bind the name `leq` at the top level to a function that takes two parameters `a` and `b`, which evaluates to `__builtin_leq a b`, which is the application of the special builtin function `__builtin_leq` to `a` and `b`.
```hs
leq a b = __builtin_leq a b ;;
```
(note that we require a double semicolon `;;` after every top-level item).
Here, we (optionally) assert the type of the binding `leq`; its placement relative to the actual binding in the source is irrelevant.
```hs
leq :: Int -> Int -> Bool ;;
```
A handy feature of SOIR is easy definition of custom operators: this defines a left-associative infix operator `<=` with very low binding power that will be replaced with calls to the binding `leq`.
```hs
infixl 1 <= leq;
```
We can also declare unary operators: this defines a unary prefix operator `-` with high binding power that will be replaced with calls to `__builtin_negate` (i.e. the negation operator).
```hs
unaryl 12 - __builtin_negate ;;
```
Now that we've define some operators, we can do things like this:
```hs
precedence_works = -12 <= 12 ;; -- true!
```

We can also use arbitrary functions as infix operators by surrounding them with backticks, though they are non-associative:
```hs
let dot = \u v ->
  List.reduce Int.add (List.map (\pair -> pair->_0 * pair->_1) (List.zip u v))
  in vec_1 `dot` vec_2
```

SOIR doesn't allow expressions in the top-level, only bindings, type assertions, and operator declarations, so we declare a quick `let_demo` symbol, but really we just want to demonstrate `let`-expressions.
```hs
let_demo =
  -- we declare my_string, which can be used inside the expression after `in`
  let my_string = "Hello," in
    __builtin_append my_string " world!" ;;
```

SOIR has the classic `if` statement for control flow:
```hs
let is_even = if ()
```

As well as `match` expressions!
```hs
Option.unwrap_or x y =
  match x in
    | `Just x2 -> x2
    | `Nothing -> y ;;
```
(even though SOIR currently doesn't allow user-defined types, it has rich type system that allows algebraic data types)
This example demonstrate parametric polymorphism and also the syntax used for deconstructing sum types; the backtick ``` ` ``` is used to delimit type constructors; here we see a type constructor ``` `Just ```that carries some associated data, and a type constructor ``` `Nothing ``` which is a unit variant.
Note that `match` isn't limited to sum types: we can use it with any data types, for example:
```hs
match number in
  | 4 -> "it's exactly 4"
  | 2 -> "it's precisely 2"
  | x -> String.append "it's a... " (Int.to_string x)
```
(in fact, `if` expressions actually are lowered to `match` expressions at a fairly early point).

Since we've shown off sum types, we should also introduce field access for product types:
```hs
let db_row = DB.Keyspace.birthdays.Indices.by_user_id.get my_user_id in
  db_row->month
```

Finally, we also have anonymous functions:
```hs
let my_func = \a b -> a + b in my_func 1 2 -- result: 3
```
