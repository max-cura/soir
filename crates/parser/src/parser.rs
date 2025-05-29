use std::{
    fmt::Write,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use chumsky::{
    Parser,
    error::Rich,
    extra::{Full, ParserExtra, SimpleState},
    input::{MapExtra, ValueInput},
    prelude::*,
};
use la_arena::{Arena, Idx};
use lasso::{Resolver, Rodeo, Spur};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::lexer::{LiteralToken, T, Token};
use telos_common::{
    source::{SourceId, Sources},
    span::{Span, Spanned},
};

/// Pattern
#[derive(Debug, Clone)]
pub enum Pat {
    Cons {
        name: Spanned<Spur>,
        args: Vec<Spanned<Pat>>,
    },
    Ident {
        name: Spanned<Spur>,
    },
    Literal {
        literal: LiteralToken,
    },
}
/// Match arm
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pat: Spanned<Pat>,
    pub body: Spanned<Idx<Expr>>,
}
#[derive(Debug, Clone)]
pub struct Def {
    pub name: Spanned<Spur>,
    pub expr: Spanned<Idx<Expr>>,
}
/// Expression
#[derive(Debug, Clone)]
pub enum Expr {
    /// If expression
    If {
        expr: Spanned<Idx<Expr>>,
        then: Spanned<Idx<Expr>>,
        else_: Spanned<Idx<Expr>>,
    },
    /// Match expression
    Match {
        expr: Spanned<Idx<Expr>>,
        arms: Vec<Spanned<MatchArm>>,
    },
    /// Non-recursive let-binding
    Let { def: Def, body: Spanned<Idx<Expr>> },
    /// Recursive let-binding
    LetRec {
        defs: Vec<Spanned<Def>>,
        body: Spanned<Idx<Expr>>,
    },
    /// Lambda expression
    Lam {
        params: Vec<Spanned<Spur>>,
        body: Spanned<Idx<Expr>>,
    },
    /// Literal
    Literal { literal: LiteralToken },
    /// Function Application
    App {
        func: Spanned<Idx<Expr>>,
        args: Vec<Spanned<Idx<Expr>>>,
    },
    /// Operator sequence (before resolution)
    OpSeq { seq: Vec<OpExpr> },
    /// Field access
    Field {
        expr: Spanned<Idx<Expr>>,
        field: Spanned<Spur>,
    },
    /// Variable reference
    Var { name: Spur },
}
#[derive(Debug, Copy, Clone)]
pub enum OpExpr {
    Expr(Spanned<Idx<Expr>>),
    Op { name: Spanned<Spur>, is_quot: bool },
}

pub fn print_pat(_pat: &Pat, _resolver: &impl Resolver, out: &mut String, indent: usize) {
    let ind = "  ".repeat(indent);
    let _ = write!(out, "{ind}<pattern>");
}

pub fn print_expr(
    ex: Idx<Expr>,
    arena: &Arena<Expr>,
    resolver: &impl Resolver,
    out: &mut String,
    indent: usize,
) {
    let ind = "  ".repeat(indent);
    match &arena[ex] {
        Expr::If { expr, then, else_ } => {
            let _ = writeln!(out, "{ind}if");
            print_expr(expr.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out, "{ind}then");
            print_expr(then.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out, "{ind}else");
            print_expr(else_.inner, arena, resolver, out, indent + 1);
        }
        Expr::OpSeq { seq } => {
            let _ = writeln!(out, "{ind}[");
            for s in seq {
                match s {
                    OpExpr::Expr(s) => {
                        print_expr(s.inner, arena, resolver, out, indent + 1);
                        let _ = writeln!(out);
                    }
                    OpExpr::Op { name, is_quot } => {
                        let quot = if *is_quot { "`" } else { "" };
                        let _ = writeln!(out, "{ind}  {quot}{}{quot}", resolver.resolve(name));
                    }
                }
            }
            let _ = write!(out, "{ind}]");
        }
        Expr::Match { expr, arms } => {
            let _ = writeln!(out, "{ind}match");
            print_expr(expr.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out, "{ind}in");
            for arm in arms {
                let _ = write!(out, "{ind}| ");
                print_pat(&arm.pat, resolver, out, indent + 1);
                let _ = writeln!(out, " ->");
                print_expr(arm.body.inner, arena, resolver, out, indent + 2);
                let _ = writeln!(out);
            }
        }
        Expr::Let { def, body } => {
            let _ = writeln!(out, "{ind}let {} =", resolver.resolve(&def.name));
            print_expr(def.expr.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out);
            let _ = writeln!(out, "{ind}in");
            print_expr(body.inner, arena, resolver, out, indent + 1);
            // let _ = writeln!(out);
        }
        Expr::LetRec { defs, body } => {
            let _ = writeln!(out, "{ind}let rec");
            for def in defs {
                let _ = write!(out, "{ind}  {} = ", resolver.resolve(&def.name));
                print_expr(def.expr.inner, arena, resolver, out, indent);
                let _ = writeln!(out);
            }
            let _ = writeln!(out, "{ind}in");
            print_expr(body.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out);
        }
        Expr::Lam { params, body } => {
            let _ = writeln!(
                out,
                "{ind}(\\{} ->",
                params
                    .iter()
                    .map(|param| resolver.resolve(&param.inner))
                    .intersperse(" ")
                    .collect::<String>()
            );
            print_expr(body.inner, arena, resolver, out, 1);
            let _ = writeln!(out);
            let _ = write!(out, "{ind})");
        }
        Expr::Literal { literal } => {
            let _ = write!(out, "{ind}");
            let _ = literal.fmt(out, resolver);
        }
        Expr::App { func, args } => {
            let _ = writeln!(out, "{ind}(");
            print_expr(func.inner, arena, resolver, out, indent + 1);
            let _ = writeln!(out);
            for arg in args {
                print_expr(arg.inner, arena, resolver, out, indent + 1);
                let _ = writeln!(out);
            }
            let _ = write!(out, "{ind})");
        }
        Expr::Field { expr, field } => {
            print_expr(expr.inner, arena, resolver, out, indent);
            let _ = write!(out, ".{}", resolver.resolve(field));
        }
        Expr::Var { name } => {
            let _ = write!(out, "{ind}{}", resolver.resolve(name));
        }
    }
}
/// Type constructor (note that `->` is a special infix constructor `Ty -> Ty -> Ty`)
#[derive(Debug)]
pub struct TyExpr {
    pub name: Spanned<Spur>,
    pub args: Vec<Spanned<Idx<TyExpr>>>,
}
/// Top-level items
#[derive(Debug)]
pub enum Item {
    /// Top-level naming
    Binding {
        name: Spanned<Spur>,
        params: Vec<Spanned<Spur>>,
        body: Spanned<Idx<Expr>>,
    },
    /// Type declaration
    TyDecl {
        name: Spanned<Spur>,
        ty_expr: Spanned<Idx<TyExpr>>,
    },
    /// Infix operator declaration
    Infix {
        operator: Spanned<Spur>,
        impl_: Spanned<Spur>,
        precedence: Spanned<u16>,
        fixity: Fixity,
    },
    /// Unary operator declaration
    Unary {
        operator: Spanned<Spur>,
        impl_: Spanned<Spur>,
        precedence: Spanned<u16>,
        side: Side,
    },
}
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Fixity {
    Left,
    Right,
    None,
}
impl std::fmt::Display for Fixity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fixity::Left => write!(f, "left"),
            Fixity::Right => write!(f, "right"),
            Fixity::None => write!(f, "non"),
        }
    }
}
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Side {
    Left,
    Right,
}
impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::Left => write!(f, "left"),
            Side::Right => write!(f, "right"),
        }
    }
}

pub struct ParserState<'r> {
    interner: &'r mut Rodeo,
    expr_arena: &'r mut Arena<Expr>,
    ty_arena: &'r mut Arena<TyExpr>,
}
impl<'r> ParserState<'r> {
    fn intern(&mut self, s: &str) -> Spur {
        self.interner.get_or_intern(s)
    }
    fn alloc_expr<'s, 'b, I, E, C>(e: Expr, ex: &mut MapExtra<'s, 'b, I, E>) -> Idx<Expr>
    where
        I: Input<'s, Span = SimpleSpan<usize, C>>,
        E: ParserExtra<'s, I, State = SimpleState<Self>>,
    {
        ex.state().expr_arena.alloc(e)
    }
    fn alloc_ty<'s, 'b, I, E, C>(t: TyExpr, ex: &mut MapExtra<'s, 'b, I, E>) -> Idx<TyExpr>
    where
        I: Input<'s, Span = SimpleSpan<usize, C>>,
        E: ParserExtra<'s, I, State = SimpleState<Self>>,
    {
        ex.state().ty_arena.alloc(t)
    }
}

#[derive(Debug, Clone)]
pub struct ParseMode {
    match_allowed: bool,
}
impl Default for ParseMode {
    fn default() -> Self {
        Self {
            match_allowed: true,
        }
    }
}

pub fn parser<'s, 'r: 's, I>() -> impl Parser<
    's,
    I,
    Vec<Spanned<Item>>,
    Full<Rich<'s, Token, Span>, SimpleState<ParserState<'r>>, ParseMode>,
>
where
    I: ValueInput<'s, Token = Token, Span = Span>,
{
    let ident = select! { Token::Ident(ident) => ident }.map_with(Spanned::from_extra);

    // -- infix item --
    let infix_item = select! {
        Token::Infix(fixity) => fixity
    }
    .then(select! {
        Token::Literal(LiteralToken::Numeric { radix, digits }) = e => {
            u16::from_str_radix(&Rodeo::resolve(<SimpleState<ParserState> as Deref>::deref(e.state()).interner, &&digits), radix as u32).unwrap()
        }
    }.labelled("precedence").map_with(Spanned::from_extra))
    .then(
        select! {
            Token::Operator(spur) => spur,
        }
        .map_with(Spanned::from_extra) ,
    )
    .then(ident.clone())
    .map_with(|(((fixity, precedence), operator), impl_), e| {
        Spanned::new(
            Item::Infix {
                operator,
                impl_,
                precedence,
                fixity,
            },
            e.span(),
        )
    })
    .labelled("infix declaration");

    // -- unary operator item --
    let unary_item = select! {
        Token::Unary(side) => side
    }.then(select! {
            Token::Literal(LiteralToken::Numeric { radix, digits }) = e => {
                u16::from_str_radix(&Rodeo::resolve(<SimpleState<ParserState> as Deref>::deref(e.state()).interner, &&digits), radix as u32).unwrap()
            }
        }.labelled("precedence").map_with(Spanned::from_extra))
        .then(
                select! {
                    Token::Operator(spur) => spur,
                                    }.map_with(Spanned::from_extra)
            )
            .then(ident.clone())
            .map_with(|(((side, precedence), operator), impl_), e| {
                    Spanned::new(Item::Unary {
                        operator, impl_, precedence, side
                    }, e.span())
                });

    // -- general utility --

    // type expression:
    //  ty := <ty> -> <ty> | <ident> [<ty>...] | ( <ty> )
    // -> is right-associative, so a -> b -> c = a -> (b -> c)
    // in which case we are nicely able to say
    //  ty-atom := <ident> [<ty> ...] | '(' <ty> ')'
    //  ty := <ty-atom> '->' <ty> | <ty-atom>
    let ty_expr = recursive(|ty_expr| {
        let atom_cons = ident
            .clone()
            .then(
                ty_expr
                    .clone()
                    .map_with(Spanned::from_extra)
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .map(|(name, args)| TyExpr { name, args })
            .map_with(ParserState::alloc_ty);
        let atom_paren = just(Token::LeftParen)
            .ignore_then(ty_expr.clone())
            .then_ignore(just(Token::RightParen));
        let ty_atom = atom_cons.or(atom_paren);
        let ty_arrow = ty_atom
            .clone()
            .map_with(Spanned::from_extra)
            .then(just(T![->]).map_with(|_, e| {
                Spanned::new(
                    ParserState::intern(
                        <SimpleState<ParserState> as DerefMut>::deref_mut(e.state()),
                        "->",
                    ),
                    e.span(),
                )
            }))
            .then(ty_expr.clone().map_with(Spanned::from_extra))
            // XXX: map().map_with(alloc_ty) would be cleaner but I got some LSP issues where map()
            // would resolve as Input::map for some reason (even if it compiled fine)
            .map_with(|((a, arrow), b), e| {
                ParserState::alloc_ty(
                    TyExpr {
                        name: arrow,
                        args: vec![a, b],
                    },
                    e,
                )
            });
        ty_arrow.or(ty_atom)
    })
    .map_with(Spanned::from_extra);

    // pattern matching
    //  pat := <pat> <pat>... | ( <pat> ) | ident | literal
    // so:
    //  pat-atom := ( <pat> ) | ident | literal
    //  pat := <ident> [ <pat-atom> ... ] | pat-atom
    let pat = recursive(|pat| {
        let pat_atom = pat
            .clone()
            .delimited_by(just(Token::LeftParen), just(Token::RightParen))
            .or(select! {
            Token::Ident(name) = e => Pat::Ident { name: Spanned::new(name, e.span()) },
            Token::Literal(literal) => Pat::Literal { literal } });
        ident
            .clone()
            .then(
                pat_atom
                    .clone()
                    .map_with(Spanned::from_extra)
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .map_with(|(name, args), _| {
                // XXX: it's not possible to disambiguate, without type information, what's a
                // constructor and what's a variable, so we do a conservative approximation:
                // we guarantee that Pat::Cons is a constructor, and Pat::Ident is either a
                // variable or a constructor
                if args.is_empty() {
                    Pat::Ident { name }
                } else {
                    Pat::Cons { name, args }
                }
            })
            .or(pat_atom)
    });

    let expr = recursive(|expr| {
        // expr := ident | literal | expr op expr | expr expr | '\' args '->' expr | if expr then expr else expr
        // | match expr in arms | let pat = expr in expr | ( expr )
        // arm := pat -> expr
        let literal = select! {
            Token::Literal(literal) => Expr::Literal { literal }
        }
        .map_with(ParserState::alloc_expr)
        .labelled("literal");
        let var = ident
            .clone()
            .map_with(|name, _| Expr::Var { name: name.inner })
            .map_with(ParserState::alloc_expr)
            .labelled("variable");
        let arm = just(T![|])
            .ignore_then(pat.clone())
            .map_with(Spanned::from_extra)
            .then_ignore(just(T![->]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .map(|(pat, body)| MatchArm { pat, body })
            .labelled("match arm");
        let match_ = just(T![match])
            .ignore_then(expr.clone().map_with(Spanned::from_extra))
            .then_ignore(just(T![in]))
            .then(
                arm.with_ctx(ParseMode {
                    match_allowed: false,
                })
                .map_with(Spanned::from_extra)
                .repeated()
                .collect::<Vec<_>>(),
            )
            .map_with(|(cond, _arms), e| {
                ParserState::alloc_expr(
                    Expr::Match {
                        expr: cond,
                        arms: vec![],
                    },
                    e,
                )
            })
            .labelled("'match'");
        let paren = expr
            .clone()
            .with_ctx(ParseMode {
                match_allowed: true,
            })
            .delimited_by(just(Token::LeftParen), just(Token::RightParen));
        let lam = just(Token::Backslash)
            .ignore_then(ident.clone().repeated().collect::<Vec<_>>())
            .then_ignore(just(T![->]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .map_with(|(params, body), _| Expr::Lam { params, body })
            .map_with(ParserState::alloc_expr);
        let if_ = just(T![if])
            .ignore_then(expr.clone().map_with(Spanned::from_extra))
            .then_ignore(just(T![then]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .then_ignore(just(T![else]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .map_with(|((expr, then), else_), _| Expr::If { expr, then, else_ })
            .map_with(ParserState::alloc_expr);
        let let_ = just(T![let])
            .ignore_then(ident.clone())
            .then_ignore(just(T![=]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .then_ignore(just(T![in]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .map_with(|((name, expr), body), _| Expr::Let { def: Def { name, expr }, body })
            .map_with(ParserState::alloc_expr);
        let let_rec = just(T![let])
            .ignore_then(just(T![rec]))
            .ignore_then(
                ident.clone().then_ignore(just(T![=]))
                .then(expr.clone().map_with(Spanned::from_extra))
                .map_with(|(name, expr), e| Spanned::from_extra(Def { name, expr }, e))
                .separated_by(just(T![,]))
                .collect::<Vec<_>>()
            )
            .then_ignore(just(T![in]))
            .then(expr.clone().map_with(Spanned::from_extra))
            .map_with(|(defs, body), _| Expr::LetRec { defs, body })
            .map_with(ParserState::alloc_expr);

        // remaining cases: expr op expr, expr expr, expr.name
        // these are both left recursive (annoyingly)
        // so we divide this into atoms:
        let expr_atom = literal
            .or(var)
            .or(match_.contextual().configure(|_cfg, ctx| ctx.match_allowed))
            .or(paren)
            .or(lam)
            .or(if_)
            .or(let_rec)
            .or(let_);
        // then parse applications:
        let app = expr_atom
            .clone()
            .map_with(Spanned::from_extra)
            .then(
                expr_atom
                    .clone()
                    .map_with(Spanned::from_extra)
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .map_with(|(func, args), e| ParserState::alloc_expr(Expr::App { func, args }, e))
            .labelled("function application");
        // then parse binopseqs
        let binopseq = select! {
            Token::Operator(op) = e => OpExpr::Op{ name: Spanned::new(op, e.span()), is_quot: false },
            Token::QuotOperator(op) = e => OpExpr::Op{ name: Spanned::new(op, e.span()), is_quot: true },
        }
        .or(Parser::map(app.clone().map_with(Spanned::from_extra), OpExpr::Expr))
        .repeated()
        .at_least(1)
        .collect::<Vec<_>>()
        .map_with(|seq, e| ParserState::alloc_expr(Expr::OpSeq { seq }, e))
        .labelled("binary operator");
        // then parse fields:
        let field = expr_atom
            .clone()
            .map_with(Spanned::from_extra)
            .then_ignore(just(T![.]))
            .then(ident.clone())
            .map_with(|(expr, field), _| Expr::Field { expr, field })
            .map_with(ParserState::alloc_expr);

        field.or(binopseq).or(app).or(expr_atom)
    })
    .labelled("expression");

    // -- type declaration --
    let type_decl_item = ident
        .clone()
        .then_ignore(just(T![:]))
        .then_ignore(just(T![:]))
        .then(ty_expr)
        .map_with(|(name, ty_expr), e| Spanned::new(Item::TyDecl { name, ty_expr }, e.span()))
        .labelled("top-level type declaration");

    // -- top-level binding --
    let binding_item = ident
        .clone()
        .then(ident.clone().repeated().collect::<Vec<_>>())
        .then_ignore(just(T![=]))
        .then(expr.clone().map_with(Spanned::from_extra))
        .map_with(|((name, params), body), _| Item::Binding { name, params, body })
        .map_with(Spanned::from_extra)
        .labelled("top-level binding");

    let item = infix_item
        .or(unary_item)
        .or(type_decl_item)
        .or(binding_item);
    let delim = just(T![;]).then(just(T![;]));
    item.recover_with(chumsky::recovery::skip_then_retry_until(
        any().and_is(delim.clone().not()).ignored(),
        end(),
    ))
    .separated_by(delim)
    .allow_trailing()
    .collect()
}

#[derive(Debug, Clone, Error, Diagnostic)]
#[error("encountered unexpected token '{found}'")]
pub struct ParseError {
    #[source_code]
    source_code: Arc<Sources>,

    found: String,

    #[label("expected {expected}")]
    span: SourceSpan,

    expected: String,
}

pub fn parse(
    sources: &Arc<Sources>,
    source_id: SourceId,
    tokens: &[Spanned<Token>],
    interner: &mut Rodeo,
    expr_arena: &mut Arena<Expr>,
    ty_arena: &mut Arena<TyExpr>,
) -> Result<Vec<Spanned<Item>>, Vec<ParseError>> {
    let len = sources.get(source_id).contents().len();
    let eoi = <Span as chumsky::span::Span>::new(source_id, len..len);

    let mut state = SimpleState(ParserState {
        interner,
        expr_arena,
        ty_arena,
    });
    let (output, errors) = parser()
        .parse_with_state(
            tokens.map(eoi, |spanned| (&spanned.inner, &spanned.span)),
            &mut state,
        )
        .into_output_errors();

    if errors.is_empty() {
        Ok(output.unwrap_or_default())
    } else {
        let mut parse_errors = vec![];
        for error in errors {
            let expected = error
                .expected()
                .map(|pat| {
                    format!(
                        "{}",
                        pat.clone()
                            .map_token(|token| token.formatted(state.interner).unwrap())
                    )
                })
                .intersperse(String::from(", "))
                .collect();
            parse_errors.push(ParseError {
                source_code: sources.clone(),
                found: error.found().map_or("EOF".to_string(), |tok| {
                    tok.formatted(state.interner).unwrap()
                }),
                span: sources.translate_span(Span::<SourceId>::new(
                    source_id,
                    error.span().start..error.span().end,
                )),
                expected,
            });
        }
        Err(parse_errors)
    }
}
