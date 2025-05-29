//! Compiler passes that o transformations on the representation.
//!
//! Current list of passes;
//!  - binary operator parsing (precedence & fixity resolution)
//!  - k-normalization
//!  - inference

// Operator parsing
pub mod operators;

// Flow Assignment
pub mod flow;

// K-normalization
pub mod knf;

// Type inference
