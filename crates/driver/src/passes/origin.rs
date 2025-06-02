use std::{collections::HashMap, iter::once};

use la_arena::{Arena, ArenaMap, Idx};
use lasso::{Rodeo, Spur};
use miette::{LabeledSpan, miette};
use pretty::{
    RcDoc,
    termcolor::{ColorSpec, StandardStream},
};
use telos_common::{
    source::Sources,
    span::{Span, Spanned},
};
use telos_parser::parser::Pat;

use super::knf::Ex;

/// A nested group of scopes that contains different variables, each of which is mapped to some `T`.
#[derive(Debug, Clone)]
pub struct Bindings<T> {
    m: Vec<HashMap<Spur, T>>,
}
impl<T> Default for Bindings<T> {
    fn default() -> Self {
        Self { m: vec![] }
    }
}
impl<T> Bindings<T> {
    pub fn insert(&mut self, spur: Spur, t: T) {
        let prev = self
            .m
            .last_mut()
            .expect("cannot insert into a `Bindings` with no scope")
            .insert(spur, t);
        assert!(
            prev.is_none(),
            "binding for {spur:?} has already been inserted"
        );
    }
    pub fn extend(&mut self, iter: impl IntoIterator<Item = (Spur, T)>) {
        self.m
            .last_mut()
            .expect("cannot insert into a `Bindings` with no scope")
            .extend(iter)
    }
    pub fn enter(&mut self) {
        self.m.push(HashMap::new());
    }
    pub fn exit(&mut self) {
        self.m.pop();
    }
    pub fn get(&self, spur: Spur) -> Option<&T> {
        self.m.iter().rev().find_map(|layer| layer.get(&spur))
    }
    pub fn get_mut(&mut self, spur: Spur) -> Option<&mut T> {
        self.m
            .iter_mut()
            .rev()
            .find_map(|layer| layer.get_mut(&spur))
    }
}

#[derive(Clone)]
pub enum Loc {
    /// This location describes the implied location of the value of some computation
    Var(Spur),
    /// This is a new location that is _dynamically derived_ from some (set of) values.
    Gen(Spur),
    /// Dynamically determined location
    Dynamic(Spur, Spur),
    // /// This location describes the implied location of the value of some parameter to a function
    // Value(Idx<Ex>),
}
impl std::fmt::Debug for Loc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Loc::Var(spur) => write!(f, "Var({:?})", spur),
            Loc::Gen(..) => write!(f, "Gen(..)"),
            Loc::Dynamic(a, b) => write!(f, "Dynamic({:?}, {:?})", a, b),
        }
    }
}

#[derive(Debug, Clone)]
pub enum OriginExpr {
    /// A location
    Loc(Loc),
    /// Concatenation of some number of origin expressions.
    Concat(Vec<OriginExpr>),
    /// Intersection between some number of origin expressions.
    Isect(Vec<OriginExpr>),
    /// Application of some location _variable_ (if the location were known, the application can be
    /// inlined).
    App(Spur, Vec<OriginExpr>),
    /// The value is available everywhere.
    Constant,
}
impl OriginExpr {
    pub fn loc_var(n: Spur) -> Self {
        OriginExpr::Loc(Loc::Var(n))
    }
    pub fn loc_gen(n: Spur) -> Self {
        OriginExpr::Loc(Loc::Gen(n))
    }
    fn substitute(
        &mut self,
        substitutions: &HashMap<Spur, &Placement>,
        base_quantification: &Vec<Spur>,
        ctx: &mut Ctx,
    ) {
        match self {
            OriginExpr::Loc(Loc::Gen(gn)) => {
                // Generator location should be swapped out with the location of the current
                // application (since substitution implies application)
                *self = Self::Loc(Loc::Gen(ctx.loc_gen(*gn)));
            }
            OriginExpr::Loc(_) | OriginExpr::Constant => (),
            OriginExpr::Concat(origin_exprs)
            | OriginExpr::Isect(origin_exprs)
            | OriginExpr::App(_, origin_exprs) => {
                //
                origin_exprs.iter_mut().for_each(|expr| {
                    if let OriginExpr::Loc(Loc::Var(var)) = expr {
                        if let Some(subst) = substitutions.get(var) {
                            *expr = subst.expr.clone();
                        }
                    } else {
                        expr.substitute(substitutions, base_quantification, ctx);
                    }
                })
            }
        }
        if let OriginExpr::App(app_spur, args) = self {
            if let Some(&func_placement) = substitutions.get(&app_spur) {
                // Since it's been applied, it looks like
                //  forall [.., pk, pk-1, .., p1, p0]
                // we need to figure out what p0..pk are so we can substitute them with `args`.
                // we then will take the `func_placement` and substitute it with `args`
                // the trouble is that we don't currently have a function that can do that
                // substitute() wants a Vec of Placements in case any of the substitutions are
                // universally quantified
                let placed_args = args
                    .iter()
                    .map(|arg| Placement {
                        expr: arg.clone(),
                        quantifiers: base_quantification.clone(),
                    })
                    .collect();
                let mut placement = func_placement.clone();
                placement.substitute(&placed_args, ctx);
                *self = placement.expr;
                // XXX: should be safe to discard `placement`'s quantifiers

                // return early; our work is done
                return;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Placement {
    pub expr: OriginExpr,
    /// Quantifiers; these are the location variables representing all of the parameters used in
    /// the origin expression.
    /// For example,
    /// ```
    /// \a b -> \c d -> a + b + c + d
    /// ```
    /// would have quantifiers [d, c, b, a].
    /// Specifically, variables are specified from innermost to outermost scope, in reverse
    /// parameter order.
    pub quantifiers: Vec<Spur>,
}
impl Placement {
    pub fn param(loc_var: Spur) -> Self {
        Self {
            expr: OriginExpr::Loc(Loc::Var(loc_var)),
            quantifiers: vec![],
        }
    }
    pub fn literal() -> Self {
        Self {
            expr: OriginExpr::Constant,
            quantifiers: vec![],
        }
    }
    pub fn add_quantifiers(self, rhs: impl IntoIterator<Item = Spur>) -> Self {
        let Self {
            expr,
            mut quantifiers,
        } = self;
        quantifiers.extend(rhs.into_iter());
        Self { expr, quantifiers }
    }
    pub fn substitute(&mut self, substitutions_vec: &Vec<Placement>, ctx: &mut Ctx) {
        if self.quantifiers.len() < substitutions_vec.len() {
            panic!(
                "cannot perform substitutions {substitutions_vec:?} when quantifier-list {:?} is shorter",
                self.quantifiers
            );
        }

        let drain_begin = self.quantifiers.len() - substitutions_vec.len();
        let substitutions = self
            .quantifiers
            .drain(drain_begin..)
            .rev()
            .zip(substitutions_vec.iter())
            .collect();

        self.expr.substitute(&substitutions, &self.quantifiers, ctx);

        self.merge_quantifiers(substitutions_vec);
    }
    pub fn merge_quantifiers(&mut self, placements: &Vec<Placement>) {
        // precondition: among self and substitutions.values(), there is one `quantifiers` for
        // which all other quantifiers are prefixes; we take that as the new `quantifiers`.
        // we can find this `quantifiers` (without checking the validity precondition), by simply
        // finding the longest `quantifiers`.

        let merged_quantifiers = once(&self.quantifiers)
            .chain(placements.iter().map(|placement| &placement.quantifiers))
            .max_by_key(|q| q.len())
            .unwrap_or(&vec![])
            .clone();
        // Check the validity
        once(&self.quantifiers)
            .chain(placements.iter().map(|placement| &placement.quantifiers))
            .for_each(|q| {
                assert!(&merged_quantifiers[..q.len()] == q);
            });
        self.quantifiers = merged_quantifiers;
    }
}

/// Analysis context object.
pub struct Ctx<'a> {
    bindings: Bindings<Placement>,
    expr_placement: ArenaMap<Idx<Ex>, Placement>,
    rodeo: Rodeo,
    sources: &'a Sources,
    unique_param: usize,
    unique_gen: usize,
}
impl Ctx<'_> {
    /// Get a fresh location variable, tagged with the name of the parameter it represents
    fn new_loc_var(&mut self, name: Spur) -> Spur {
        let s = format!("#{}_{}", self.unique_param, self.rodeo.resolve(&name));
        self.unique_param += 1;
        self.rodeo.get_or_intern(&s)
    }
    /// Get the placement of the expression `expr`
    pub fn get_expr_placement(&self, expr: Idx<Ex>) -> miette::Result<&Placement> {
        let Some(original_placement_info) = self.expr_placement.get(expr) else {
            panic!("{expr:?} is not placed");
        };
        Ok(original_placement_info)
    }
    /// Get the placement of the binding `name`.
    fn get_binding_placement(&self, name: Spanned<Spur>) -> miette::Result<&Placement> {
        let Some(placement) = self.bindings.get(*name) else {
            return Err(miette!(
                labels = vec![LabeledSpan::at(
                    self.sources.translate_span(name.span),
                    "referenced here"
                )],
                "no binding '{}' is in scope",
                self.rodeo.resolve(&name)
            )
            .with_source_code(self.sources.clone()));
        };
        Ok(placement)
    }
    fn place_expr(&mut self, idx: Idx<Ex>, placement: Placement) -> Placement {
        self.expr_placement.insert(idx, placement.clone());
        placement
    }

    fn loc_gen(&mut self, gn: Spur) -> Spur {
        let s = format!("{}_{}", self.rodeo.resolve(&gn), self.unique_gen);
        self.unique_gen += 1;
        self.rodeo.get_or_intern(&s)
    }
}

pub fn analyze_items(
    inputs: &[(Span, Spur, Vec<Spanned<Spur>>, Spanned<Idx<Ex>>)],
    rodeo: Rodeo,
    arena: &mut Arena<Ex>,
    builtins: &HashMap<Spur, Placement>,
    sources: &Sources,
) -> miette::Result<(Rodeo, ArenaMap<Idx<Ex>, Placement>)> {
    let mut ctx = Ctx {
        bindings: Bindings::default(),
        expr_placement: ArenaMap::new(),
        rodeo,
        sources,
        unique_param: 0,
        unique_gen: 0,
    };
    ctx.bindings.enter();
    ctx.bindings.extend(builtins.clone());
    ctx.bindings.enter();

    for (tl_span, tl_name, tl_params, tl_body) in inputs {
        let fake_lam = arena.alloc(Ex::Lam {
            params: tl_params.clone(),
            body: *tl_body,
        });
        let placement = analyze_expr(Spanned::new(fake_lam, *tl_span), arena, &mut ctx)?;
        ctx.bindings.insert(*tl_name, placement);
    }
    // let Some(main) = ctx.bindings.get(main_spur) else {
    //     panic!("no `main` item defined at the toplevel")
    // };
    let Ctx {
        expr_placement,
        rodeo,
        ..
    } = ctx;

    Ok((rodeo, expr_placement))
}

fn analyze_expr(
    expr: Spanned<Idx<Ex>>,
    arena: &Arena<Ex>,
    ctx: &mut Ctx,
) -> miette::Result<Placement> {
    match &arena[*expr] {
        Ex::Match {
            expr: match_expr,
            arms,
        } => {
            let expr_placement = ctx.get_binding_placement(*match_expr)?.clone();
            let arm_placements: Vec<Placement> = arms
                .iter()
                .map(|arm| -> miette::Result<Placement> {
                    ctx.bindings.enter();
                    for fv in arm.pat.find_free_vars() {
                        ctx.bindings.insert(fv, expr_placement.clone());
                    }
                    let arm_placement = analyze_expr(arm.body, arena, ctx)?;
                    ctx.bindings.exit();
                    Ok(arm_placement)
                })
                .try_collect()?;
            let placement_expr = OriginExpr::Concat(vec![
                expr_placement.expr.clone(),
                OriginExpr::Isect(
                    arm_placements
                        .iter()
                        .map(|placement| placement.expr.clone())
                        .collect(),
                ),
            ]);
            let mut placement = Placement {
                expr: placement_expr,
                quantifiers: expr_placement.quantifiers,
            };
            placement.merge_quantifiers(&arm_placements);
            Ok(ctx.place_expr(*expr, placement))
        }
        Ex::Let { def, body } => {
            let expr_placement = analyze_expr(def.expr, arena, ctx)?;
            ctx.bindings.enter();
            ctx.bindings.insert(def.name, expr_placement);
            let body_placement = analyze_expr(*body, arena, ctx)?;
            ctx.bindings.exit();
            Ok(ctx.place_expr(*expr, body_placement))
        }
        Ex::LetRec { defs: _, body: _ } => {
            todo!("not implemented")
        }
        Ex::Lam { params, body } => {
            ctx.bindings.enter();
            let mut loc_vars = Vec::new();
            for param in params.iter().rev() {
                let loc_var = ctx.new_loc_var(**param);
                loc_vars.push(loc_var);
                ctx.bindings.insert(**param, Placement::param(loc_var));
            }
            let body_placement = analyze_expr(*body, arena, ctx)?;
            ctx.bindings.exit();
            Ok(ctx.place_expr(*expr, body_placement.add_quantifiers(loc_vars)))
        }
        Ex::Literal { literal: _ } => Ok(ctx.place_expr(*expr, Placement::literal())),
        Ex::App { func, args } => {
            let mut placement = ctx.get_binding_placement(*func)?.clone();
            let arg_placements: Vec<Placement> = args
                .iter()
                .map(|arg| -> miette::Result<Placement> {
                    Ok(ctx.get_binding_placement(*arg)?.clone())
                })
                .try_collect()?;
            placement.substitute(&arg_placements, ctx);
            Ok(ctx.place_expr(*expr, placement))
        }
        Ex::Field {
            expr: field_expr,
            field: _,
        } => Ok(ctx.place_expr(*expr, ctx.get_binding_placement(*field_expr)?.clone())),
        Ex::Var { name } => Ok(ctx.place_expr(
            *expr,
            ctx.get_binding_placement(expr.map(|_| *name))?.clone(),
        )),
    }
}

type PrettyDoc<'a> = RcDoc<'a, ColorSpec>;

#[derive(Debug, Copy, Clone)]
pub struct PrintCtx<'a> {
    pub resolver: &'a Rodeo,
    pub arena: &'a Arena<Ex>,
    pub width: usize,
    pub ex_map: Option<&'a ArenaMap<Idx<Ex>, Placement>>,
}
impl PrintCtx<'_> {
    fn print_origin_expr(&self, expr: &OriginExpr) -> PrettyDoc {
        RcDoc::text("(")
            .append(match expr {
                OriginExpr::Loc(loc) => match loc {
                    Loc::Var(spur) => RcDoc::as_string(self.resolver.resolve(spur)),
                    Loc::Gen(gn) => RcDoc::as_string(format!("#G{}", self.resolver.resolve(&gn))),
                    Loc::Dynamic(gn, inst) => RcDoc::as_string(format!(
                        "#G{}_{}",
                        self.resolver.resolve(&gn),
                        self.resolver.resolve(&inst)
                    )),
                },
                OriginExpr::Concat(origin_exprs) => RcDoc::intersperse(
                    origin_exprs.iter().map(|oe| self.print_origin_expr(oe)),
                    RcDoc::text("⨆"),
                ),
                OriginExpr::Isect(origin_exprs) => RcDoc::intersperse(
                    origin_exprs.iter().map(|oe| self.print_origin_expr(oe)),
                    RcDoc::text("⨅"),
                ),
                OriginExpr::App(spur, origin_exprs) => {
                    RcDoc::as_string(self.resolver.resolve(spur))
                        .append(RcDoc::text("∘"))
                        .append(RcDoc::intersperse(
                            origin_exprs.iter().map(|oe| self.print_origin_expr(oe)),
                            RcDoc::text(","),
                        ))
                }
                OriginExpr::Constant => RcDoc::text("ε"),
            })
            .append(RcDoc::text(")"))
    }
    fn print_placement(&self, placement: &Placement) -> PrettyDoc {
        RcDoc::text("∀")
            .append(RcDoc::intersperse(
                placement
                    .quantifiers
                    .iter()
                    .map(|q| RcDoc::as_string(self.resolver.resolve(q))),
                RcDoc::line(),
            ))
            .append(".")
            .append(self.print_origin_expr(&placement.expr))
    }
    fn annotate(&self, idx: Idx<Ex>) -> PrettyDoc {
        if let Some(ex_map) = self.ex_map {
            RcDoc::text(":")
                .append(
                    self.print_placement(&ex_map[idx]).annotate(
                        ColorSpec::new()
                            .set_fg(Some(pretty::termcolor::Color::Cyan))
                            .clone(),
                    ),
                )
                .group()
        } else {
            RcDoc::nil()
        }
    }
    fn print_pat(&self, pat: &Pat) -> PrettyDoc {
        match pat {
            Pat::Cons { name, args } => RcDoc::text("(`")
                .append(self.resolver.resolve(name))
                .group()
                .append(RcDoc::line())
                .append(
                    RcDoc::intersperse(args.iter().map(|arg| self.print_pat(arg)), RcDoc::line())
                        .nest(4)
                        .group(),
                )
                .append(RcDoc::text(")"))
                .group(),
            Pat::Ident { name } => RcDoc::as_string(self.resolver.resolve(name)),
            Pat::Literal { literal } => {
                let mut s = String::new();
                literal.fmt(&mut s, &self.resolver).unwrap();
                RcDoc::text(s)
            }
        }
    }
    fn print_idx(&self, ex: Idx<Ex>) -> PrettyDoc {
        self.print_idx_inner(ex).append(self.annotate(ex))
    }
    fn print_idx_inner(&self, ex: Idx<Ex>) -> PrettyDoc {
        match &self.arena[ex] {
            Ex::Match { expr, arms } => {
                // match expr in
                //  .. | pat -> body
                RcDoc::text("match")
                    .append(RcDoc::line())
                    .append(RcDoc::as_string(self.resolver.resolve(&expr.inner)))
                    .append(RcDoc::line())
                    .append(RcDoc::text("in"))
                    .group()
                    .append(RcDoc::line_())
                    .append(RcDoc::intersperse(
                        arms.iter().map(|arm| {
                            RcDoc::text("|")
                                .append(RcDoc::line())
                                .append(self.print_pat(&arm.pat))
                                .append("->")
                                .group()
                                .append(self.print_idx(*arm.body).nest(4))
                        }),
                        RcDoc::line(),
                    ))
            }
            Ex::Let { def, body } => {
                // let name = expr in body
                RcDoc::text("let")
                    .append(RcDoc::line())
                    .append(RcDoc::as_string(self.resolver.resolve(&def.name)))
                    .append(RcDoc::line())
                    .group()
                    .append(
                        RcDoc::text("=")
                            .append(RcDoc::line())
                            .append(self.print_idx(*def.expr))
                            .append(RcDoc::line())
                            .append(RcDoc::text("in"))
                            .group()
                            .nest(4)
                            .append(RcDoc::line_())
                            .append(self.print_idx(**body).nest(4)),
                    )
            }
            Ex::LetRec { defs: _, body: _ } => {
                todo!("unimplemented")
            }
            Ex::Lam { params, body } => {
                // \..params -> body
                RcDoc::text("\\")
                    .append(RcDoc::intersperse(
                        params
                            .iter()
                            .map(|param| RcDoc::as_string(self.resolver.resolve(param))),
                        RcDoc::line(),
                    ))
                    .append(RcDoc::line())
                    .append(RcDoc::text("->"))
                    .group()
                    .append(self.print_idx(**body).nest(4))
                    .group()
            }
            Ex::Literal { literal } => {
                // <literal>
                let mut s = String::new();
                literal.fmt(&mut s, &self.resolver).unwrap();
                RcDoc::text(s)
            }
            Ex::App { func, args } => RcDoc::text("(")
                .append(RcDoc::as_string(self.resolver.resolve(func)))
                .group()
                .append(RcDoc::line())
                .append(RcDoc::intersperse(
                    args.iter()
                        .map(|arg| RcDoc::as_string(self.resolver.resolve(arg))),
                    RcDoc::line(),
                ))
                .append(RcDoc::text(")"))
                .group(),
            Ex::Field { expr, field } => {
                // expr.field
                RcDoc::as_string(self.resolver.resolve(expr))
                    .append(RcDoc::as_string(self.resolver.resolve(field)))
                    .group()
            }
            Ex::Var { name } => {
                // <name>
                RcDoc::as_string(self.resolver.resolve(name))
            }
        }
    }
    pub fn pretty_print(&self, idx: Idx<Ex>) {
        self.print_idx(idx)
            .render_colored(
                self.width,
                StandardStream::stderr(pretty::termcolor::ColorChoice::Always),
            )
            .unwrap();
    }
}
