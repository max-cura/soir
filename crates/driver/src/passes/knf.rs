use std::{cell::RefCell, rc::Rc};

use chumsky::extra;
use la_arena::{Arena, Idx};
use lasso::{Resolver, Rodeo, Spur};
use std::fmt::Write;
use telos_common::span::{Span, Spanned};
use telos_parser::{
    lexer::LiteralToken,
    parser::{Def, Expr, Item, OpExpr, Pat, print_pat},
};

/// Match arm
#[derive(Debug, Clone)]
pub struct Case {
    pub pat: Pat,
    pub pat_span: Option<Span>,
    pub body: Spanned<Idx<Ex>>,
}
#[derive(Debug, Clone)]
pub struct Defn {
    pub name: Spur,
    pub name_span: Option<Span>,
    pub expr: Spanned<Idx<Ex>>,
}
#[derive(Debug, Clone)]
pub enum Ex {
    /// Match expression
    Match {
        expr: Spanned<Spur>,
        arms: Vec<Spanned<Case>>,
    },
    /// Non-recursive let-binding
    Let { def: Defn, body: Spanned<Idx<Ex>> },
    /// Recursive let-binding
    LetRec {
        defs: Vec<Spanned<Defn>>,
        body: Spanned<Idx<Ex>>,
    },
    /// Lambda expression
    Lam {
        params: Vec<Spanned<Spur>>,
        body: Spanned<Idx<Ex>>,
    },
    /// Literal
    Literal { literal: LiteralToken },
    /// Function Application
    App {
        func: Spanned<Spur>,
        args: Vec<Spanned<Spur>>,
    },
    /// Field access
    Field {
        expr: Spanned<Spur>,
        field: Spanned<Spur>,
    },
    /// Variable reference
    Var { name: Spur },
}

pub fn print_ex(
    ex: Idx<Ex>,
    arena: &Arena<Ex>,
    resolver: &impl Resolver,
    out: &mut String,
    indent: usize,
) {
    let ind = "  ".repeat(indent);
    match &arena[ex] {
        Ex::Match { expr, arms } => {
            let _ = writeln!(out, "{ind}match {} in", resolver.resolve(expr));
            for arm in arms {
                let _ = write!(out, "| ");
                print_pat(&arm.pat, resolver, out, indent + 1);
                let _ = writeln!(out, " ->");
                print_ex(arm.body.inner, arena, resolver, out, indent + 2);
                let _ = writeln!(out);
            }
        }
        Ex::Let { def, body } => {
            let _ = writeln!(out, "{ind}let {} =", resolver.resolve(&def.name));
            print_ex(def.expr.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out);
            let _ = write!(out, "{ind}in ");
            print_ex(body.inner, arena, resolver, out, 0);
            // let _ = writeln!(out);
        }
        Ex::LetRec { defs, body } => {
            let _ = writeln!(out, "{ind}let rec");
            for def in defs {
                let _ = write!(out, "{ind}  {} = ", resolver.resolve(&def.name));
                print_ex(def.expr.inner, arena, resolver, out, indent);
                let _ = writeln!(out);
            }
            let _ = writeln!(out, "{ind}in");
            print_ex(body.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out);
        }
        Ex::Lam { params, body } => {
            let _ = writeln!(
                out,
                "{ind}(\\{} ->",
                params
                    .iter()
                    .map(|param| resolver.resolve(&param.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
            print_ex(body.inner, arena, resolver, out, 1);
            let _ = writeln!(out);
            let _ = write!(out, "{ind})");
        }
        Ex::Literal { literal } => {
            let _ = write!(out, "{ind}");
            let _ = literal.fmt(out, resolver);
        }
        Ex::App { func, args } => {
            let _ = write!(
                out,
                "{ind}({} {})",
                resolver.resolve(&func.inner),
                args.iter()
                    .map(|arg| resolver.resolve(&arg.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
        }
        Ex::Field { expr, field } => {
            let _ = write!(
                out,
                "{ind}{}.{}",
                resolver.resolve(&expr.inner),
                resolver.resolve(field)
            );
        }
        Ex::Var { name } => {
            let _ = write!(out, "{ind}{}", resolver.resolve(name));
        }
    }
}

struct KCtx {
    expr_arena: Arena<Expr>,
    k_arena: Arena<Ex>,
    interner: Rodeo,
    vars: usize,
}
impl KCtx {
    /// Create new name of the form `$N` where N is an auto-incrementing counter
    fn new_var(&mut self) -> Spur {
        let next = self.vars;
        self.vars += 1;
        self.interner.get_or_intern(format!("$k{}", next).as_str())
    }
    fn alloc_ex(&mut self, ex: Ex) -> Idx<Ex> {
        self.k_arena.alloc(ex)
    }
    fn alloc_spanned_ex(&mut self, ex: Spanned<Ex>) -> Spanned<Idx<Ex>> {
        ex.map(|ex| self.alloc_ex(ex))
    }
}

type RCtx = Rc<RefCell<KCtx>>;
type Cont = Box<dyn FnOnce(Spanned<Spur>, RCtx) -> Spanned<Ex>>;

fn insert_let(expr: Spanned<Ex>, ctx: RCtx, cont: Cont) -> Spanned<Ex> {
    match expr.inner {
        Ex::Var { name } => cont(Spanned::new(name, expr.span), ctx),
        _ => {
            let name = ctx.borrow_mut().new_var();
            let body = cont(Spanned::new(name, expr.span), ctx.clone());
            let body_span = body.span;
            let expr = ctx.borrow_mut().alloc_spanned_ex(expr);
            let body = ctx.borrow_mut().alloc_spanned_ex(body);
            Spanned::new(
                Ex::Let {
                    def: Defn {
                        name,
                        name_span: None,
                        expr,
                    },
                    body,
                },
                body_span,
            )
        }
    }
}

pub fn k_norm_items(
    inputs: Vec<Spanned<Item>>,
    interner: Rodeo,
    expr_arena: Arena<Expr>,
) -> (
    Vec<(Span, Spur, Vec<Spanned<Spur>>, Idx<Ex>)>,
    Rodeo,
    Arena<Ex>,
) {
    let mut new_items = vec![];
    let k_arena = Arena::new();
    let rctx = Rc::new(RefCell::new(KCtx {
        expr_arena,
        k_arena,
        interner,
        vars: 0,
    }));
    for input_item in inputs {
        let span = input_item.span;
        let Item::Binding { name, params, body } = input_item.inner else {
            continue;
        };
        let body = k_norm(body, rctx.clone());
        new_items.push((
            span,
            name.inner,
            params,
            rctx.borrow_mut().alloc_ex(body.inner),
        ));
    }
    let KCtx {
        k_arena, interner, ..
    } = Rc::into_inner(rctx)
        .expect("all other strong references should be out of scope")
        .into_inner();
    (new_items, interner, k_arena)
}

fn k_norm(input: Spanned<Idx<Expr>>, ctx: Rc<RefCell<KCtx>>) -> Spanned<Ex> {
    let tmp = { ctx.borrow().expr_arena[*input].clone() };
    match tmp {
        Expr::If { expr, then, else_ } => {
            // let @0 = expr in
            //   match @0 in
            //     | True -> (then)
            //     | False -> (else_)
            let then_span = then.span;
            let else_span = else_.span;
            let input_span = input.span;
            let then = k_norm(then, ctx.clone());
            let else_ = k_norm(else_, ctx.clone());
            insert_let(
                k_norm(expr, ctx.clone()),
                ctx.clone(),
                Box::new(move |t, ctx| {
                    Spanned::new(
                        Ex::Match {
                            expr: t,
                            arms: vec![
                                then.map(|then| Case {
                                    pat: Pat::Literal {
                                        literal: LiteralToken::Bool(true),
                                    },
                                    pat_span: None,
                                    body: Spanned::new(ctx.borrow_mut().alloc_ex(then), then_span),
                                }),
                                else_.map(|else_| Case {
                                    pat: Pat::Literal {
                                        literal: LiteralToken::Bool(false),
                                    },
                                    pat_span: None,
                                    body: Spanned::new(ctx.borrow_mut().alloc_ex(else_), else_span),
                                }),
                            ],
                        },
                        input_span,
                    )
                }),
            )
        }
        Expr::Match { expr, arms } => {
            // let @0 = expr in
            //   match @a0 in { (arms) }
            let arms = arms
                .into_iter()
                .map(|arm| {
                    arm.map(|arm| {
                        let pat_span = Some(arm.pat.span);
                        let body = k_norm(arm.body, ctx.clone());
                        Case {
                            pat: arm.pat.inner,
                            pat_span,
                            body: ctx.borrow_mut().alloc_spanned_ex(body),
                        }
                    })
                })
                .collect();
            insert_let(
                k_norm(expr, ctx.clone()),
                ctx.clone(),
                Box::new(move |t, _ctx| Spanned::new(Ex::Match { expr: t, arms }, input.span)),
            )
        }
        Expr::Let {
            def: Def { name, expr },
            body,
        } => {
            let expr = k_norm(expr, ctx.clone());
            let body = k_norm(body, ctx.clone());
            let expr = ctx.borrow_mut().alloc_spanned_ex(expr);
            let body = ctx.borrow_mut().alloc_spanned_ex(body);
            Spanned::new(
                Ex::Let {
                    def: Defn {
                        name: name.inner,
                        name_span: Some(name.span),
                        expr,
                    },
                    body,
                },
                input.span,
            )
        }
        Expr::LetRec { defs, body } => {
            let body = k_norm(body, ctx.clone());
            Spanned::new(
                Ex::LetRec {
                    defs: defs
                        .into_iter()
                        .map(|def| {
                            def.map(|Def { name, expr }: Def| {
                                let expr = k_norm(expr, ctx.clone());
                                Defn {
                                    name: name.inner,
                                    name_span: Some(name.span),
                                    expr: ctx.borrow_mut().alloc_spanned_ex(expr),
                                }
                            })
                        })
                        .collect(),
                    body: ctx.borrow_mut().alloc_spanned_ex(body),
                },
                input.span,
            )
        }
        Expr::Lam { params, body } => {
            let body = k_norm(body, ctx.clone());
            Spanned::new(
                Ex::Lam {
                    params: params,
                    body: ctx.borrow_mut().alloc_spanned_ex(body),
                },
                input.span,
            )
        }
        Expr::Literal { literal } => input.map(|_| Ex::Literal { literal }),
        Expr::App { func, args } => {
            let input_span = input.span;
            insert_let(
                k_norm(func, ctx.clone()),
                ctx.clone(),
                Box::new(move |f, ctx| {
                    fn bind(
                        mut arg_bindings: Vec<Spanned<Spur>>,
                        mut args: Vec<Spanned<Idx<Expr>>>,
                        ctx: RCtx,
                        input_span: Span,
                        f: Spanned<Spur>,
                    ) -> Spanned<Ex> {
                        if let Some(head) = args.pop() {
                            insert_let(
                                k_norm(head, ctx.clone()),
                                ctx.clone(),
                                Box::new(move |arg, ctx| {
                                    arg_bindings.push(arg);
                                    bind(arg_bindings, args, ctx, input_span, f)
                                }),
                            )
                        } else {
                            Spanned::new(
                                Ex::App {
                                    func: f,
                                    args: arg_bindings,
                                },
                                input_span,
                            )
                        }
                    }
                    bind(vec![], args.clone(), ctx, input_span, f)
                }),
            )
        }
        Expr::OpSeq { mut seq } => {
            assert_eq!(seq.len(), 1);
            let OpExpr::Expr(expr) = seq.remove(0) else {
                unreachable!();
            };
            k_norm(expr, ctx)
        }
        Expr::Field { expr, field } => insert_let(
            k_norm(expr, ctx.clone()),
            ctx.clone(),
            Box::new(move |t, _ctx| Spanned::new(Ex::Field { expr: t, field }, input.span)),
        ),
        Expr::Var { name } => Spanned::new(Ex::Var { name }, input.span),
    }
}
