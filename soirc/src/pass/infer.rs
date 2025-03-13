//! Based on https://blog.stimsina.com/post/implementing-a-hindley-milner-type-system-part-2

use std::collections::{HashMap, HashSet};
use crate::ast::{flat_anf, Constant, TypId};

use maplit::hashset;
use crate::ast::flat_anf::{Pat, Atom, Val, Expr, ERef, UntypedAnf};

trait Tvs {
    fn tvs(&self) -> HashSet<TypId>;
}
trait Substitutable {
    fn subst(&mut self, substitutions: &HashMap<TypId, Typ>);
}

#[derive(Debug, Clone)]
pub enum Typ {
    Cons(TypId, Vec<Typ>),
    Var(TypId),
}

impl Tvs for Typ {
    /// Find free type variables
    fn tvs(&self) -> HashSet<TypId> {
        match self {
            Typ::Cons(_, v) => v.tvs(),
            Typ::Var(v) => hashset![*v],
        }
    }
}
impl Substitutable for Typ {
    fn subst(&mut self, substitutions: &HashMap<TypId, Typ>) {
        match self {
            Typ::Var(v) => {
                if let Some(f) = substitutions.get(&v) {
                    *self = f.clone();
                }
            }
            Typ::Cons(_, v) => v.subst(substitutions),
        }
    }
}

/// A universally quantified type
struct Forall {
    pub vars: HashSet<TypId>,
    pub typ: Typ,
}

impl Tvs for Forall {
    fn tvs(&self) -> HashSet<TypId> {
        self.typ.tvs().difference(&self.vars).copied().collect()
    }
}
impl Substitutable for Forall {
    fn subst(&mut self, substitutions: &HashMap<TypId, Typ>) {
        let mut s2 = substitutions.clone();
        s2.retain(|k, _| !self.vars.contains(k));
        self.typ.subst(&s2)
    }
}

/// An equality constraint on two types
struct Constraint(Typ, Typ);
impl Tvs for Constraint {
    fn tvs(&self) -> HashSet<TypId> {
        self.0.tvs().union(&self.1.tvs()).copied().collect()
    }
}
impl Substitutable for Constraint {
    fn subst(&mut self, substitutions: &HashMap<TypId, Typ>) {
        self.0.subst(substitutions);
        self.1.subst(substitutions);
    }
}

impl<T: Tvs, I> Tvs for I
    where
        for<'a> &'a I: IntoIterator<Item=&'a T>,
{
    fn tvs(&self) -> HashSet<TypId> {
        self.into_iter().map(Tvs::tvs).fold(hashset![], |a, b| a.union(&b).copied().collect())
    }
}
impl<T: Substitutable, I> Substitutable for I
    where
        for<'a> &'a mut I: IntoIterator<Item=&'a mut T>,
{
    fn subst(&mut self, substitutions: &HashMap<TypId, Typ>) {
        self.into_iter().for_each(|t| t.subst(substitutions));
    }
}

pub struct AlgW {
    context: HashMap<TypId, Forall>,
    tvar_counter: usize,
}

impl AlgW {
    fn new() -> Self {
        Self {
            context: Default::default(),
            tvar_counter: 0,
        }
    }
    fn new_tvar(&mut self) -> TypId {
        let id = self.tvar_counter;
        self.tvar_counter += 1;
        TypId::new(format!("@{id}"))
    }
    fn generalize(&self, typ: &Typ) -> Forall {
        Forall {
            vars: typ.tvs().difference(
                &self.context.values().map(Tvs::tvs).fold(hashset![], |a, b| a.union(&b).copied().collect())
            ).copied().collect(),
            typ: typ.clone(),
        }
    }
    fn instantiate(&mut self, Forall { vars, mut typ }: Forall) -> Typ {
        let subst = HashMap::from_iter(vars.into_iter().map(|v| (v, Typ::Var(self.new_tvar()))));
        typ.subst(&subst);
        typ
    }
}

impl AlgW {
    fn infer_vars(&mut self, atom: &Atom<()>, anf: UntypedAnf<()>, anf2: &mut UntypedAnf<Typ>) -> Atom<Typ> {
        match atom {
            Atom::Constant(ct, _) => match ct {
                Constant::Bool(_) => atom.with()
                Constant::Int(_) => {}
                Constant::Unit => {}
            }
            Atom::Var(_, _) => {}
            Atom::Lambda(_, _, _) => {}
        }
    }
    fn infer_expr(&mut self, expr: ERef, anf: UntypedAnf<()>, anf2: &mut UntypedAnf<Typ>) {
        match anf.get(expr) {
            Expr::LetA(_, _, _, _) => {}
            Expr::LetV(_, _, _, _) => {}
            Expr::Atom(atom) => {
                self.infer_atom(atom, anf)
            }
            Expr::Val(_) => {}
        }
    }
}

pub fn run(
    input: flat_anf::UntypedAnf<()>
) {
    todo!()
}