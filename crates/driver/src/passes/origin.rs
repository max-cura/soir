//! Origin flow analysis.

use std::{cell::RefCell, collections::HashMap, iter::once};

use la_arena::{Arena, ArenaMap, Idx};
use lasso::{Rodeo, Spur};
use miette::{LabeledSpan, miette};
use pretty::{
    RcDoc,
    termcolor::{Color, ColorSpec, StandardStream},
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

#[derive(Copy, Clone)]
pub enum Loc {
    /// This location describes the implied location of the value of some computation
    Var(Spur),
    /// This is a new location that is _dynamically derived_ from some (set of) values.
    Gen(Spur),
    /// This value is available everywhere
    Anywhere,
}
impl std::fmt::Debug for Loc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Loc::Var(spur) => write!(f, "Var({:?})", spur),
            Loc::Gen(spur) => write!(f, "Gen({:?})", spur),
            Loc::Anywhere => write!(f, "Anywhere"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LocExpr {
    Var(Spur),
    Gen(Spur),
    App(Spur, Vec<OriginExpr>),
    Root,
}
impl PartialEq for LocExpr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Var(a), Self::Var(b)) => a == b,
            (Self::Gen(a), Self::Gen(b)) => a == b,
            (Self::Root, Self::Root) => true,
            (Self::App(..), Self::App(..)) => {
                tracing::warn!("ignored LocExpr::App comparison");
                false
            }
            _ => false,
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
}
impl OriginExpr {
    pub fn normalize(&self) -> OriginExpr {
        match self {
            OriginExpr::Loc(_) => self.clone(),
            // Assume that this happens post-substitution, so there's no way to resolve this
            // application any further. However, we can normalize the parameters.
            OriginExpr::App(f, origin_exprs) => {
                OriginExpr::App(*f, origin_exprs.iter().map(Self::normalize).collect())
            }
            OriginExpr::Concat(origin_exprs) => {
                // Simplifications: flattening
                // Key point: this converges immediately
                let simplified_origin_exprs: Vec<_> =
                    origin_exprs.iter().map(Self::normalize).collect();
                let mut flattened_origin_exprs = vec![];
                for oe in simplified_origin_exprs {
                    if let Self::Concat(nest) = oe {
                        flattened_origin_exprs.extend(nest);
                    } else if let Self::Loc(Loc::Anywhere) = oe {
                        // skip
                    } else {
                        flattened_origin_exprs.push(oe);
                    }
                }
                // In theory, we could also deduplicate here; however, that causes problems because
                // consistency does not imply equality, and it's not really clear which origins we
                // wish to retain
                OriginExpr::Concat(flattened_origin_exprs)
            }
            OriginExpr::Isect(origin_exprs) => {
                // Simplifications: flattening and intersection
                // Key point: these converges immediately, even under composition

                // Flattening: (a ⨅ b) ⨅ (c ⨅ d) = a ⨅ b ⨅ c ⨅ d
                let simplified_origin_exprs: Vec<_> =
                    origin_exprs.iter().map(Self::normalize).collect();
                let mut flattened_origin_exprs = vec![];
                for oe in simplified_origin_exprs {
                    if let Self::Isect(nest) = oe {
                        flattened_origin_exprs.extend(nest);
                    } else {
                        flattened_origin_exprs.push(oe);
                    }
                }
                // tracing::debug!("Isect : flattened = {flattened_origin_exprs:?}");

                // Intersection: go element by element and try to run intersection.
                // The key point is that there may be some places where we don't do normalization

                // First, we need to describe the current set of valid locations; we initialize it
                // to ⊤, and let it be implicitly a `Concat`.
                let mut valid_set: Option<Vec<OriginExpr>> = None;
                for item in flattened_origin_exprs {
                    // Guarantees: `item` is not an Isect
                    let mut restriction_union = match item {
                        OriginExpr::Loc(loc) => {
                            vec![OriginExpr::Loc(loc)]
                        }
                        OriginExpr::Concat(origin_exprs) => {
                            // standard restriction
                            origin_exprs
                        }
                        app @ OriginExpr::App(_, _) => {
                            // let f = \y -> \g -> g y
                            // g : ∀y. g∘y
                            // g has no intrinsic location (as it is not a builtin), so we must
                            // endeavour to determine for ourselves. thus, it will fall to
                            // `normalize()`. Then this should be conservatively estimated to be
                            // available at the union of the arguments' origins
                            vec![app]
                        }
                        OriginExpr::Isect(_) => {
                            unreachable!("post-flattening Isect OE contains Isects")
                        }
                    };
                    if let Some(valid_set_) = valid_set {
                        let mut valid_set_new = vec![];
                        for oe in &valid_set_ {
                            // oe is either Isect, App, or Loc
                            // we previously normalized origin_exprs, so that rules out Isect as well
                            // thus: App or Loc
                            for item in &restriction_union {
                                match (&oe, item) {
                                    (OriginExpr::Loc(Loc::Anywhere), _) => {
                                        unreachable!("Loc::Anywhere in valid_set")
                                    }
                                    // skip - if it's available everywhere, it doesn't restrict the
                                    // valid set
                                    // XXX: nvm, it seems like 'anywhere' doesn't have good semantics rn
                                    (&a, OriginExpr::Loc(Loc::Anywhere)) => {
                                        // Anywhere only can come from Isect, where it has no effect
                                        // Don't have to worry about the weird Concat(Anywhere, ...) case
                                        valid_set_new.push(a.clone());
                                    }
                                    (
                                        OriginExpr::Loc(Loc::Var(a)),
                                        OriginExpr::Loc(Loc::Var(b)),
                                    ) if a == b => valid_set_new.push(oe.clone()),
                                    (
                                        OriginExpr::Loc(Loc::Gen(a)),
                                        OriginExpr::Loc(Loc::Gen(b)),
                                    ) if a == b => valid_set_new.push(oe.clone()),
                                    (OriginExpr::Loc(_), OriginExpr::Loc(_)) => continue,
                                    (OriginExpr::Loc(_), OriginExpr::App(_, _)) => continue,
                                    (OriginExpr::App(_, _), OriginExpr::Loc(_)) => continue,
                                    (OriginExpr::App(_, _), OriginExpr::App(_, _)) => continue,
                                    _ => unreachable!(),
                                }
                            }
                        }
                        // tracing::debug!(
                        // "{valid_set_:?} restricted by {restriction_union:?} = {valid_set_new:?}"
                        // );
                        valid_set = Some(valid_set_new);
                    } else {
                        restriction_union
                            .retain(|oe| !matches!(oe, OriginExpr::Loc(Loc::Anywhere)));
                        valid_set = Some(restriction_union);
                    }
                }
                let Some(valid_set) = valid_set else {
                    panic!("interseciton {origin_exprs:?} has empty valid_set (None)");
                };
                // if valid_set.is_empty() {
                //     panic!("intersection {origin_exprs:?} has empty valid_set ([])")
                // }
                if valid_set.len() == 1 {
                    valid_set[0].clone()
                } else {
                    OriginExpr::Concat(valid_set)
                }
            }
        }
    }
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
        replace_gen: Option<(Spur, Spur)>,
    ) {
        match self {
            OriginExpr::Loc(Loc::Gen(gn)) => {
                // Generator location should be swapped out with the location of the current
                // application (since substitution implies application)
                if let Some((replace, with)) = replace_gen
                    && *gn == replace
                {
                    *self = Self::Loc(Loc::Gen(with));
                } else {
                    *self = Self::Loc(Loc::Gen(ctx.loc_gen(*gn)));
                }
            }
            OriginExpr::Loc(_) => (),
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
                        expr.substitute(substitutions, base_quantification, ctx, replace_gen);
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
                        location: None,
                    })
                    .collect();
                let mut placement = func_placement.clone();
                placement.substitute(&placed_args, ctx);
                *self = placement.expr;
                // XXX: should be safe to discard `placement`'s quantifiers
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
    pub location: Option<LocExpr>,
}
impl Placement {
    pub fn param(loc_var: Spur) -> Self {
        Self {
            expr: OriginExpr::Loc(Loc::Var(loc_var)),
            quantifiers: vec![],
            location: Some(LocExpr::Var(loc_var)),
        }
    }
    pub fn literal() -> Self {
        Self {
            expr: OriginExpr::Loc(Loc::Anywhere),
            quantifiers: vec![],
            location: Some(LocExpr::Root),
        }
    }
    pub fn substitute(&mut self, substitutions_vec: &Vec<Placement>, ctx: &mut Ctx) {
        if self.quantifiers.len() < substitutions_vec.len() {
            panic!(
                "cannot perform substitutions {substitutions_vec:?} when quantifier-list {:?} is shorter",
                self.quantifiers
            );
        }

        let replace_gen = if let Some(LocExpr::Gen(gfn)) = self.location {
            let new_loc = ctx.loc_gen(gfn);
            self.location = Some(LocExpr::Gen(new_loc));
            Some((gfn, new_loc))
        } else {
            None
        };

        let drain_begin = self.quantifiers.len() - substitutions_vec.len();
        let substitutions = self
            .quantifiers
            .drain(drain_begin..)
            .rev()
            .zip(substitutions_vec.iter())
            .collect();

        self.expr
            .substitute(&substitutions, &self.quantifiers, ctx, replace_gen);

        self.merge_quantifiers(substitutions_vec);

        if replace_gen.is_none() {
            // If the subsequent location isn't across a network boundary, then we need to figure
            // out what the candidates are
            // This means that we need to actively calculate and normalize the origins on all sides
            // If all sides are the same, then just run with that
            // Otherwise, find the intersection, and pick the most commonly repeated location
            // This is a kludge, but we'll live
            // tracing::debug!("pre-normalization = {:?}", self.expr);
            let normalized = self.expr.normalize();
            // tracing::debug!("normalized = {normalized:?}");
            let candidates = match normalized {
                OriginExpr::Concat(origin_exprs) => origin_exprs,
                OriginExpr::Isect(origin_exprs) => {
                    unreachable!("normalize() returned Isect({origin_exprs:?})");
                }
                x => vec![x],
            };
            #[derive(Debug)]
            enum Candidate {
                Loc(Loc),
                App(Spur, Vec<OriginExpr>),
            }
            impl PartialEq for Candidate {
                fn eq(&self, other: &Self) -> bool {
                    match (self, other) {
                        (Self::Loc(a), Self::Loc(b)) => match (a, b) {
                            (Loc::Var(a), Loc::Var(b)) => a == b,
                            (Loc::Gen(a), Loc::Gen(b)) => a == b,
                            (Loc::Anywhere, _) | (_, Loc::Anywhere) => unreachable!(),
                            _ => false,
                        },
                        _ => false,
                    }
                }
            }
            let candidates: Vec<_> = candidates
                .into_iter()
                .map(|oe| match oe {
                    OriginExpr::Loc(loc) => Candidate::Loc(loc),
                    OriginExpr::Concat(_) => unreachable!(),
                    OriginExpr::Isect(_) => unreachable!(),
                    OriginExpr::App(spur, origin_exprs) => Candidate::App(spur, origin_exprs),
                })
                .collect();
            if candidates.is_empty() {
                self.location = Some(LocExpr::Root);
            } else {
                // pick 1
                let mut counts: Vec<(Candidate, usize)> = vec![];
                for candidate in candidates {
                    let mut found = false;
                    for c in &mut counts {
                        if candidate == c.0 {
                            c.1 += 1;
                            assert!(!found);
                            found = true;
                        }
                    }
                    let initial = if matches!(candidate, Candidate::Loc(_)) {
                        1
                    } else {
                        0
                    };
                    counts.push((candidate, initial));
                }
                // tracing::debug!("counts = {counts:?}");
                let (candidate, count) =
                    counts.into_iter().max_by_key(|(_, count)| *count).unwrap();
                if count == 0 {
                    // a bit hacky, but the current location system can't handle deferred locations
                    self.location = Some(LocExpr::Root);
                }
                match candidate {
                    Candidate::Loc(loc) => {
                        self.location = Some(match loc {
                            Loc::Var(spur) => LocExpr::Var(spur),
                            Loc::Gen(spur) => LocExpr::Gen(spur),
                            Loc::Anywhere => unreachable!(),
                        });
                    }
                    Candidate::App(f, oes) => {
                        self.location = Some(LocExpr::App(f, oes));
                    }
                }
            }
        }
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
                location: None,
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
            let mut body_placement = analyze_expr(*body, arena, ctx)?;
            ctx.bindings.exit();
            body_placement.quantifiers.extend(loc_vars);
            body_placement.location = Some(LocExpr::Root);
            Ok(ctx.place_expr(*expr, body_placement))
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

#[derive(Debug, Clone)]
pub struct PrintCtx<'a> {
    pub resolver: &'a Rodeo,
    pub arena: &'a Arena<Ex>,
    pub width: usize,
    pub ex_map: Option<&'a ArenaMap<Idx<Ex>, Placement>>,
    pub color_gen: RefCell<ColorGenerator>,
}
impl PrintCtx<'_> {
    fn print_origin_expr(&self, expr: &OriginExpr) -> PrettyDoc {
        match expr {
            OriginExpr::Loc(loc) => match loc {
                Loc::Var(spur) => RcDoc::as_string(self.resolver.resolve(spur)),
                Loc::Gen(gn) => RcDoc::as_string(format!("#G{}", self.resolver.resolve(&gn))),
                Loc::Anywhere => RcDoc::text("⊤"),
            },
            OriginExpr::Concat(origin_exprs) => RcDoc::text("(")
                .append(RcDoc::intersperse(
                    origin_exprs.iter().map(|oe| self.print_origin_expr(oe)),
                    RcDoc::text("⨆"),
                ))
                .append(RcDoc::text(")")),
            OriginExpr::Isect(origin_exprs) => RcDoc::text("(")
                .append(RcDoc::intersperse(
                    origin_exprs.iter().map(|oe| self.print_origin_expr(oe)),
                    RcDoc::text("⨅"),
                ))
                .append(RcDoc::text(")")),
            OriginExpr::App(spur, origin_exprs) => RcDoc::text("(")
                .append(
                    RcDoc::as_string(self.resolver.resolve(spur))
                        .append(RcDoc::text("∘"))
                        .append(RcDoc::intersperse(
                            origin_exprs.iter().map(|oe| self.print_origin_expr(oe)),
                            RcDoc::text(","),
                        )),
                )
                .append(RcDoc::text(")")),
        }
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
            .append("@")
            .append(match &placement.location {
                Some(loc) => match loc {
                    LocExpr::Root => RcDoc::text("ε"),
                    x => self.print_origin_expr(&match x {
                        LocExpr::Var(spur) => OriginExpr::Loc(Loc::Var(*spur)),
                        LocExpr::Gen(spur) => OriginExpr::Loc(Loc::Gen(*spur)),
                        LocExpr::App(spur, origin_exprs) => {
                            OriginExpr::App(*spur, origin_exprs.clone())
                        }
                        LocExpr::Root => unreachable!(),
                    }),
                },
                None => PrettyDoc::text("??").annotate(ColorSpec::new().set_bold(true).clone()),
            })
    }
    fn annotate(&self, idx: Idx<Ex>, color: Color) -> PrettyDoc {
        if let Some(ex_map) = self.ex_map {
            RcDoc::text(":")
                .append(
                    self.print_placement(&ex_map[idx])
                        .annotate(ColorSpec::new().set_fg(Some(color)).clone()),
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
        let color = self.color_gen.borrow_mut().next();
        self.print_idx_inner(ex)
            .annotate(ColorSpec::new().set_fg(Some(color)).clone())
            .append(self.annotate(ex, color))
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
                    .append(RcDoc::text("="))
                    .append(RcDoc::line())
                    .append(self.print_idx(*def.expr))
                    .append(RcDoc::line())
                    .append(RcDoc::text("in"))
                    .group()
                    .append(RcDoc::line())
                    // .nest(4)
                    // )
                    .append(self.print_idx(**body).group().nest(4))
                    .append(RcDoc::line_())
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
                    .append(RcDoc::line())
                    .append(self.print_idx(**body).nest(4))
                    .append(RcDoc::line())
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

// From https://docs.rs/ariadne/latest/src/ariadne/draw.rs.html.

/// A type that can generate distinct 8-bit colors.
#[derive(Debug, Copy, Clone)]
pub struct ColorGenerator {
    state: [u16; 3],

    min_brightness: f32,
}
impl Default for ColorGenerator {
    fn default() -> Self {
        Self::from_state([30000, 15000, 35000], 0.5)
    }
}
impl ColorGenerator {
    /// Create a new [`ColorGenerator`] with the given pre-chosen state.
    ///
    /// The minimum brightness can be used to control the colour brightness (0.0 - 1.0). The default is 0.5.

    pub fn from_state(state: [u16; 3], min_brightness: f32) -> Self {
        Self {
            state,
            min_brightness: min_brightness.max(0.0).min(1.0),
        }
    }

    /// Create a new [`ColorGenerator`] with the default state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate the next colour in the sequence.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Color {
        for i in 0..3 {
            // magic constant, one of only two that have this property!
            self.state[i] = (self.state[i] as usize).wrapping_add(40503 * (i * 4 + 1130)) as u16;
        }

        Color::Ansi256(
            16 + ((self.state[2] as f32 / 65535.0 * (1.0 - self.min_brightness)
                + self.min_brightness)
                * 5.0
                + (self.state[1] as f32 / 65535.0 * (1.0 - self.min_brightness)
                    + self.min_brightness)
                    * 30.0
                + (self.state[0] as f32 / 65535.0 * (1.0 - self.min_brightness)
                    + self.min_brightness)
                    * 180.0) as u8,
        )
    }
}
