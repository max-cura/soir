use crate::ast::{
    untyped_anf::{Atom as NAtom, Val as NVal, Expr as NExpr},
    flat_anf::{Pat as UPat, Atom as UAtom, Val as UVal, Expr as UExpr, ERef, UntypedAnf},
};
use crate::ast::flat_anf::{Defn, Pat};

struct Flattener {
    inner: UntypedAnf<()>,
}
impl Flattener {
    fn visit_atom(&mut self, atom: NAtom) -> UAtom<()> {
        match atom {
            NAtom::Constant(k) => {
                UAtom::Constant(k, ())
            }
            NAtom::Var(id) => {
                UAtom::Var(id, ())
            }
            NAtom::Lambda(args, body) => {
                let body = self.visit_expr(*body);
                UAtom::Lambda(args, body, ())
            }
        }
    }
    fn visit_val(&mut self, val: NVal) -> UVal<()> {
        match val {
            NVal::App(f, args) => {
                let f = self.visit_atom(f);
                let args = args.into_iter().map(|arg| self.visit_atom(arg)).collect();
                UVal::App(f, args, ())
            }
            NVal::If(c, e1, e2) => {
                let c = self.visit_atom(c);
                let e1 = self.visit_expr(*e1);
                let e2 = self.visit_expr(*e2);
                UVal::If(c, e1, e2, ())
            }
            NVal::Match(atom, pats) => {
                let atom = self.visit_atom(atom);
                let pats = pats.into_iter().map(|(pat, expr)| {
                    (pat, self.visit_expr(expr))
                }).collect();
                UVal::Match(atom, pats, ())
            }
        }
    }
    fn visit_expr(&mut self, expr: NExpr) -> ERef {
        match expr {
            NExpr::LetA(id, atom, body) => {
                let atom = self.visit_atom(atom);
                let body = self.visit_expr(*body);
                self.inner.push(UExpr::LetA(id, atom, body, ()))
            }
            NExpr::LetV(id, val, body) => {
                let val = self.visit_val(val);
                let body = self.visit_expr(*body);
                self.inner.push(UExpr::LetV(id, val, body, ()))
            }
            NExpr::Atom(atom) => {
                let atom = self.visit_atom(atom);
                self.inner.push(UExpr::Atom(atom))
            }
            NExpr::Val(val) => {
                let val = self.visit_val(val);
                self.inner.push(UExpr::Val(val))
            }
        }
    }
}

pub struct Definer {
    inner: UntypedAnf<()>,
}
impl Definer {
    fn visit_atom(&mut self, atom: UAtom<()>, expr: ERef, which: usize) -> () {
        match atom {
            UAtom::Constant(_, _) => {}
            UAtom::Var(_, _) => {}
            UAtom::Lambda(args, body, _) => {
                for arg in args {
                    self.inner.define(arg, Defn::Lambda(expr, which));
                }
                self.visit_expr(body);
            }
        }
    }
    fn visit_pat(&mut self, pat: UPat, expr: ERef) -> () {
        match pat {
            Pat::Constant(_) => {}
            Pat::Var(id) => {
                self.inner.define(id, Defn::Match(expr))
            }
            Pat::Cons(_, pats) => {
                for pat in pats {
                    self.visit_pat(pat, expr);
                }
            }
        }
    }
    fn visit_val(&mut self, val: UVal<()>, expr: ERef) -> () {
        match val {
            UVal::App(f, args, _) => {
                let all_atoms = std::iter::once(f).chain(args.into_iter()).enumerate();
                for (which, atom) in all_atoms {
                    self.visit_atom(atom, expr, which + 1);
                }
            }
            UVal::If(c, e1, e2, _) => {
                self.visit_atom(c, expr, 0);
                self.visit_expr(e1);
                self.visit_expr(e2);
            }
            UVal::Match(atom, pats, _) => {
                self.visit_atom(atom, expr, 0);
                for (pat, e) in pats {
                    self.visit_pat(pat, expr);
                    self.visit_expr(e);
                }
            }
        }
    }
    fn visit_expr(&mut self, expr: ERef) -> () {
        match self.inner.get(expr).clone() {
            UExpr::LetA(id, atom, body, _) => {
                self.inner.define(id, Defn::Let(expr));
                self.visit_atom(atom, expr, 0);
                self.visit_expr(body);
            }
            UExpr::LetV(id, val, body, _) => {
                self.inner.define(id, Defn::Let(expr));
                self.visit_val(val, expr);
                self.visit_expr(body);
            }
            UExpr::Atom(atom) => {
                self.visit_atom(atom, expr, 0);
            }
            UExpr::Val(val) => {
                self.visit_val(val, expr);
            }
        }
    }
}

pub fn flatten(
    input: NExpr,
) -> UntypedAnf<()> {
    let mut flattener = Flattener {
        inner: UntypedAnf::new(),
    };
    let root = flattener.visit_expr(input);
    let mut flat_anf = flattener.inner;
    flat_anf.set_root(root);

    let mut definer = Definer {
        inner: flat_anf,
    };
    definer.visit_expr(root);
    definer.inner
}