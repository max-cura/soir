//! This module implements the ALICE analysis pass.

use std::collections::{HashMap, HashSet};
use maplit::hashset;
use crate::ast::anf::{Anf, Atom, ERef, Expr, Val};
use crate::ast::Id;

pub struct Dep {
    computed: ERef,
    direct_dependencies: HashSet<Id>,
    proper_dependencies: HashSet<Id>,
}
pub struct DepGraph {
    arena: HashMap<ERef, Dep>,
}
impl DepGraph {
    fn new() -> Self {
        Self {
            arena: Default::default(),
        }
    }
    fn set(&mut self, eref: ERef, dep: Dep) {
        self.arena.insert(eref, dep);
    }
    fn get(&self, eref: ERef) -> Option<&Dep> {
        self.arena.get(&eref)
    }
}

fn find_atom_refs(atom: &Atom, anf: &Anf, dg: &mut DepGraph) -> HashSet<Id> {
    match atom {
        Atom::Constant(_, _) => hashset![],
        Atom::Var(id, _) => hashset![*id],
        Atom::Lambda(args, body, _) => {
            // find free variables
            visit(*body, anf, dg);
            dg
                .get(*body)
                .unwrap()
                .direct_dependencies
                .difference(&HashSet::from_iter(args.iter().copied()))
                .copied()
                .collect()
        }
    }
}

fn find_val_refs(val: &Val, anf: &Anf, dg: &mut DepGraph) -> HashSet<Id> {
    match val {
        Val::App(f, args, _) => {
            let x = find_atom_refs(f, anf, dg);
            args.iter().fold(x, |x, a| {
                x.union(&find_atom_refs(a, anf, dg)).copied().collect()
            })
        }
        Val::If(c, e1, e2, _) => {
            visit(*e1, anf, dg);
            visit(*e2, anf, dg);
            let e1_e2 = dg.get(*e1).unwrap().direct_dependencies
                .union(&dg.get(*e2).unwrap().direct_dependencies)
                .copied()
                .collect();
            find_atom_refs(c, anf, dg)
                .union(&e1_e2)
                .copied()
                .collect()
        }
        Val::Match(c, pats, _) => {
            let pat_deps = pats.iter().fold(HashSet::new(), |x, (_pat, e)| {
                visit(*e, anf, dg);
                x.union(&dg.get(*e).unwrap().direct_dependencies).copied().collect()
            });
            find_atom_refs(c, anf, dg)
                .union(&pat_deps)
                .copied()
                .collect()
        }
    }
}

fn visit(eref: ERef, anf: &Anf, dg: &mut DepGraph) {
    match anf.get(eref) {
        Expr::LetA(_, atom, body, _) => {
            let mut dd = HashSet::new();
            dd.extend(
                find_atom_refs(atom, anf, dg)
                    .into_iter());
            dg.set(eref, Dep {
                computed: eref,
                direct_dependencies: dd,
                proper_dependencies: HashSet::new(),
            });

            visit(*body, anf, dg);
        }
        Expr::LetV(_, val, body, _) => {
            let mut dd = HashSet::new();
            dd.extend(
                find_val_refs(val, anf, dg)
                    .into_iter());
            dg.set(eref, Dep {
                computed: eref,
                direct_dependencies: dd,
                proper_dependencies: HashSet::new(),
            });

            visit(*body, anf, dg);
        }
        Expr::Atom(atom) => {
            let mut dd = HashSet::new();
            dd.extend(
                find_atom_refs(atom, anf, dg)
                    .into_iter());
            dg.set(eref, Dep {
                computed: eref,
                direct_dependencies: dd,
                proper_dependencies: HashSet::new(),
            });
        }
        Expr::Val(val) => {
            let mut dd = HashSet::new();
            dd.extend(
                find_val_refs(val, anf, dg)
                    .into_iter());
            dg.set(eref, Dep {
                computed: eref,
                direct_dependencies: dd,
                proper_dependencies: HashSet::new(),
            });
        }
    }
}

fn find_deps(anf: &Anf, dg: &mut DepGraph) {
    visit(anf.root(), anf, dg);
}

pub fn run(
    input: &Anf
) {
    // step 1: go over all expressions, construct a dependency graph
    let mut dg = DepGraph::new();
    find_deps(input, &mut dg);
}