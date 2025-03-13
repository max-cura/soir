// use sqlparser::ast::{Expr, Ident, JoinOperator, ObjectName, Query, Select, SelectItem, SetExpr, Statement, Values};
// use crate::compiler::Compiler;
// use crate::sql::ast_shallow::PStmt;
//
// #[derive(Debug, Copy, Clone, Eq, PartialEq)]
// pub enum CompoundOperator {
//     Union,
//     UnionAll,
//     Except,
//     Intersect,
// }
//
// #[derive(Debug, Clone)]
// pub enum CompoundOperation {
//     Values(Box<Values>),
//     Select(Box<Select>),
//     Operation {
//         operator: CompoundOperator,
//         left: Box<CompoundOperation>,
//         right: Box<CompoundOperation>,
//     }
// }
//
// fn compile_query(query: &Query, compiler: &Compiler) -> eyre::Result<Vec<Statement>> {
//     if query.fetch.is_some() {
//         eyre::bail!("FETCH clauses are not supported");
//     }
//     if !query.locks.is_empty() {
//         eyre::bail!("SKIP LOCKED/NOWAIT is not supported");
//     }
//     if query.for_clause.is_some() {
//         eyre::bail!("FOR XML/JSON clauses are not supported");
//     }
//     if query.settings.is_some() {
//         eyre::bail!("SETTINGS clauses are not supported");
//     }
//     if query.format_clause.is_some() {
//         eyre::bail!("FORMAT clauses are not supported");
//     }
//
//     // remaining properties:
//     // - with: Option<With>
//     // - body: Box<SetExpr>
//     // - order_by: Option<OrderBy>
//     // - limit: Option<Expr>
//     // - limit_by: Vec<Expr>
//     // - offset: Option<Offset>
//
//     // codegen template:
//     //  - evaluate expression in limit
//     //  - determine order_by
//     //  - make CTEs available
//     //  - parse body
//     //      - SetExpr::Select, SetExpr::Values
//     //      - SetExpr::Query just re-invokes
//     //      - SetOperation is only allowed to be ALL, and only then for UNION
//     //      - SetOperator::Minus = SetOperator::Except
//     //  ignore: SetExpr::Table (postgres), SetExpr::Insert + ::Update (no idea)
//     match query.body.as_ref() {
//         SetExpr::Select(select) => {
//             // .distinct : Option<Distinct>
//             // .projection : Vec<SelectItem>
//             // .from : Vec<TableWithJoins>
//             // .selection : Option<Expr>
//             // .group_by : GroupByExpr
//             // .having : Option<Expr>
//             let ss = SimpleSelect {
//                 distinct: select.distinct.is_some(),
//                 projection: select.projection.into_iter().map(|si| match si {
//                     SelectItem::UnnamedExpr(expr) => {
//                         Projection::Expr(expr)
//                     }
//                     SelectItem::ExprWithAlias { expr, alias } => {
//                         Projection::AliasedExpr(expr, alias)
//                     }
//                     SelectItem::QualifiedWildcard(obj, _) => {
//                         Projection::QualifiedAll(obj)
//                     }
//                     SelectItem::Wildcard(_) => {
//                         Projection::All
//                     }
//                 }).collect(),
//                 from: From {
//                     factors
//                 }
//             }
//         }
//         _ => panic!()
//     }
//
//     todo!()
// }
//
// enum Projection {
//     All,
//     QualifiedAll(ObjectName),
//     Expr(Expr),
//     AliasedExpr(Expr, Ident),
// }
// enum TableLike {
//     Table {
//         name: ObjectName,
//         alias: Option<TableAlias>,
//     },
// }
// struct TableWithJoins {
//     factors: Vec<TableLike>,
//     join_operators: Vec<JoinOperator>,
// }
// struct SimpleSelect {
//     distinct: bool,
//     projection: Vec<Projection>,
//     from: From,
//     selection: Option<Expr>,
//     group_by: Vec<Expr>,
//     having: Option<Expr>,
// }
//
// pub fn compile_transaction(transaction: Vec<PStmt>, compiler: &Compiler) -> eyre::Result<()> {
//     for (_i, pstmt) in transaction.iter().enumerate() {
//         //
//         match pstmt {
//             PStmt::StartTransaction(_st) => {
//                 // we're not very interested in this atm
//             }
//             PStmt::Commit(_) => {
//                 // also not very interested
//             }
//             PStmt::Query(query) => {
//                 // okay, we're in business
//                 compile_query(query.as_ref(), compiler)?;
//             }
//         }
//     }
//     Ok(())
// }
