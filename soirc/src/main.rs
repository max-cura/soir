#![feature(impl_trait_in_assoc_type)]

use std::sync::atomic::AtomicUsize;
use crate::ast::Id;
use crate::pass::a_normalize::VarNamer;

pub mod ast;
pub mod pass;
pub mod parse;

fn main() {
    let program = r#"
    let a = K.read A in
    let b = K.read B in
    let a' = await a in
    let b' = await b in
    let a'' = K.read a' in
    let b'' = K.read b' in
    let a''' = await a'' in
    let b''' = await b'' in
    Int.add a''' b'''
    "#;
    let untyped = *parse::from_str(program).unwrap();

    struct VN { c: AtomicUsize }
    impl VN {
        fn new() -> Self { Self { c: AtomicUsize::new(0) } }
    }
    impl VarNamer for VN {
        fn name_var(&self) -> Id {
            let n = self.c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Id::new(format!("${}", n))
        }
    }
    let anf = pass::a_normalize::normalize(untyped, VN::new());
    let flat = pass::flatten::flatten(anf);

    println!("{:?}", flat);
}
