//! Spans.

use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

use chumsky::{
    extra::ParserExtra,
    input::{Input, MapExtra},
    span::{SimpleSpan, Span as _},
};

use crate::source::SourceId;

pub type Span<C = SourceId> = SimpleSpan<usize, C>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spanned<T, C = SourceId> {
    pub inner: T,
    pub span: Span<C>,
}
impl<T, C> Spanned<T, C> {
    pub fn new(inner: T, span: Span<C>) -> Self {
        Self { inner, span }
    }
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U, C> {
        Spanned {
            inner: f(self.inner),
            span: self.span,
        }
    }
    pub fn from_extra<'s, 'b, I, E>(input: T, s: &mut MapExtra<'s, 'b, I, E>) -> Spanned<T, C>
    where
        I: Input<'s, Span = SimpleSpan<usize, C>>,
        E: ParserExtra<'s, I>,
    {
        Spanned::new(input, s.span())
    }
}
// XXX: Span::context(), ::start(), ::end() only exists for C: Clone
impl<T, C: Clone> Spanned<T, C> {
    pub fn map_both<U, K: Clone, F: FnOnce(T, Span<C>) -> (U, Span<K>)>(
        self,
        f: F,
    ) -> Spanned<U, K> {
        let Self { inner, span } = self;
        let (inner, span) = f(inner, span);
        Spanned { inner, span }
    }
    pub fn map_context<K: Clone, F: FnOnce(C) -> K>(self, f: F) -> Spanned<T, K> {
        let Self { inner, span } = self;
        Spanned {
            inner,
            span: Span::<K>::new(f(span.context()), span.start()..span.end()),
        }
    }
}
impl<T, C> AsRef<T> for Spanned<T, C> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}
impl<T, C> Deref for Spanned<T, C> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<T, C> DerefMut for Spanned<T, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
