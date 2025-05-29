use std::collections::HashSet;

use la_arena::{Arena, Idx};
use maplit::hashset;

trait TypVars {
    /// Retrieve all non-quantified (i.e. free) type variables in a type expression.
    fn free_type_vars(&self) -> HashSet<Idx<Typ>>;
}
/// Perform type substitution: [`subst`].
trait Subst {
    fn subst(&mut self, substitutions: &HashMap<Idx<Typ>, Idx<Typ>>);
}

pub struct TypCtx {
    arena: Arena<Typ>,
}

/// A type expression.
#[derive(Debug, Clone)]
pub enum Typ {
    Cons(Spur, Vec<Idx<Typ>>),
    Var(Spur),
}

/// A universally quantified type
struct Forall {
    pub vars: HashSet<Idx<Typ>>,
    pub typ: Typ,
}

/// An equality constraint on two type expressions.
struct Constraint(Idx<Typ>, Idx<Typ>);

impl TypVars for Typ {
    fn free_type_vars(&self, ctx: TypCtx) -> HashSet<Idx<Typ>> {
        match self {
            Typ::Cons(s, v) => v.type_vars(),
            Typ::Var(s) => hashset![*s],
        }
    }
}
impl TypVars for Forall {
    // tvs(forall <quantifiers>. type) = tvs(type) - quantifiers
    fn free_type_vars(&self) -> HashSet<Idx<Typ>> {
        self.typ
            .free_type_vars()
            .difference(&self.vars)
            .copied()
            .collect()
    }
}
impl TypVars for Constraint {
    fn free_type_vars(&self) -> HashSet<Idx<Typ>> {
        self.0
            .free_type_vars()
            .union(&self.1.free_type_vars())
            .copied()
            .collect()
    }
}
impl<T: TypVars, I> TypVars for I
where
    for<'a> &'a I: IntoIterator<Item = &'a T>,
{
    fn free_type_vars(&self) -> HashSet<Idx<Typ>> {
        self.into_iter()
            .map(TypVars::free_type_vars)
            .fold(hashset![], |a, b| a.union(&b).copied().collect())
    }
}
