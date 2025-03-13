pub mod anf;
pub mod untyped;
pub mod untyped_anf;
pub mod flat_anf;

use std::sync::{LazyLock};

pub static TYP_RODEO: LazyLock<lasso::ThreadedRodeo> = LazyLock::new(|| lasso::ThreadedRodeo::new());
pub static ID_RODEO: LazyLock<lasso::ThreadedRodeo> = LazyLock::new(|| lasso::ThreadedRodeo::new());

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Id(pub lasso::Spur);
impl Id {
    pub fn new(s: impl AsRef<str>) -> Self {
        Id(ID_RODEO.get_or_intern(s))
    }
}
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct TypId(pub lasso::Spur);
impl TypId {
    pub fn new(s: impl AsRef<str>) -> Self {
        TypId(TYP_RODEO.get_or_intern(s))
    }
}

#[derive(Debug)]
pub enum Typ {
    Const(TypId),
    App(TypId, Vec<Typ>)
}

#[derive(Debug, Clone)]
pub enum Constant {
    Bool(bool),
    Int(i64),
    Unit,
}
