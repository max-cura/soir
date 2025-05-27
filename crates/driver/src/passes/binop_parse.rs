use std::collections::HashMap;

use chumsky::{
    Parser,
    input::ValueInput,
    pratt::{Operator, infix, left},
    prelude::*,
    select,
};
use either::Either;
use la_arena::{Arena, Idx};
use lasso::Spur;
use miette::SourceSpan;
use telos_common::{
    source::{SourceId, Sources},
    span::{Span, Spanned},
};
use telos_parser::parser::{Expr, Fixity, Item, OpExpr};

pub fn run(
    items: &[Spanned<Item>],
    arena: &mut Arena<Expr>,
    rodeo: &impl lasso::Resolver,
    sources: &Sources,
) -> Result<(), Vec<OperatorError>> {
    let binop_info = get_operator_info(items);
    let mut errors = vec![];
    for item in items.iter() {
        if let Item::Binding {
            name: _,
            params: _,
            body,
        } = item.inner
        {
            visit_exprs(body, arena, &binop_info, &mut errors, rodeo, sources);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn get_operator_info(items: &[Spanned<Item>]) -> HashMap<Spur, (u16, Fixity)> {
    items
        .into_iter()
        .filter_map(|item| match &item.inner {
            Item::Infix {
                operator,
                impl_: _,
                precedence,
                fixity,
            } => Some((operator.inner, (*precedence, *fixity))),
            _ => None,
        })
        .collect()
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum OperatorError {
    #[error("operator '{op0_name}' is not associative")]
    Nonassociative {
        #[label("used here")]
        op0: SourceSpan,
        op0_name: String,
        #[label("other operator '{op1_name}' used here")]
        op1: SourceSpan,
        op1_name: String,
    },
    #[error("undeclared operator '{name}'")]
    Undeclared {
        #[label("used here")]
        op: SourceSpan,
        name: String,
    },
    #[error("mismatched associativities: '{op0_name}' and '{op1_name}'")]
    EqualBinding {
        #[label("'{op0_name}' is {op0_assoc}-associative with precedence {op0_prec}")]
        op0: SourceSpan,
        op0_name: String,
        op0_assoc: Fixity,
        op0_prec: u16,
        #[label("'{op1_name}' is {op1_assoc}-associative with precedence {op1_prec}")]
        op1: SourceSpan,
        op1_name: String,
        op1_assoc: Fixity,
        op1_prec: u16,
    },
}

fn visit_exprs(
    // root expr
    root: Spanned<Idx<Expr>>,
    // need to allocate new Expr::BinOp's
    arena: &mut Arena<Expr>,
    // binary operator information
    binop_info: &HashMap<Spur, (u16, Fixity)>,
    // errors discovered during parsing
    errors: &mut Vec<OperatorError>,
    // used for some error messages
    rodeo: &impl lasso::Resolver,
    sources: &Sources,
) {
    // generate Pratt parser

    let mut queue = vec![root];

    while let Some(expr) = queue.pop() {
        match &arena[expr.inner] {
            // -- primary --
            Expr::BinOpSeq { seq } => {
                // fixity: left -> a + b + c = (a + b) + c
                // fixity: right -> a + b + c = a + (b + c)
                // fixity: none -> a + b * c = <error> iff precedence is same on + and *

                // 1. Retrieve operator information, and check for undeclared operators.
                let result = seq
                    .iter()
                    .enumerate()
                    .filter_map(|(i, op)| match op {
                        OpExpr::Op {
                            name: Spanned { inner: name, span },
                            is_quot,
                        } => Some((
                            i,
                            Spanned {
                                inner: (*name, *is_quot),
                                span: *span,
                            },
                        )),
                        _ => None,
                    })
                    .map(|(i, op)| {
                        let (spur, is_quot) = &op.inner;
                        if *is_quot {
                            Ok((i, format!("`{}`", rodeo.resolve(spur)), Fixity::None))
                        } else {
                            if let Some((_, fixity)) = binop_info.get(spur) {
                                Ok((i, rodeo.resolve(spur).to_string(), *fixity))
                            } else {
                                Err(OperatorError::Undeclared {
                                    op: sources.translate_span(op.span),
                                    name: rodeo.resolve(spur).to_string(),
                                })
                            }
                        }
                    })
                    .fold(Ok(Vec::new()), |a, n| match n {
                        Ok(t) => a.map(|mut v| {
                            v.push(t);
                            v
                        }),
                        Err(e) => match a {
                            Ok(_) => Err(vec![e]),
                            Err(mut v) => {
                                v.push(e);
                                Err(v)
                            }
                        },
                    });
                let opsinfo = match result {
                    Ok(v) => v,
                    Err(e) => {
                        errors.extend(e);
                        continue;
                    }
                };
                // 2. need to catch non-associatives
                if let Some((i, (ii, name, _))) = opsinfo
                    .iter()
                    .enumerate()
                    .find(|(_, (_, _, fixity))| matches!(fixity, Fixity::None))
                    && opsinfo.len() > 1
                {
                    let j = if i == 0 { 1 } else { 0 };
                    let jj = opsinfo[j].0;
                    errors.push(OperatorError::Nonassociative {
                        op0: sources.translate_span(seq[*ii].span),
                        op0_name: name.clone(),
                        op1: sources.translate_span(seq[jj].span),
                        op1_name: opsinfo[j].1.clone(),
                    });
                    continue;
                }
                // 2.5: do we need to catch left-right conflicts?
                // 3. Pratt parsing
                let end = seq.last().unwrap().span.end;
            }

            // -- propagation-only --
            Expr::If { expr, then, else_ } => {
                queue.push(*expr);
                queue.push(*then);
                queue.push(*else_);
            }
            Expr::Match { expr, arms } => {
                queue.push(*expr);
                for arm in arms {
                    queue.push(arm.inner.body);
                }
            }
            Expr::Let {
                name: _,
                expr,
                body,
            } => {
                queue.push(*expr);
                queue.push(*body);
            }
            Expr::Lam { params: _, body } => {
                queue.push(*body);
            }
            Expr::App { func: _, args } => {
                queue.extend_from_slice(&args);
            }

            // -- do nothing --
            Expr::Literal { literal: _ } => {}
            Expr::Var { name: _ } => {}

            // -- error if we run into this --
            Expr::BinOp { .. } => {
                panic!("reached BinOp in binop_parse::visit_exprs; this should never happen")
            }
        }
    }
}
