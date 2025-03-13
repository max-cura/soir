use crate::ast::{Constant, Id};

#[derive(Debug, Clone)]
pub enum Pat {
    Constant(Constant),
    Var(Id),
    Cons(Id, Vec<Pat>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Let(Id, Box<Expr>, Box<Expr>),
    App(Box<Expr>, Vec<Expr>),
    Lambda(Vec<Id>, Box<Expr>),
    Var(Id),
    Constant(Constant),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<(Pat, Expr)>),
}
