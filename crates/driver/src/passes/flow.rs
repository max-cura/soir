//! Information we need:
//! - every expression has some set of other expressions upon which the value it computes depends
//! - every expression must be resolved to a particular _location_ where it will be run
//! - location is defined by the value resulting from an invocation of a boundary function
//! - when a variable with location crosses a boundary function, the produced location is
//!   _appended_ to its original location
//! - when a variable with location crosses a non-boundary function, the produced location is the same
//! - when multiple variables with locations cross a non-boundary function, the produced location is
//!   the intersection of all of the variables' locations
//! - finally, we perform a _resolution_ pass, which for every value, assigns it to a particular
//!   location in its ancestry (the most recent)
//! Analysis across function boundaries:
//! - we create a schema of what happens within the function; there are various possibilities:
//!    a. the function adds ancestry
//!    b. the function intersects ancestry
//!    c. (a) and (b) repeatedly in any order any number of times
//! On ancestry:
//! - 'add ancestry' composes and is commutative with itself
//! - 'intersect ancestry' composes with itself and is commutative with itself
//! - 'add ancestry' and 'intersect ancestry' compose with each other but DO NOT COMMUTE
//! Analysis of recursive functions:
//! - proper way would be to initialize the ancestry of the fixpoint to the ancestry universe and
//!   then narrow it until convergence
//! - however a convergence proof is needed, so we defer it for now
//! Errors that can be produced:
//! - assuming the program is well-formed, none

use std::collections::HashMap;

use la_arena::{Arena, Idx};
use lasso::Spur;
use telos_common::span::Spanned;

use super::knf::Ex;

#[derive(Debug)]
enum Loc {
    /// This location is derived from computation (e.g. a boundary function)
    Value(Spur),
}

#[derive(Debug, Default)]
struct Origin {
    inner: Vec<Loc>,
}

#[derive(Debug, Default)]
struct PlacementInfo {
    // Immediate set of expressions upon which the value of this expression is dependent
    immediate_deps: Vec<Spur>,
    // All of the locations which are guaranteed to be available to this expression when it is run
    origin: Origin,
    // The final decision for where this expression will be run
    location: Option<Loc>,
}

pub fn analyze_expr(expr: Spanned<Idx<Ex>>, ctx: Ctx) {
    match &ctx.arena[expr.inner] {
        Ex::Match { expr, arms } => todo!(),
        Ex::Let { def, body } => todo!(),
        Ex::LetRec { defs, body } => todo!(),
        Ex::Lam { params, body } => todo!(),
        Ex::Literal { literal } => todo!(),
        Ex::App { func, args } => todo!(),
        Ex::Field { expr, field } => todo!(),
        Ex::Var { name } => todo!(),
    }
}

pub struct Ctx {
    pub funcs: Bindings<FuncPlacement>,
    pub arena: Arena<Ex>,
}

// Functions have a type a -> b
// They also are members of the class of morphisms from origins to origins
// So e.g. (a, b, c) -> d will have #a, #b, #c, and #d will be defined in terms of this
// an origin expression can be one of three things:
//  #x . #y
//  #x & #y
//  (f #x #y ...)
// Important note: we do not allow projection, so .. -> (d, e) will have a single #(d, e) origin
// that is output
pub enum OriginExpr {
    Var(Spur),
    Comp(Vec<OriginExpr>),
    ///
    Isect(Vec<OriginExpr>),
    /// Origin is given by application of some parameter which has a function type
    /// Note that App(s, ...) implies Var(s)
    App(Spur, Vec<OriginExpr>),
}
pub struct FuncPlacement {
    pub in_origins: Vec<Spur>,
    pub expr: OriginExpr,
}

pub struct Bindings<T> {
    m: Vec<HashMap<Spur, T>>,
}
impl<T> Bindings<T> {
    pub fn insert(&mut self, spur: Spur, t: T) {
        self.m
            .last_mut()
            .expect("cannot insert into a `Bindings` with no scope")
            .insert(spur, t);
    }
    pub fn enter(&mut self) {
        self.m.push(HashMap::new());
    }
    pub fn exit(&mut self) {
        self.m.pop();
    }
    pub fn get(&self, spur: Spur) -> Option<&T> {
        self.m.iter().rev().find_map(|layer| layer.get(&spur))
    }
    pub fn get_mut(&mut self, spur: Spur) -> Option<&mut T> {
        self.m
            .iter_mut()
            .rev()
            .find_map(|layer| layer.get_mut(&spur))
    }
}
