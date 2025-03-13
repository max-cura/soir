use std::collections::HashMap;
use std::fmt::Debug;
use crate::ast::{Constant, Id};

pub use crate::ast::untyped::Pat;

#[derive(Debug, Clone)]
pub enum Atom<T: Debug + Clone> {
    Constant(Constant, T),
    Var(Id, T),
    Lambda(Vec<Id>, ERef, T)
}
impl<T: Debug + Clone> Atom<T> {
    pub fn map<U: Debug + Clone>(&self, x: impl Fn(&T) -> U) -> Atom<U> {
        match self {
            Atom::Constant(ct, t) => Atom::Constant(ct.clone(), x(t)),
            Atom::Var(id, t) => Atom::Var(*id, x(t)),
            Atom::Lambda(args, body, t) => Atom::Lambda(args.clone(), body.clone(), x(t)),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Val<T: Debug + Clone> {
    App(Atom<T>, Vec<Atom<T>>, T),
    If(Atom<T>, ERef, ERef, T),
    Match(Atom<T>, Vec<(Pat, ERef)>, T)
}
impl<T: Debug + Clone> Val<T> {
    pub fn map<U: Debug + Clone>(&self, x: impl Fn(&T) -> U) -> Val<U> {
        match self {
            Val::App(f, args, t) => {
                let f = f.map(x);
                let args = args.iter().map(|a| a.map(x)).collect();
                Val::App(f, args, x(t))
            },
            Val::If(c, e1, e2, t) => Val::If(c.map(x), e1.clone(), e2.clone(), x(t))
            Val::Match(s, arms, t) => {}
        }
    }
}
#[derive(Debug, Clone)]
pub enum Expr<T: Debug + Clone> {
    LetA(Id, Atom<T>, ERef, T),
    LetV(Id, Val<T>, ERef, T),
    Atom(Atom<T>),
    Val(Val<T>),
}
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ERef(usize);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Defn {
    /// It's possible for multiple lambdas to occur in a single expression, so we disambiguate.
    /// This only occurs in Val::App. We could probably drop the index, since we're more or less
    /// assuming unique naming.
    Lambda(ERef, usize),
    Let(ERef),
    Match(ERef),
}

#[derive(Debug)]
pub struct UntypedAnf<T: Debug + Clone> {
    pub arena: Vec<Expr<T>>,
    pub root: Option<ERef>,
    pub definitions: HashMap<Id, Defn>,
}
impl<T: Debug + Clone> UntypedAnf<T> {
    pub fn new() -> Self {
        Self {
            arena: vec![],
            root: None,
            definitions: Default::default(),
        }
    }

    pub fn push(&mut self, expr: Expr<T>) -> ERef {
        let index = self.arena.len();
        self.arena.push(expr);
        ERef(index)
    }
    pub fn define(&mut self, id: Id, defn: Defn) {
        assert!(
            self.definitions.insert(id, defn).is_none(),
            "Duplicate name definition"
        );
    }
    pub fn set_root(&mut self, root: ERef) {
        self.root = Some(root);
    }

    pub fn get(&self, id: ERef) -> &Expr<T> {
        self.arena.get(id.0).unwrap()
    }
}