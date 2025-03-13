use std::rc::Rc;
use std::sync::Arc;

pub struct Compiler {
    backend: Arc<dyn Backend>
}

impl Compiler {
    pub fn new<B: Backend + 'static>(backend: B) -> Self {
        Self {
            backend: Arc::new(backend)
        }
    }
    pub fn backend(&self) -> Arc<dyn Backend> {
        Arc::clone(&self.backend)
    }
}

/// Limited type constructor for SQL
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TyCons {
    Integer,
    Text,
    Blob,
    Boolean,
    Tuple,
}

/// Represents the type `cons [params...]`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Ty {
    pub cons: TyCons,
    pub params: Vec<Ty>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Column {
    pub name: String,
    pub ty: Ty,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum NaturalOrder {
    Ascending,
    Descending,
    Unordered,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Index {
    pub key_columns: Vec<Rc<Column>>,
    pub key_ty: Ty,
    pub value_columns: Vec<Rc<Column>>,
    pub value_ty: Ty,
    pub natural_order: NaturalOrder
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Rc<Column>>,
    pub indices: Vec<Index>,
    pub row_ty: Ty,
}

pub trait Backend {
    fn tables(&self) -> Vec<Table>;
}