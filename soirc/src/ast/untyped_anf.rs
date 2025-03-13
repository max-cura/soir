use crate::ast::{Constant, Id};

pub use crate::ast::untyped::Pat;

#[derive(Debug, Clone)]
pub enum Atom {
    Constant(Constant),
    Var(Id),
    Lambda(Vec<Id>, Box<Expr>)
}
#[derive(Debug, Clone)]
pub enum Val {
    App(Atom, Vec<Atom>),
    If(Atom, Box<Expr>, Box<Expr>),
    Match(Atom, Vec<(Pat, Expr)>)
}
#[derive(Debug, Clone)]
pub enum Expr {
    LetA(Id, Atom, Box<Expr>),
    LetV(Id, Val, Box<Expr>),
    Atom(Atom),
    Val(Val),
}

