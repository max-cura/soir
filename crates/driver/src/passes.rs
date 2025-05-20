//! Compiler passes that o transformations on the representation.
//!
//! Current list of passes;
//!  - binary operator parsing (precedence & fixity resolution)
//!  - k-normalization
//!  - inference

// base AST -> base AST
pub mod binop_parse;
// base AST -> KNAST
pub mod knf;
// KNAST -> typed KNAST
pub mod infer;
