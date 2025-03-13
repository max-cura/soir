use std::collections::HashMap;
use crate::ast::{Constant, Id, Typ};

#[derive(Debug)]
pub enum Pat {
    Constant(Constant, Typ),
    Var(Id, Typ),
    Cons(Id, Vec<Pat>, Typ),
}
pub enum Atom {
    Constant(Constant, Typ),
    Var(Id, Typ),
    Lambda(Vec<Id>, ERef, Typ)
}
pub enum Val {
    App(Atom, Vec<Atom>, Typ),
    If(Atom, ERef, ERef, Typ),
    Match(Atom, Vec<(Pat, ERef)>, Typ)
}
pub enum Expr {
    LetA(Id, Atom, ERef, Typ),
    LetV(Id, Val, ERef, Typ),
    Atom(Atom),
    Val(Val),
}
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ERef(pub usize);

pub struct Anf {
    arena: Vec<Expr>,
    root: ERef,
    definitions: HashMap<Id, ERef>,
}
impl Anf {
    pub fn get(&self, eref: ERef) -> &Expr {
        &self.arena[eref.0]
    }
    pub fn find_definition(&self, id: Id) -> ERef {
        self.definitions.get(&id).map(|e| *e)
            .expect("no definition found")
    }
    pub fn root(&self) -> ERef {
        self.root
    }
}