//! Adapted from https://matt.might.net/articles/a-normalization/, which is adapted from
//! Flanagan et al.'s [The essence of compiling with continuations].
//!
//! [The essence of compiling with continuations]: https://dl.acm.org/doi/10.1145/173262.155113

use std::collections::VecDeque;
use std::convert::identity;
use std::rc::Rc;
use crate::ast::{Id, untyped_anf::{Expr as NExpr, Atom as NAtom, Val as NVal}, untyped::{Expr as AExpr}};

pub trait VarNamer {
    fn name_var(&self) -> Id;
}

fn norm_name(m: AExpr, namer: Rc<impl VarNamer + 'static>, k: Box<dyn FnOnce(NAtom) -> NExpr + 'static>) -> NExpr {
    norm(m, Rc::clone(&namer), Box::new(move |n| {
        match n {
            NExpr::Atom(a) => k (a),
            NExpr::Val(v) => {
                let name = namer.name_var();
                let e = k(NAtom::Var(name));
                NExpr::LetV(name, v, Box::new(e))
            }
            _ => panic!()
        }
    }))
}

fn norm(m: AExpr, namer: Rc<impl VarNamer + 'static>, k: Box<dyn FnOnce(NExpr) -> NExpr + 'static>) -> NExpr {
    match m {
        AExpr::Lambda(p, e1) => {
            k (NExpr::Atom(NAtom::Lambda(p, Box::new(norm(*e1, namer, Box::new(identity))))))
        }
        AExpr::Let(id, e1, e2) => {
            norm(*e1, Rc::clone(&namer), Box::new(move |e1| {
                match e1 {
                    NExpr::Atom(a) => {
                        NExpr::LetA(id, a, Box::new(norm(*e2, namer, k)))
                    }
                    NExpr::Val(v) => {
                        NExpr::LetV(id, v, Box::new(norm(*e2, namer, k)))
                    }
                    x => panic!("expected NAtom or NVal: {x:?}")
                }
            }))
        }
        AExpr::App(e1, args) => {
            let args = VecDeque::from(args);
            fn multi(mut args: VecDeque<AExpr>, mut atoms: Vec<NAtom>, namer: Rc<impl VarNamer + 'static>, k: Box<dyn FnOnce(Vec<NAtom>) -> NExpr + 'static>) -> NExpr {
                let hd = args.pop_front().expect("Apply should have at least one argument");
                norm_name(hd, Rc::clone(&namer), Box::new(move |hd| {
                    atoms.push(hd);
                    if args.is_empty() {
                        k (atoms)
                    } else {
                        multi(args, atoms, namer, k)
                    }
                }))
            }
            norm_name(*e1, Rc::clone(&namer), Box::new(move |e1| {
                multi(args,  vec![], namer, Box::new(move |args| {
                    k (NExpr::Val(NVal::App(e1, args)))
                }))
            }))
        }
        AExpr::Var(id) => {
            k (NExpr::Atom(NAtom::Var(id)))
        }
        AExpr::Constant(ct) => {
            k (NExpr::Atom(NAtom::Constant(ct)))
        }
        AExpr::If(e1, e2, e3) => {
            norm_name(*e1, Rc::clone(&namer), Box::new(move  |n| {
                k (NExpr::Val(NVal::If(n, Box::new(norm(*e2, Rc::clone(&namer), Box::new(identity))), Box::new(norm(*e3, namer, Box::new(identity))))))
            }))
        }
        AExpr::Match(e1, pats) => {
            norm_name(*e1, Rc::clone(&namer), Box::new(move |n| {
                k (NExpr::Val(NVal::Match(n, pats.into_iter()
                    .map(|(p, e)| {
                        (p, norm(e, Rc::clone(&namer), Box::new(identity)))
                    }).collect())))
            }))
        }
    }
}

pub fn normalize(program: AExpr, namer: impl VarNamer + 'static) -> NExpr {
    norm(program, Rc::new(namer), Box::new(identity))
}

