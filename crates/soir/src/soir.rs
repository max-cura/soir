//! SOIR AST.

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Id(pub lasso::Spur);
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct StrId(pub lasso::Spur);

#[derive(Debug)]
pub enum Constant {
    Bool(bool),
    Int(i64),
    Str(StrId),
    Byte(u8),
    Nil,
    Unit,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Typ {
    TCon(Id, Vec<Typ>),
    TVar(Id),
}

pub mod untyped {
    use super::{Constant, Id};

    pub enum Pat {
        Constant(Constant),
        Var(Id),
        Cons(Id, Box<Pat>),
    }

    pub enum Expr {
        Let(Id, Box<Expr>, Box<Expr>),
        Apply(Box<Expr>, Vec<Expr>),
        Lambda(Vec<Id>, Box<Expr>),
        Var(Id),
        Constant(Constant),
        If(Box<Expr>, Box<Expr>, Box<Expr>),
        Match(Box<Expr>, Vec<(Pat, Expr)>),
    }
}

pub mod infer {
    use std::collections::HashMap;

    use lasso::Interner;
    use lasso::Rodeo;

    use super::untyped::*;
    use super::typed::*;
    use super::{Constant, Id, Typ};

    pub struct TypCtx {
        type_vars: HashMap<Id, Option<Typ>>,
        tv_ct: usize,
        type_rodeo: Rodeo,
    }
    impl TypCtx {
        pub fn new() -> Self {
            let mut this = Self {
                type_vars: HashMap::new(),
                tv_ct: 0,
                type_rodeo: Rodeo::new()
            };
            this
        }
        pub fn new_tcon(&mut self, s: &str) -> Id {
            let id = Id(self.type_rodeo.get_or_intern(format!("c${s}")));
            id
        }
        pub fn constant_tcon(&mut self, c: &Constant) -> Typ {
            match c {
                Constant::Bool(_) => Typ::TCon(self.new_tcon("Bool"), vec![]),
                Constant::Int(_) => Typ::TCon(self.new_tcon("Int"), vec![]),
                Constant::Str(_) => Typ::TCon(self.new_tcon("Str"), vec![]),
                Constant::Byte(_) => Typ::TCon(self.new_tcon("Byte"), vec![]),
                Constant::Nil => Typ::TCon(self.new_tcon("List"), vec![Typ::TVar(self.new_tvar())]),
                Constant::Unit => Typ::TCon(self.new_tcon("Unit"), vec![]),
            }
        }
        pub fn new_tvar(&mut self) -> Id {
            let id = Id(self.type_rodeo.get_or_intern(format!("v$t{}", self.tv_ct)));
            self.tv_ct += 1;
            id
        }
        pub fn specialize(&mut self, id: Id, typ: Typ) {
            self.type_vars.get_mut(&id).unwrap().replace(typ);
        }
        pub fn tvar(&self, id: Id) -> &Option<Typ> {
            self.type_vars.get(&id).unwrap()
        }
    }

    pub struct VarCtx {
        scopes: Vec<HashMap<Id, Typ>>,
    }
    impl VarCtx {
        pub fn enter(&mut self) {
            self.scopes.push(HashMap::new());
        }
        pub fn exit(&mut self) {
            self.scopes.pop();
        }
        pub fn get(&self, id: Id) -> Option<&Typ> {
            for scope in self.scopes.iter().rev() {
                if let Some(t) = scope.get(&id) {
                    return Some(t)
                }
            }
            None
        }
        pub fn insert(&mut self, id: Id, typ: Typ) {
            self.scopes.last_mut().unwrap().insert(id, typ);
        }
    }

    fn infer(e: Expr, tctx: &mut TypCtx, vctx: &mut VarCtx) -> AExpr {
        match e {
            Expr::Let(id, e0, e1) => {
                vctx.enter();
                let e0 = infer(*e0, tctx, vctx);
                vctx.set(id, e0.typ());
                let e1 = infer(*e1, tctx, vctx);
                let typ = e1.typ().clone();
                vctx.exit();
                AExpr::Let(id, Box::new(e0), Box::new(e1), typ)
            },
            Expr::Lambda(args, e) => {
                let mut argt : Vec<Typ> = args.iter().map(|_| tctx.new_tvar()).map(Typ::TVar).collect();
                vctx.enter();
                for (arg, t) in args.iter().zip(&argt) {
                    tctx.set(*arg, t)
                }
                let e = infer(*e, tctx, vctx);
                argt.push(e.typ().clone());
                let typ = Typ::TCon(tctx.new_tcon("Fn"), argt);
                vctx.exit();

                AExpr::Lambda(args, Box::new(e), typ)
            },
            Expr::Apply(e, vec) => todo!(),
            Expr::Var(id) => {
                let t = vctx.get(id).expect("name not found");
                AExpr::Var(id, t.clone())
            },
            Expr::Constant(ct) => {
                let t = tctx.constant_tcon(&ct);
                AExpr::Constant(ct, t)
            },
            Expr::If(e0, e1, e2) => {

            },
            Expr::Match(e, vec) => todo!(),
        }
        todo!()
    }
}

pub mod typed {
    use super::{Id, Constant, Typ};

    #[derive(Debug)]
    pub enum APat {
        Constant(Constant, Typ),
        Var(Id, Typ),
        Cons(Id, Box<APat>, Typ),
    }

    #[derive(Debug)]
    pub enum AExpr {
        Let(Id, Box<AExpr>, Box<AExpr>, Typ),
        Apply(Box<AExpr>, Vec<AExpr>, Typ),
        Lambda(Vec<Id>, Box<AExpr>, Typ),
        Var(Id, Typ),
        Constant(Constant, Typ),
        If(Box<AExpr>, Box<AExpr>, Box<AExpr>, Typ),
        Match(Box<AExpr>, Vec<(APat, AExpr)>, Typ),
    }
    impl AExpr {
        pub fn typ(&self) -> &Typ {
            match self {
                AExpr::Let(_, _, _, t)
                | AExpr::Apply(_, _, t)
                | AExpr::Lambda(_, _, t)
                | AExpr::Var(_, t)
                | AExpr::Constant(_, t)
                | AExpr::If(_, _, _, t)
                | AExpr::Match(_, _, t) => t,
            }
        }
    }
}

pub mod normalize {
    //! Adapted from https://matt.might.net/articles/a-normalization/, which is adapted from
    //! Flanagan et al.'s [The essence of compiling with continuations].
    //!
    //! [The essence of compiling with continuations]: https://dl.acm.org/doi/10.1145/173262.155113

    use std::collections::VecDeque;
    use std::convert::identity;

    use super::typed::*;
    use super::anf::*;
    use super::Id;

    pub trait VarNamer {
        fn name_var(&self) -> Id;
    }

    fn norm_name(m: AExpr, namer: &impl VarNamer, k: impl FnOnce(NAtom) -> NExpr) -> NExpr {
        norm(m, namer, move |n| {
            match n {
                NExpr::Atom(a) => k (a),
                NExpr::Val(v) => {
                    let name = namer.name_var();
                    let typ = v.typ().clone();
                    let e = k(NAtom::Var(name, typ));
                    let e_typ = e.typ().clone();
                    NExpr::LetV(name, v, Box::new(e), e_typ)
                }
                _ => panic!()
            }
        })
    }

    fn norm(m: AExpr, namer: &impl VarNamer, k: impl FnOnce(NExpr) -> NExpr) -> NExpr {
        match m {
            AExpr::Lambda(p, e1, typ) => {
                k (NExpr::Atom(NAtom::Lambda(p, Box::new(norm(*e1, namer, identity)), typ)))
            }
            AExpr::Let(id, e1, e2, typ) => {
                norm(*e1, namer, move |e1| {
                    match e1 {
                        NExpr::Atom(a) => {
                            NExpr::LetA(id, a, Box::new(norm(*e2, namer, k)), typ)
                        }
                        NExpr::Val(v) => {
                            NExpr::LetV(id, v, Box::new(norm(*e2, namer, k)), typ)
                        }
                        x => panic!("expected NAtom or NVal: {x:?}")
                    }
                })
            }
            AExpr::Apply(e1, args, typ) => {
                let args = VecDeque::from(args);
                fn multi(mut args: VecDeque<AExpr>, mut atoms: Vec<NAtom>, namer: &impl VarNamer, k: impl FnOnce(Vec<NAtom>) -> NExpr) -> NExpr {
                    let hd = args.pop_front().expect("Apply should have at least one argument");
                    norm_name(hd, namer, move |hd| {
                        atoms.push(hd);
                        if args.is_empty() {
                            k (atoms)
                        } else {
                            multi(args, atoms, namer, k)
                        }
                    })
                }
                norm_name(*e1, namer, move |e1| {
                    multi(args,  vec![], namer, move |args| {
                        k (NExpr::Val(NVal::Apply(e1, args, typ)))
                    })
                })
            }
            AExpr::Var(id, typ) => {
                k (NExpr::Atom(NAtom::Var(id, typ)))
            }
            AExpr::Constant(ct, typ) => {
                k (NExpr::Atom(NAtom::Constant(ct, typ)))
            }
            AExpr::If(e1, e2, e3, typ) => {
                norm_name(*e1, namer, move  |n| {
                    k (NExpr::Val(NVal::If(n, Box::new(norm(*e2, namer, identity)), Box::new(norm(*e3, namer, identity)), typ)))
                })
            }
            AExpr::Match(e1, pats, typ) => {
                norm_name(*e1, namer, move |n| {
                    k (NExpr::Val(NVal::Match(n, pats.into_iter().map(|(p, e)| (p, norm(e, namer, identity))).collect(), typ)))
                })
            }
        }
    }

    pub fn normalize(program: AExpr, namer: &mut impl VarNamer) -> NExpr {
        norm(program, namer, identity)
    }
}

pub mod anf {
    use crate::soir::{Constant, Id, Typ};
    use crate::soir::typed::APat;

    #[derive(Debug)]
    pub enum NAtom {
        Constant(Constant, Typ),
        Var(Id, Typ),
        Lambda(Vec<Id>, Box<NExpr>, Typ),
    }
    #[derive(Debug)]
    pub enum NVal {
        Apply(NAtom, Vec<NAtom>, Typ),
        If(NAtom, Box<NExpr>, Box<NExpr>, Typ),
        Match(NAtom, Vec<(APat, NExpr)>, Typ),
    }
    #[derive(Debug)]
    pub enum NExpr {
        LetA(Id, NAtom, Box<NExpr>, Typ),
        LetV(Id, NVal, Box<NExpr>, Typ),
        Atom(NAtom),
        Val(NVal),
    }

    impl NAtom {
        pub fn typ(&self) -> &Typ {
            match self {
                NAtom::Constant(_, t)
                | NAtom::Var(_, t)
                | NAtom::Lambda(_, _, t) => t,
            }
        }
    }
    impl NVal {
        pub fn typ(&self) -> &Typ {
            match self {
                NVal::Apply(_, _, t)
                | NVal::If(_, _, _, t)
                | NVal::Match(_, _, t) => t
            }
        }
    }
    impl NExpr {
        pub fn typ(&self) -> &Typ {
            match self {
                NExpr::LetA(_, _, _, t) | NExpr::LetV(_, _, _, t) => t,
                NExpr::Atom(a) => a.typ(),
                NExpr::Val(v) => v.typ(),
            }
        }
    }
}

pub mod dataflow {
    // ANF is
    // <atom> ::= <constant> | <var> | \<var> -> <expr>
    // <val>  ::= <atom> <atom> | if <atom> then <expr> else <expr> | match <atom> in [ <pat> -> <expr> ... ]
    // <expr> ::= Let <var> = <atom> IN <expr> | Let <var> = <val> IN <expr> | <val> | <atom>
    // We want to form a digraph of groups of expressions, so that each group consists of a single data access A,
    // and all expressions depending only on $"dep"(A) union {A}$ (the "shadow" of A).
    // Notes:
    //  - At the edge of the "shadow" of A, our AST may develop "holes"
    //  - Make names unique
    //  - How to handle fixpoint?
    //  - Do we need special handling for streams?
}