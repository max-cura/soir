use chumsky::extra::Full;
use chumsky::prelude::*;
use chumsky::text::{ident, int, keyword};
use crate::ast::{Constant, Id};
use crate::ast::untyped::{Expr, Pat};

pub fn from_str(t: &str) -> ParseResult<Box<Expr>, Rich<char>> {
    parser()
        .parse(t)
}

pub fn parser<'a>() -> impl Parser<'a, &'a str, Box<Expr>, Full<Rich<'a, char>, (), ()>> {
    let constant = choice((
        int(10).map(|x| Constant::Int(i64::from_str_radix(x, 10).unwrap())),
        keyword("true").map(|_| Constant::Bool(true)),
        keyword("false").map(|_| Constant::Bool(false)),
        keyword("()").map(|_| Constant::Unit),
    )).padded();
    let kw = choice((just("in"),just("let"),just("match")));
    let ident_base = one_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_-'.$")
        .repeated()
        .at_least(1)
        .collect::<String>();
    let var = ident_base.and_is(kw.not()).padded().map(Id::new);
    recursive(|expr| {
        let let_ = keyword("let").padded()
            .ignore_then(var.clone())
            .then_ignore(just("=").padded())
            .then(expr.clone())
            .then_ignore(keyword("in").padded())
            .then(expr.clone())
            .padded()
            .map(|((id, e1), e2)| {
                Box::new(Expr::Let(id, e1, e2))
            });
        let lam = just("\\").padded()
            .ignore_then(var.clone().padded().separated_by(just(",").padded().ignored()).collect::<Vec<_>>())
            .then_ignore(just("->").padded())
            .then(expr.clone())
            .padded()
            .map(|(args,body)| {
                Box::new(Expr::Lambda(args, body))
            });
        let const_ = constant.clone().map(Expr::Constant).map(Box::new);
        let var_ = var.clone().map(Expr::Var).map(Box::new);
        let non_greedy_expr = choice((
            const_.clone(),
            var_.clone(),
            expr.clone().delimited_by(just("(").padded(), just(")").padded())
        )).padded();
        let if_ = keyword("if").padded()
            .ignore_then(non_greedy_expr.clone())
            .then_ignore(keyword("then").padded())
            .then(non_greedy_expr.clone())
            .then_ignore(keyword("else").padded())
            .then(non_greedy_expr.clone())
            .padded()
            .map(|((e0, e1), e2)| {
                Box::new(Expr::If(e0, e1, e2))
            });
        let pattern = recursive(|pat| {
            let pat_const = constant.clone().map(Pat::Constant);
            let pat_var = var.clone().map(Pat::Var);
            let paren_pat = pat.delimited_by(just("(").padded(), just(")").padded()).padded();
            let cons_arg = choice((
                pat_const.clone(),
                pat_var.clone(),
                paren_pat
                )).padded();
            let pat_cons =
                var.clone().then(cons_arg.repeated().at_least(1).collect::<Vec<_>>())
                    .map(|(cons, args)| {
                        Pat::Cons(cons, args)
                    }).padded();
            choice((
                pat_const,
                pat_cons,
                pat_var,
            ))
                .padded()
        });
        let match_arm = pattern
            .then_ignore(just("->").padded())
            .then(expr.clone())
            .padded()
            .map(|(pat, expr)| (pat, *expr));
        let match_ = keyword("match").padded()
            .ignore_then(non_greedy_expr.clone())
            .then_ignore(keyword("in").padded())
            .then(match_arm
                .separated_by(just(",").padded())
                .at_least(1)
                .collect::<Vec<_>>())
            .map(|(e0, arms)| {
                Box::new(Expr::Match(e0, arms))
            });
        let app = non_greedy_expr.clone().then(non_greedy_expr.clone().repeated().at_least(1).collect::<Vec<_>>().padded())
            .padded()
            .map(|(f, x)| {
                Box::new(Expr::App(f, x.into_iter().map(|x| *x).collect()))
            });
        choice((
            let_,
            lam,
            if_,
            match_,
            const_,
            app,
            var_,
        ))
            .padded()
    }).then_ignore(end())
}

#[cfg(test)]
mod tests {
    use chumsky::Parser;
    use crate::parse;

    #[test]
    fn test() {
        let inputs = &[
            "26",
            "let x = 0 in 2",
            "add x y",
            "add (add x y) z",
            "\\x -> add x x",
            "if true then d else 1",
            "match x in Cons x -> 1, 1 -> 2",
            "match x in Just (Just y) -> 0, Just z -> 1, Nothing -> w"
        ];
        let outputs = &[
        "ParseResult { output: Some(Constant(Int(26))), errs: [] }",
        "ParseResult { output: Some(Let(Id(Spur(1)), Constant(Int(0)), Constant(Int(2)))), errs: [] }",
        "ParseResult { output: Some(App(Var(Id(Spur(2))), [Var(Id(Spur(1))), Var(Id(Spur(3)))])), errs: [] }",
        "ParseResult { output: Some(App(Var(Id(Spur(2))), [App(Var(Id(Spur(2))), [Var(Id(Spur(1))), Var(Id(Spur(3)))]), Var(Id(Spur(4)))])), errs: [] }",
        "ParseResult { output: Some(Lambda([Id(Spur(1))], App(Var(Id(Spur(2))), [Var(Id(Spur(1))), Var(Id(Spur(1)))]))), errs: [] }",
        "ParseResult { output: Some(If(Constant(Bool(true)), Var(Id(Spur(5))), Constant(Int(1)))), errs: [] }",
        "ParseResult { output: Some(Match(Var(Id(Spur(1))), [(Cons(Id(Spur(6)), [Var(Id(Spur(1)))]), Constant(Int(1))), (Constant(Int(1)), Constant(Int(2)))])), errs: [] }",
        "ParseResult { output: Some(Match(Var(Id(Spur(1))), [(Cons(Id(Spur(7)), [Cons(Id(Spur(7)), [Var(Id(Spur(3)))])]), Constant(Int(0))), (Cons(Id(Spur(7)), [Var(Id(Spur(4)))]), Constant(Int(1))), (Var(Id(Spur(8))), Var(Id(Spur(9))))])), errs: [] }",
        ];
        for (i, input) in inputs.iter().enumerate() {
            let p = parse::parser().parse(input);
            assert_eq!(format!("{p:?}"), outputs[i]);
        }
    }
}
