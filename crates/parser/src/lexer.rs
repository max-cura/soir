use std::{borrow::Cow, fmt, ops::DerefMut, sync::Arc};

use chumsky::{
    Parser,
    error::Rich,
    extra::{Full, SimpleState},
    prelude::*,
    text::{keyword, unicode},
};
use lasso::{Rodeo, Spur};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use telos_common::{
    source::{SourceId, Sources},
    span::{Span, Spanned},
};

use crate::parser::{Fixity, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralToken {
    Numeric { radix: u8, digits: Spur },
    String(Spur),
    Bool(bool),
    Unit,
}
impl LiteralToken {
    pub fn fmt(&self, w: &mut impl std::fmt::Write, interner: &impl lasso::Reader) -> fmt::Result {
        match self {
            LiteralToken::Numeric { radix, digits } => {
                let prefix = match radix {
                    16 => "0x",
                    10 => "",
                    8 => "0o",
                    2 => "0b",
                    other => &format!("B{}_", other),
                };
                write!(w, "{}{}", prefix, interner.resolve(digits))
            }
            LiteralToken::String(spur) => {
                write!(w, "'{}'", interner.resolve(spur).escape_debug())
            }
            LiteralToken::Bool(b) => {
                if *b {
                    write!(w, "true")
                } else {
                    write!(w, "false")
                }
            }
            LiteralToken::Unit => write!(w, "()"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    // --- non-unit tokens ---
    Literal(LiteralToken),
    Ident(Spur),
    Operator(Spur),
    QuotOperator(Spur),

    // --- keywords ---
    If,
    Then,
    Else,
    Match,
    In,
    Let,
    Infix(Fixity),
    Unary(Side),

    // --- punctuation ---
    Eq,
    Arrow,
    Colon,
    Semicolon,
    Dot,
    Backslash,
    Pipe,

    // --- delimiters ---
    LeftParen,
    RightParen,

    // --- error recovery ---
    Error(Spur),
}
impl Token {
    pub fn fmt_tokens(
        tokens: impl IntoIterator<Item = impl AsRef<Self>>,
        interner: &impl lasso::Reader,
    ) -> Result<String, fmt::Error> {
        tokens
            .into_iter()
            .map(|token| {
                let token: &Self = token.as_ref();
                let mut s = String::new();
                token.fmt(&mut s, interner)?;
                Ok(Cow::Owned(s))
            })
            .intersperse(Ok(Cow::Borrowed(" ")))
            .try_collect::<String>()
    }
    pub fn fmt(&self, w: &mut impl std::fmt::Write, interner: &impl lasso::Reader) -> fmt::Result {
        match self {
            Token::Literal(literal_token) => literal_token.fmt(w, interner),
            Token::Ident(spur) => write!(w, "{}", interner.resolve(spur)),
            Token::Operator(spur) => write!(w, "{}", interner.resolve(spur)),
            Token::QuotOperator(spur) => write!(w, "`{}`", interner.resolve(spur)),
            Token::If => write!(w, "if"),
            Token::Then => write!(w, "then"),
            Token::Else => write!(w, "else"),
            Token::Match => write!(w, "match"),
            Token::In => write!(w, "in"),
            Token::Let => write!(w, "let"),
            Token::Infix(fixity) => match fixity {
                Fixity::None => write!(w, "infix"),
                Fixity::Left => write!(w, "infixl"),
                Fixity::Right => write!(w, "infixr"),
            },
            Token::Unary(side) => match side {
                Side::Left => write!(w, "unaryl"),
                Side::Right => write!(w, "unaryr"),
            },
            Token::Eq => write!(w, "="),
            Token::Arrow => write!(w, "->"),
            Token::Colon => write!(w, ":"),
            Token::Semicolon => write!(w, ";"),
            Token::Dot => write!(w, "."),
            Token::Backslash => write!(w, "\\"),
            Token::Pipe => write!(w, "|"),
            Token::LeftParen => write!(w, "("),
            Token::RightParen => write!(w, ")"),
            Token::Error(spur) => write!(w, "<ERROR:{}>", interner.resolve(spur)),
        }
    }
    pub fn formatted(&self, interner: &impl lasso::Reader) -> Result<String, fmt::Error> {
        let mut s = String::new();
        self.fmt(&mut s, interner)?;
        Ok(s)
    }
}
pub macro T {
    [=] => { $crate::lexer::Token::Eq },
    [->] => { $crate::lexer::Token::Arrow },
    [:] => { $crate::lexer::Token::Colon },
    [;] => { $crate::lexer::Token::Semicolon },
    [.] => { $crate::lexer::Token::Dot },
    [|] => { $crate::lexer::Token::Pipe },
    [if] => { $crate::lexer::Token::If },
    [then] => { $crate::lexer::Token::Then },
    [else] => { $crate::lexer::Token::Else },
    [match] => { $crate::lexer::Token::Match },
    [in] => { $crate::lexer::Token::In },
    [let] => { $crate::lexer::Token::Let },
    [infixl] => { $crate::lexer::Token::Infix(Fixity::Left) },
    [infixr] => { $crate::lexer::Token::Infix(Fixity::Right) },
    [infix] => { $crate::lexer::Token::Infix(Fixity::None) },
    [unaryl] => { $crate::lexer::Token::Unary(Side::Left) },
    [unaryr] => { $crate::lexer::Token::Unary(Side::Right) },
    [true] => { $crate::lexer::Token::Literal(LiteralToken::Bool(true)) },
    [false] => { $crate::lexer::Token::Literal(LiteralToken::Bool(false)) },
    [()] => { $crate::lexer::Token::Literal(LiteralToken::Unit) },
}

pub struct LexerState<'r> {
    interner: &'r mut Rodeo,
}
impl<'r> LexerState<'r> {
    fn intern(&mut self, s: &str) -> Spur {
        self.interner.get_or_intern(s)
    }
}

pub fn lexer<'s, 'r: 's>() -> impl Parser<
    's,
    &'s str,
    Vec<Spanned<Token, ()>>,
    Full<Rich<'s, char, Span<()>>, SimpleState<LexerState<'r>>, ()>,
> {
    let mut token = Recursive::declare();
    let hex_literal = just("0x")
        .ignore_then(text::digits(16).repeated().at_least(1).to_slice())
        .map_with(|digits, e| LiteralToken::Numeric {
            radix: 16,
            digits: LexerState::intern(
                <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                digits,
            ),
        })
        .labelled("hexadecimal integer literal");
    let bin_literal = just("0b")
        .ignore_then(text::digits(2).repeated().at_least(1).to_slice())
        .map_with(|digits, e| LiteralToken::Numeric {
            radix: 2,
            digits: LexerState::intern(
                <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                digits,
            ),
        })
        .labelled("binary integer literal");
    let dec_literal = text::digits(10)
        .repeated()
        .at_least(1)
        .to_slice()
        .map_with(|digits, e| LiteralToken::Numeric {
            radix: 10,
            digits: LexerState::intern(
                <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                digits,
            ),
        })
        .labelled("decimal integer literal");
    let integer_literal = hex_literal
        .or(bin_literal)
        .or(dec_literal)
        .map(Token::Literal)
        .labelled("integer literal");
    let escape = just("\\").ignore_then(select! {
        'r' => '\r',
        'n' => '\n',
        't' => '\t',
        '0' => '\0',
        '"' => '"',
    });
    let simple_string_literal = escape
        .or(none_of("\"\r"))
        .or(just("\n").then_ignore(text::inline_whitespace()).to('\n'))
        .repeated()
        .collect::<String>()
        .delimited_by(just('\"'), just('\"'))
        .labelled("simple string literal");
    let string_literal = simple_string_literal
        .map_with(|s, e| {
            Token::Literal(LiteralToken::String(LexerState::intern(
                <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                &s,
            )))
        })
        .labelled("string literal");
    let bool_literal = keyword("true")
        .to(T![true])
        .or(keyword("false").to(T![false]))
        .labelled("boolean literal");
    let unit_literal = just("()").to(T![()]).labelled("()");
    let literal = integer_literal
        .or(bool_literal)
        .or(unit_literal)
        .or(string_literal)
        .labelled("literal");
    let ident_parser = keyword("if")
        .to(T![if])
        .or(keyword("then").to(T![then]))
        .or(keyword("else").to(T![else]))
        .or(keyword("match").to(T![match]))
        .or(keyword("in").to(T![in]))
        .or(keyword("let").to(T![let]))
        .or(keyword("infixl").to(T![infixl]))
        .or(keyword("infixr").to(T![infixr]))
        .or(keyword("infix").to(T![infix]))
        .or(keyword("unaryl").to(T![unaryl]))
        .or(keyword("unaryr").to(T![unaryr]))
        .labelled("keyword")
        .or((unicode::ident().or(just(".")))
            .repeated()
            .at_least(1)
            .to_slice()
            .map_with(|ident, e| {
                Token::Ident(LexerState::intern(
                    <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                    ident,
                ))
            })
            .labelled("identifier"));
    let punct_parser = just("=")
        .to(T![=])
        .or(just("->").to(T![->]))
        .or(just(":").to(T![:]))
        .or(just(";").to(T![;]))
        .or(just(".").to(T![.]))
        .or(just("|").to(T![|]))
        .or(just("\\").to(Token::Backslash))
        .or(just("(").to(Token::LeftParen))
        .or(just(")").to(Token::RightParen));
    let standard_op_parser = one_of("/=-+!*%<>&|^?~.:#$")
        .repeated()
        .at_least(1)
        .to_slice()
        .map_with(|op, e| {
            Token::Operator(LexerState::intern(
                <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                op,
            ))
        })
        .labelled("unquoted operator");
    let quot_op_parser = any()
        .and_is(just('`').not())
        .repeated()
        .at_least(1)
        .to_slice()
        .delimited_by(just("`"), just("`"))
        .map_with(|op, e| {
            Token::QuotOperator(LexerState::intern(
                <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                op,
            ))
        })
        .labelled("quote operator");
    let op_parser = standard_op_parser.or(quot_op_parser).labelled("operator");

    // requirements:
    //  - literal < tt_parser because of ()
    //  - literal < ident because of true/false
    //  - punct_parser < op_parser because all punctuation are also valid operator characters
    token.define(
        literal
            .clone()
            .or(ident_parser)
            .or(punct_parser)
            .or(op_parser),
    );

    let line_comment_parser = just("--")
        .then(any().and_is(just('\n').not()).repeated())
        .padded()
        .ignored()
        .labelled("line comment");
    // let block_comment_parser = //recursive(|block_comment_parser| {
    //     any()
    //         .and_is(just("(*").not())
    //         //.or_not()
    //         .ignored()
    //         //.padded_by(block_comment_parser)
    //         .repeated()
    //         .delimited_by(just("(*"), just("*)"))
    //         .padded()
    // //})
    // .labelled("block comment");
    // let block_comment_parser = just("{*")
    //     .ignore_then(
    //         any() //.and_is(just("(*").not()))
    //             .repeated()
    //             .collect::<Vec<_>>()
    //             .ignored(),
    //     )
    //     .then_ignore(just("*}"))
    //     .padded();

    let comment_parser = line_comment_parser
        // .or(block_comment_parser)
        .labelled("comment");

    token
        .clone()
        .map_with(|token, e| Spanned::new(token, e.span()))
        // recovery strategy: wait for next valid token - this works pretty well for the lexer,
        // since we want to pull out all valid tokens. note that we do an extra post processing
        // step to merge consecutive error tokens
        .recover_with(via_parser(any().and_is(token.not()).to_slice().map_with(
            |s, e| {
                Spanned::new(
                    Token::Error(LexerState::intern(
                        <SimpleState<LexerState> as DerefMut>::deref_mut(e.state()),
                        s,
                    )),
                    e.span(),
                )
            },
        )))
        // XXX: do this after the recovery to ensure that we don't emit errors for end-of-input or
        // whitespace after lexer errors
        .padded_by(comment_parser.repeated())
        .padded()
        .repeated()
        .collect()
}

#[derive(Debug, Clone, Error, Diagnostic)]
#[error("invalid token")]
pub struct LexerError {
    #[source_code]
    source_code: Arc<Sources>,

    #[label("expected {expected}")]
    span: SourceSpan,

    expected: String,
}

pub fn lex_source<'s>(
    sources: &Arc<Sources>,
    source_id: SourceId,
    interner: &mut Rodeo,
) -> Result<Vec<Spanned<Token>>, Vec<LexerError>> {
    let source = sources.get(source_id);
    let mut state = SimpleState(LexerState { interner });
    let lexer = lexer();
    let (output, errors) = lexer
        .parse_with_state(source.contents(), &mut state)
        .into_output_errors();
    if errors.is_empty() {
        Ok(output
            .map(|output| {
                output
                    .into_iter()
                    .map(|s| -> Spanned<Token> { s.map_context(|_| source_id) })
                    .collect()
            })
            .unwrap_or_default())
    } else {
        // merge errors
        let mut lexer_errors = vec![];
        for error in errors {
            let expected = error
                .expected()
                .map(|pat| pat.to_string())
                .filter(|pat| pat != "comment" && pat != "''*''")
                .intersperse(String::from(", "))
                .collect();
            lexer_errors.push(LexerError {
                source_code: sources.clone(),
                span: sources
                    .translate_span(Span::<SourceId>::new(source_id, error.span().into_range())),
                expected,
            });
        }
        lexer_errors.sort_by_key(|e| e.span);
        // XXX: properly, we ought to do some kind of merging of expectations, but right now we just
        // indiscriminately delete the all errors except the first.
        lexer_errors.dedup_by_key(|e| e.span);
        Err(lexer_errors)
    }
}
