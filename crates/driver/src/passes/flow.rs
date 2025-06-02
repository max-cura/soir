//! Information we need:
//! - every expression has some set of other expressions upon which the value it computes depends
//! - every expression must be resolved to a particular _location_ where it will be run
//! - location is defined by the value resulting from an invocation of a boundary function
//! - when a variable with location crosses a boundary function, the produced location is
//!   _appended_ to its original location
//! - when a variable with location crosses a non-boundary function, the produced location is the same
//! - when multiple variables with locations cross a non-boundary function, the produced location is
//!   the intersection of all of the variables' locations
//! - finally, we perform a _resolution_ pass, which for every value, assigns it to a particular
//!   location in its ancestry (the most recent)
//! Analysis across function boundaries:
//! - we create a schema of what happens within the function; there are various possibilities:
//!    a. the function adds ancestry
//!    b. the function intersects ancestry
//!    c. (a) and (b) repeatedly in any order any number of times
//! On ancestry:
//! - 'add ancestry' composes and is commutative with itself
//! - 'intersect ancestry' composes with itself and is commutative with itself
//! - 'add ancestry' and 'intersect ancestry' compose with each other but DO NOT COMMUTE
//! Analysis of recursive functions:
//! - proper way would be to initialize the ancestry of the fixpoint to the ancestry universe and
//!   then narrow it until convergence
//! - however a convergence proof is needed, so we defer it for now
//! Errors that can be produced:
//! - assuming the program is well-formed, none

use std::collections::HashMap;

use la_arena::{Arena, ArenaMap, Idx};
use lasso::{Rodeo, Spur};
use miette::{LabeledSpan, miette};
use telos_common::{
    source::Sources,
    span::{Span, Spanned},
};

use super::knf::Ex;

#[derive(Debug, Clone)]
enum Loc {
    /// This location is derived from computation (e.g. a boundary function)
    Value(Idx<Ex>),
    /// This location is derived from some parameter
    Param(Spur),
}

#[derive(Debug, Clone, Default)]
struct Origin {
    inner: Vec<Loc>,
}
impl Origin {
    fn no_deps() -> Self {
        Origin { inner: vec![] }
    }
    fn param(arg_name: Spur) -> Self {
        Origin {
            inner: vec![Loc::Param(arg_name)],
        }
    }
    fn compose(&self, rhs: &Self) -> Self {
        todo!()
    }
    fn intersect(all: &[Self]) -> Self {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct PlacementInfo {
    // // Immediate set of expressions upon which the value of this expression is dependent
    // immediate_deps: Vec<Idx<Ex>>,
    // All of the locations which are guaranteed to be available to this expression when it is run
    origin: Origin,
    // // The final decision for where this expression will be run
    // location: Option<Loc>,
    /// If it's a function
    func: Option<FuncPlacement>,
}
impl PlacementInfo {
    /// `PlacementInfo` for an expression with no dependencies (i.e. that can be executed on any
    /// node).
    fn no_deps() -> Self {
        PlacementInfo {
            // immediate_deps: vec![],
            origin: Origin::no_deps(),
            func: None,
        }
    }
    fn param(arg_name: Spur) -> Self {
        PlacementInfo {
            origin: Origin::param(arg_name),
            func: None,
        }
    }
    fn intersect(all: &[Self]) -> Self {
        todo!()
    }
    fn compose(&self, rhs: &Self) -> Self {
        todo!()
    }
}

fn analyze_func(
    expr: Spanned<Idx<Ex>>,
    params: Vec<Spur>,
    arena: &Arena<Ex>,
    ctx: &mut Ctx,
) -> miette::Result<FuncPlacement> {
    todo!()
}

/// Analyze an expression `expr` which was named `bound_name` by K-normalization.
fn analyze_expr(expr: Spanned<Idx<Ex>>, arena: &Arena<Ex>, ctx: &mut Ctx) -> miette::Result<()> {
    match &arena[expr.inner] {
        Ex::Match {
            expr: match_expr,
            arms,
        } => {
            // compose `expr` with the intersection of `arms`
            // make sure to bind any pattern variables in the arms
            // as for the final origin of the `match`, it can only be the intersection of the origins
            let expr_placement = ctx
                .get_binding_placement(**match_expr, match_expr.span)?
                .clone();
            let mut arm_origins = vec![];
            for arm in arms {
                ctx.bindings.enter();
                for fv in arm.pat.find_free_vars() {
                    ctx.bindings.insert(fv, expr_placement.clone());
                }
                analyze_expr(arm.body, arena, ctx)?;
                arm_origins.push(
                    ctx.mapping
                        .get(*arm.body)
                        .expect("Idx<Ex> is not mapped")
                        .clone(),
                );
                ctx.bindings.exit();
            }

            // First, we have a hard dependency on `expr_placement`, simply because we must have
            // gotten that information in order to figure out which arm to take.
            // Secondly, we must conservatively assume that any of the arms could have been chosen,
            // so we take the set of origins that are common to all the arms.
            //
            // thus: origin = expr_placement | (&* arm_placements)
            let placement = expr_placement.compose(&PlacementInfo::intersect(&arm_origins));
            ctx.mapping.insert(*expr, placement);
        }
        Ex::Let { def, body } => {
            // TODO: when def.expr is a function, we MUST bind it in func_placement

            // bind `def` and then proxy to `body`
            analyze_expr(def.expr, arena, ctx)?;
            ctx.bindings.enter();
            ctx.bindings.insert(
                def.name,
                ctx.get_binding_placement_from_idx(*def.expr)?.clone(),
            );
            analyze_expr(*body, arena, ctx)?;
            ctx.bindings.exit();
            let Some(body_placement) = ctx.mapping.get(**body) else {
                panic!(
                    "analyze_expr on '{:?}' did not generate a mapping",
                    body.inner
                );
            };
            ctx.mapping.insert(*expr, body_placement.clone());
        }
        Ex::LetRec { defs, body } => {
            // bind `defs` and then proxy to `body`
            // note that in theory `defs` are allowed to be mutually recursive
            todo!()
        }
        Ex::Lam { params, body } => {
            // generate a FuncPlacement and bind it to `bound_name`
            ctx.bindings.enter();
            let mut param_syms = vec![];
            for param in params {
                let param_sym = ctx.new_param();
                param_syms.push(param_sym);
                ctx.bindings
                    .insert(param.inner, PlacementInfo::param(param_sym));
            }

            ctx.bindings.exit();
            todo!()
        }
        Ex::Literal { literal: _ } => {
            // no dependencies
            ctx.mapping.insert(*expr, PlacementInfo::no_deps());
        }
        Ex::App { func, args } => {
            // First, get the origin morphism that `func` defines. There are two cases:
            //  1. the morphism exists and is already bound
            //  2. the morphism was referred to by the program but does not exist or is not in scope
            // If the function is e.g. a lambda, then the expression will have been brought into
            // scope already. Therefore, it's safe to make an error of this directly.
            let Some(func_origin) = ctx.funcs.get(**func) else {
                return Err(miette!(
                    labels = [
                        LabeledSpan::at(ctx.sources.translate_span(func.span), "is not a function"),
                        LabeledSpan::at(
                            ctx.sources.translate_span(expr.span),
                            "used in this application"
                        )
                    ],
                    "application does not involve a function"
                )
                .with_source_code(ctx.sources.clone()));
            };
            let arg_origins: Vec<Origin> = args
                .iter()
                .map(|arg| -> miette::Result<Origin> {
                    Ok(ctx
                        .get_binding_placement(arg.inner, arg.span)
                        .cloned()?
                        .origin)
                })
                .try_collect()?;
            let origin = func_origin.evaluate(&arg_origins);
            ctx.mapping.insert(
                *expr,
                PlacementInfo {
                    origin,
                    location: None,
                },
            );
        }
        Ex::Field {
            expr: field_expr,
            field: _,
        } => {
            // It should be the case that if there's a problem with the referenced binding, then it
            // won't be one of the KNF-generated ones.
            let placement_info = ctx.get_binding_placement(field_expr.inner, expr.span)?;
            ctx.mapping.insert(*expr, placement_info.clone());
        }
        Ex::Var { name } => {
            let placement_info = ctx.get_binding_placement(*name, expr.span)?;
            ctx.mapping.insert(*expr, placement_info.clone());
        }
    }

    Ok(())
}

struct Ctx {
    // Map bindings to their defining expressions
    bindings: Bindings<PlacementInfo>,
    // Map expressions to their placement information
    mapping: ArenaMap<Idx<Ex>, PlacementInfo>,
    resolver: Rodeo,
    sources: Sources,
    unique_param: usize,
}
impl Ctx {
    fn new_param(&mut self) -> Spur {
        let s = format!("#p{}", self.unique_param);
        self.unique_param += 1;
        self.resolver.get_or_intern(&s)
    }
    fn get_binding_placement_from_idx(&self, idx_ex: Idx<Ex>) -> miette::Result<&PlacementInfo> {
        // Get the original PlacementInfo of expression that was bound to `name` in the current
        // scope.
        let Some(original_placement_info) = self.mapping.get(idx_ex) else {
            panic!("{idx_ex:?} is not mapped");
        };
        Ok(original_placement_info)
    }
    fn get_binding_placement(&self, name: Spur, span: Span) -> miette::Result<&PlacementInfo> {
        let Some(placement) = self.bindings.get(name) else {
            return Err(miette!(
                labels = vec![LabeledSpan::at(
                    self.sources.translate_span(span),
                    "referenced here"
                )],
                "no binding '{}' is in scope",
                self.resolver.resolve(&name)
            )
            .with_source_code(self.sources.clone()));
        };
        Ok(placement)
    }
}

// Functions have a type a -> b
// They also are members of the class of morphisms from origins to origins
// So e.g. (a, b, c) -> d will have #a, #b, #c, and #d will be defined in terms of this
// an origin expression can be one of three things:
//  #x . #y
//  #x & #y
//  (f #x #y ...)
// Important note: we do not allow projection, so .. -> (d, e) will have a single #(d, e) origin
// that is output
#[derive(Debug, Clone)]
pub enum OriginExpr {
    /// Origin variable
    Var(Spur),
    /// Composition of some number of origin expressions
    Comp(Vec<OriginExpr>),
    /// Intersection between some number of origin expressions
    Isect(Vec<OriginExpr>),
    /// Origin is given by application of some parameter which has a function type
    /// Note that App(s, ...) implies Var(s)
    App(Spur, Vec<OriginExpr>),
}
#[derive(Debug, Clone)]
pub struct FuncPlacement {
    /// Set of origin variables that the origin expression is quantified by
    pub vars: Vec<Spur>,
    pub expr: OriginExpr,
}
impl FuncPlacement {
    fn evaluate(&self, args_origins: &[Origin]) -> Origin {
        todo!()
    }
}

pub struct Bindings<T> {
    m: Vec<HashMap<Spur, T>>,
}
impl<T> Bindings<T> {
    pub fn insert(&mut self, spur: Spur, t: T) {
        self.m
            .last_mut()
            .expect("cannot insert into a `Bindings` with no scope")
            .insert(spur, t);
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
