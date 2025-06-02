//! Compiler passes that o transformations on the representation.
//!
//! Current list of passes;
//!  - binary operator parsing (precedence & fixity resolution)
//!  - k-normalization
//!  - inference

// Operator parsing
pub mod operators;

// Flow Assignment
// pub mod flow;
pub mod origin;
// pub mod origin_flow;

// K-normalization
pub mod knf;

// Type inference
