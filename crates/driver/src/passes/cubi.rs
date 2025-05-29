use std::collections::HashMap;

use la_arena::Idx;
use lasso::Spur;
use telos_common::span::{Span, Spanned};

#[derive(Debug, Copy, Clone)]
struct Value {}
#[derive(Debug, Copy, Clone)]
struct Use {}

/// Bindings and their types in a scope
pub struct Bindings {
    m: HashMap<Spur, Value>,
}
impl Bindings {
    fn new() -> Self {
        Self { m: HashMap::new() }
    }
    fn get(&self, k: Spur) -> Option<Value> {
        self.m.get(&k).copied()
    }
    fn insert(&mut self, k: Spur, v: Value) {
        self.m.insert(k, v);
    }
    fn with_child_scope<T>(&mut self, lam: impl FnOnce(&mut Self) -> T) -> T {
        let mut scope = Bindings { m: self.m.clone() };
        lam(&mut scope)
    }
}

pub struct SumTyp {
    cases: HashMap<Spur, (Span, Spanned<Idx<Typ>>)>,
}
pub struct ProdTyp {
    fields: HashMap<Spur, (Span, Spanned<Idx<Typ>>)>,
}

/// Type declarations
pub struct Declarations {
    sum_types: HashMap<Spur, (Span, Spanned<SumTyp>)>,
    product_types: HashMap<Spur, (Span, Spanned<SumTyp>)>,
}

pub enum Typ {
    //
}
