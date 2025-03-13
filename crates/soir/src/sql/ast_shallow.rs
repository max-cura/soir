use sqlparser::ast::{BeginTransactionKind, Query, Statement, TransactionMode, TransactionModifier};
use eyre::Result;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StartTransaction {
    pub modes: Vec<TransactionMode>,
    pub begin: bool,
    pub transaction: Option<BeginTransactionKind>,
    pub modifier: Option<TransactionModifier>,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Commit {
    pub chain: bool,
    pub end: bool,
    pub modifier: Option<TransactionModifier>,
}

/// Internal SQL AST type. Similar to [`sqlparser::ast::Statement`] but more limited in scope, and
/// all variants are full types (i.e. there aren't any statements whose information is captured
/// entirely inside variant fields).
pub enum PStmt {
    StartTransaction(StartTransaction),
    Commit(Commit),

    Query(Box<Query>),
    // Insert(Insert),
    // Delete(Delete),
    // Update(Update),
    // Truncate(Truncate)
}

/// Extract all transactions from a series of parsed SQL statements.
/// Transactions are either of the form `BEGIN... END` (or equivalent syntax) or single statements
/// outside such a context.
///
/// Will return an error if an unsupported statement is encountered. Supported statements are:
/// `SELECT`, `BEGIN/START`, `END/COMMIT`.
pub fn extract_transactions(statements: Vec<Statement>) -> Result<Vec<Vec<PStmt>>> {
    let mut transactions = vec![];
    let mut current_txn = vec![];
    for statement in statements.into_iter() {
        let pstmt = match statement {
            // Transaction control statements
            Statement::StartTransaction { modes, begin, transaction, modifier } => {
                PStmt::StartTransaction(StartTransaction { modes, begin, transaction, modifier })
            }
            Statement::Commit { chain, end, modifier } => {
                PStmt::Commit(Commit { chain, end, modifier })
            }

            // Query statements
            Statement::Query(query) => {
                PStmt::Query(query)
            }
            // Statement::Insert(insert) => {
            //     PStmt::Insert(insert)
            // }
            // Statement::Delete(delete) => {
            //     PStmt::Delete(delete)
            // }
            // Statement::Update { table, assignments, from, selection, returning, or } => {
            //     PStmt::Update(Update { table, assignments, from, selection, returning, or })
            // }
            // Statement::Truncate { table_names, partitions, table, only, identity, cascade, on_cluster } => {
            //     PStmt::Truncate(Truncate { table_names, partitions, table, only, identity, cascade, on_cluster })
            // }

            stmt => {
                eyre::bail!("Unsupported SQL statement: {:?}", stmt);
            }
        };
        if current_txn.is_empty() && !matches!(pstmt, PStmt::StartTransaction(..)) {
            transactions.push(vec![pstmt]);
        } else {
            let commit = matches!(pstmt, PStmt::Commit(..));
            current_txn.push(pstmt);
            if commit {
                transactions.push(std::mem::take(&mut current_txn));
            }
        }
    }
    Ok(transactions)
}
