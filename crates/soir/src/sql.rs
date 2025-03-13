pub mod ast_shallow;
pub mod sql_hir;

//
// fn parse_sql(
//     sql: &str,
// ) -> Result<Vec<Statement>> {
//     let ast = sqlparser::parser::Parser::new(&sqlparser::dialect::SQLiteDialect {})
//         .with_options(ParserOptions::new().with_trailing_commas(true))
//         .try_with_sql(sql)?
//         .parse_statements()?;
//     Ok(ast)
// }
//
// pub fn compile_sql(
//     sql: &str,
//     compiler: &Compiler,
// ) -> Result<()> {
//     let statements = parse_sql(sql)?;
//     let transactions = sql::ast_shallow::extract_transactions(statements)?;
//     for transaction in transactions.into_iter() {
//         sql::sql_hir::compile_transaction(transaction, compiler)?;
//     }
//
//     Ok(())
// }
