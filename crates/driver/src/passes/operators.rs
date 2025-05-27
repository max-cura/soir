use std::{collections::HashMap, iter::once};

use chumsky::span::Span;
use la_arena::{Arena, Idx};
use lasso::{Resolver, Spur};
use miette::{Diagnostic, SourceSpan};
use telos_common::{source::Sources, span::Spanned};
use telos_parser::parser::{Expr, Fixity, Item, OpExpr, Side};
use thiserror::Error;

#[derive(Default)]
struct OperatorSet {
    /// Set of infix binary operators declared in the program
    infix_binary: HashMap<Spur, Spanned<(u16, Fixity)>>,
    /// Set of unary prefix operators declared in the program
    left_unary: HashMap<Spur, Spanned<u16>>,
    /// Set of unary postfix operators declared in the program
    right_unary: HashMap<Spur, Spanned<u16>>,
}

/// Get the set of operators declared in a program; check for _ambiguous_ duplications and return
/// those as errors.
fn get_operator_info(
    items: &[Spanned<Item>],
    resolver: &impl Resolver,
    sources: &Sources,
) -> (OperatorSet, Vec<OperatorError>) {
    let mut infix_binary: HashMap<Spur, Spanned<(u16, Fixity)>> = HashMap::default();
    let mut left_unary: HashMap<Spur, Spanned<u16>> = HashMap::default();
    let mut right_unary: HashMap<Spur, Spanned<u16>> = HashMap::default();

    let mut errors = vec![];

    for Spanned { inner, span } in items.into_iter() {
        match inner {
            Item::Binding { .. } | Item::TyDecl { .. } => continue,
            Item::Infix {
                operator,
                impl_: _,
                precedence,
                fixity,
            } => {
                if let Some(first) = infix_binary.get(&operator.inner) {
                    // infix operator already declared
                    errors.push(OperatorError::Redeclaration {
                        operator: resolver.resolve(&operator.inner).to_string(),
                        first: sources.translate_span(first.span),
                        second: sources.translate_span(*span),
                    });
                } else {
                    infix_binary.insert(
                        operator.inner,
                        Spanned::new((precedence.inner, *fixity), *span),
                    );
                }
            }
            Item::Unary {
                operator,
                impl_: _,
                precedence,
                side: Side::Left,
            } => {
                if let Some(first) = left_unary.get(&operator.inner) {
                    errors.push(OperatorError::Redeclaration {
                        operator: resolver.resolve(&operator.inner).to_string(),
                        first: sources.translate_span(first.span),
                        second: sources.translate_span(*span),
                    });
                } else {
                    left_unary.insert(operator.inner, Spanned::new(precedence.inner, *span));
                }
            }
            Item::Unary {
                operator,
                impl_: _,
                precedence,
                side: Side::Right,
            } => {
                if let Some(first) = right_unary.get(&operator.inner) {
                    errors.push(OperatorError::Redeclaration {
                        operator: resolver.resolve(&operator.inner).to_string(),
                        first: sources.translate_span(first.span),
                        second: sources.translate_span(*span),
                    });
                } else {
                    right_unary.insert(operator.inner, Spanned::new(precedence.inner, *span));
                }
            }
        }
    }

    (
        OperatorSet {
            infix_binary,
            left_unary,
            right_unary,
        },
        errors,
    )
}

#[derive(Debug, Error, Diagnostic)]
pub enum OperatorError {
    #[error("operator {operator} declared multiple times")]
    Redeclaration {
        operator: String,
        #[label("first declared here")]
        first: SourceSpan,
        #[label("declared again here")]
        second: SourceSpan,
    },
    #[error("expected operator or end of input")]
    UnexpectedExpression {
        #[label("found expression")]
        span: SourceSpan,
    },
    #[error("operator '{operator}' expected expression")]
    ExpectedExpression {
        operator: String,
        #[label("... but no expression follows")]
        op_span: SourceSpan,
    },
    #[error("'{operator}' is not a {fix} operator")]
    NotXFix {
        operator: String,
        fix: String,
        #[label("referenced here")]
        op_span: SourceSpan,
    },
}

pub struct OperatorCtx<'a, R> {
    operators: &'a OperatorSet,
    arena: &'a mut Arena<Expr>,
    resolver: &'a R,
    sources: &'a Sources,
}
impl<R: Resolver> OperatorCtx<'_, R> {
    pub fn infix_binding_power(&self, operator: Spur) -> Option<(u32, u32)> {
        let Spanned {
            inner: (precedence, fixity),
            span: _,
        } = self.operators.infix_binary.get(&operator)?;
        let base = *precedence as u32 * 2;
        let (left, right) = match fixity {
            Fixity::Left => (base, base + 1),
            Fixity::Right => (base + 1, base),
            Fixity::None => (base, base),
        };
        Some((left, right))
    }
    pub fn prefix_binding_power(&self, operator: Spur) -> Option<u32> {
        self.operators
            .left_unary
            .get(&operator)
            .map(|spanned| spanned.inner as u32 * 2)
    }
    pub fn postfix_binding_power(&self, operator: Spur) -> Option<u32> {
        self.operators
            .right_unary
            .get(&operator)
            .map(|spanned| spanned.inner as u32 * 2)
    }
}

pub fn resolve_all_operators<R: Resolver>(
    items: &[Spanned<Item>],
    resolver: &R,
    sources: &Sources,
    arena: &mut Arena<Expr>,
) -> Result<(), Vec<OperatorError>> {
    let (operator_set, mut errors) = get_operator_info(items, resolver, sources);
    if !errors.is_empty() {
        return Err(errors);
    }
    for item in items {
        let Item::Binding {
            name: _,
            params: _,
            body,
        } = item.inner
        else {
            continue;
        };
        if let Err(e) = resolve_operators(
            body,
            OperatorCtx {
                operators: &operator_set,
                arena,
                resolver,
                sources,
            },
        ) {
            errors.push(e);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(())
}

pub fn resolve_operators<R: Resolver>(
    expr: Spanned<Idx<Expr>>,
    mut ctx: OperatorCtx<R>,
) -> Result<(), OperatorError> {
    // Visitor for `Expr`s that allows us to recursively visit expressions. Order doesn't
    // particularly matter as long as `Idx`s don't change (they don't).
    let mut queue: Vec<Spanned<Idx<Expr>>> = vec![expr];

    while let Some(expr) = queue.pop() {
        // First, push all of the sub-expressions
        match &ctx.arena[expr.inner] {
            Expr::OpSeq { seq } => queue.extend(seq.iter().filter_map(|op_expr| match op_expr {
                OpExpr::Expr(spanned) => Some(spanned),
                OpExpr::Op { .. } => None,
            })),
            Expr::If { expr, then, else_ } => queue.extend([*expr, *then, *else_].into_iter()),
            Expr::Match { expr, arms } => {
                queue.extend(once(*expr).chain(arms.iter().map(|arm| arm.inner.body)))
            }
            Expr::Let {
                name: _,
                expr,
                body,
            } => queue.extend([*expr, *body].into_iter()),
            Expr::Lam { params: _, body } => queue.push(*body),
            Expr::App { func: _, args } => queue.extend_from_slice(&args),
            Expr::Literal { literal: _ } | Expr::Var { name: _ } => {}
            Expr::BinOp { .. } => {
                unreachable!("Reached BinOp in operators::resolve_operators")
            }
            Expr::Unary { .. } => {
                unreachable!("Reached Unary in operators::resolve_operators")
            }
        }

        // Next, if it's an operator sequence, grab it
        let Expr::OpSeq { seq: op_seq } = &mut ctx.arena[expr.inner] else {
            continue;
        };
        let mut op_seq = std::mem::take(op_seq).into_iter();

        let Some(new_root_expr) = pratt(&mut op_seq, &mut queue, &mut ctx, 0)? else {
            unreachable!("expected OpSeq to be non-empty")
        };

        // note: "span" hasn't actually changed, so we just use the Expr itself
        ctx.arena[expr.inner] = new_root_expr.inner;
    }

    Ok(())
}
// The actual Pratt parsing machinery
fn pratt<R: Resolver>(
    base_op_seq: &mut dyn Iterator<Item = OpExpr>,
    queue: &mut Vec<Spanned<Idx<Expr>>>,
    ctx: &mut OperatorCtx<R>,
    min_power: u32,
) -> Result<Option<Spanned<Expr>>, OperatorError> {
    let mut op_seq = base_op_seq.peekable();
    let mut lhs = match op_seq.next() {
        // Case: expression atom
        Some(OpExpr::Expr(expr)) => {
            queue.push(expr);
            expr.map(|expr_idx| ctx.arena[expr_idx].clone())
        }
        // Case: prefix operator
        Some(OpExpr::Op { name, is_quot: _ }) => {
            let Some(power) = ctx.prefix_binding_power(*name) else {
                return Err(OperatorError::NotXFix {
                    operator: ctx.resolver.resolve(&name).to_string(),
                    fix: "prefix".to_string(),
                    op_span: ctx.sources.translate_span(name.span),
                });
            };
            let Some(rhs) = pratt(&mut op_seq, queue, ctx, power)? else {
                return Err(OperatorError::ExpectedExpression {
                    operator: ctx.resolver.resolve(&name).to_owned(),
                    op_span: ctx.sources.translate_span(name.span),
                });
            };
            let combined_span = name.span.union(rhs.span);
            Spanned::new(
                Expr::Unary {
                    operator: name,
                    expr: rhs.map(|rhs| ctx.arena.alloc(rhs)),
                },
                combined_span,
            )
        }
        None => {
            // can only happen if operator should be followed by expression but was not
            return Ok(None);
        }
    };

    loop {
        let (operator, is_quot) = match op_seq.next() {
            Some(OpExpr::Op { name, is_quot }) => (name, is_quot),
            None => break, // end of input
            Some(OpExpr::Expr(expr)) => {
                return Err(OperatorError::UnexpectedExpression {
                    span: ctx.sources.translate_span(expr.span),
                });
            }
        };

        if let Some(power) = ctx.postfix_binding_power(operator.inner) {
            if power < min_power {
                break;
            }
            let combined_span = lhs.span.union(operator.span);
            lhs = Spanned::new(
                Expr::Unary {
                    operator: operator,
                    expr: lhs.map(|lhs| ctx.arena.alloc(lhs)),
                },
                combined_span,
            );
            continue;
        }

        let Some((l_power, r_power)) = ctx.infix_binding_power(operator.inner) else {
            // error: not actually an infix operator OR a postfix operator; we can check
            // the following token to determine which we _expect_
            let next_next = op_seq.peek();
            let expected_postfix = match next_next {
                // if next "token" is an expression, then this was probably supposed to be
                // an infix
                Some(OpExpr::Expr(..)) => false,
                // otherwise, if the next "token" is end-of-input or another operator, then
                // this was most likely supposed to be a postfix
                None | Some(OpExpr::Op { .. }) => true,
            };
            return Err(OperatorError::NotXFix {
                operator: ctx.resolver.resolve(&operator).to_owned(),
                fix: if expected_postfix { "postfix" } else { "infix" }.to_string(),
                op_span: ctx.sources.translate_span(operator.span),
            });
        };
        if l_power < min_power {
            break;
        }
        let Some(rhs) = pratt(&mut op_seq, queue, ctx, r_power)? else {
            return Err(OperatorError::ExpectedExpression {
                operator: ctx.resolver.resolve(&operator).to_owned(),
                op_span: ctx.sources.translate_span(operator.span),
            });
        };

        let combined_span = lhs.span.union(rhs.span);
        lhs = Spanned::new(
            Expr::BinOp {
                operator,
                is_quot_operator: is_quot,
                lhs: lhs.map(|lhs| ctx.arena.alloc(lhs)),
                rhs: rhs.map(|rhs| ctx.arena.alloc(rhs)),
            },
            combined_span,
        );
    }
    Ok(Some(lhs))
}
