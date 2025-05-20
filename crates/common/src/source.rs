//! Source management. The core type is [`Sources`].

use std::{
    collections::{BTreeMap, HashMap},
    ops::Bound,
    path::{Path, PathBuf},
};

use chumsky::span::Span as _;
use miette::{
    Diagnostic, MietteError, MietteSpanContents, SourceCode, SourceOffset, SourceSpan, SpanContents,
};
use thiserror::Error;

use crate::span::Span;

/// Language name, for use via `miette` for syntax highlighting in snippets.
pub const HIGHLIGHT_LANGUAGE: &str = "soir";

/// Errors that can occur when creating a [`Source`].
#[derive(Debug, Error, Diagnostic)]
pub enum SourceError {
    #[error("path ''{0}'' is not valid UTF-8")]
    NonUtf8Path(PathBuf),
    #[error("path does not exist")]
    DoesNotExist,
    #[error("path refers to a directory, not a file")]
    IsDirectory,
    #[error("path is not a regular file")]
    AbnormalFile,
    #[error("could not read file: {0}")]
    Io(#[source] std::io::Error),
}

/// Represents a single SOIR source (UTF-8 string) and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    contents: String,
    name: String,
}
impl Source {
    /// Create a `Source` from the file at `path`, if `path` is a file and can be read.
    /// The name of this source will be taken to be `path`.
    ///
    /// # Errors
    ///
    /// If the path is not valid UTF-8, does not exist, is not a regular file, or cannot be read,
    /// an error is returned.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, SourceError> {
        let path = path.as_ref();
        let Some(path_str) = path.to_str() else {
            return Err(SourceError::NonUtf8Path(path.to_path_buf()));
        };
        Self::from_path_with_name(path, path_str)
    }
    /// Create a `Source` from the file at `path`, if `path` is a file and can be read.
    ///
    /// # Errors
    ///
    /// If the path does not exist, is not a regular file, or cannot be read, an error is returned.
    pub fn from_path_with_name<P: AsRef<Path>, S: Into<String>>(
        path: P,
        name: S,
    ) -> Result<Self, SourceError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(SourceError::DoesNotExist);
        } else if path.is_dir() {
            return Err(SourceError::IsDirectory);
        } else if !path.is_file() {
            return Err(SourceError::AbnormalFile);
        }
        let contents = std::fs::read_to_string(path).map_err(SourceError::Io)?;
        let name = name.into();
        Ok(Self { contents, name })
    }
    /// Create a `Source` from a [`String`] and a name.
    pub fn new<C: Into<String>, S: Into<String>>(contents: C, name: S) -> Self {
        Self {
            contents: contents.into(),
            name: name.into(),
        }
    }
    /// Contents of the source.
    pub fn contents(&self) -> &str {
        &self.contents
    }
    /// Name of the source.
    pub fn name(&self) -> &str {
        &self.name
    }
}
impl SourceCode for Source {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        // Based on the implementation of miette::NamedSource
        let inner_contents =
            self.contents()
                .read_span(span, context_lines_before, context_lines_after)?;
        let contents = MietteSpanContents::new_named(
            self.name.clone(),
            inner_contents.data(),
            *inner_contents.span(),
            inner_contents.line(),
            inner_contents.column(),
            inner_contents.line_count(),
        )
        .with_language(HIGHLIGHT_LANGUAGE);
        Ok(Box::new(contents))
    }
}

/// Cheap, per-[`Sources`]-unique identifier for a [`Source`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(usize);

/// Group of [`Source`]s.
///
/// Exists to overcome the single-[`SourceCode`] limitation of `miette`.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    map: HashMap<SourceId, Source>,
    span_sources: BTreeMap<usize, SourceId>,
    source_spans: HashMap<SourceId, usize>,
}
impl Sources {
    /// Create new `Sources`. To populate it, see [`insert`].
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            span_sources: BTreeMap::new(),
            source_spans: HashMap::new(),
        }
    }
    /// Add a [`Source`] to this `Sources`. Will fail and return [`None`] if adding `source` would
    /// result in a combined length that overflows [`usize`], otherwise will return the [`SourceId`]
    /// assigned to `source`. Will also fail and return [`None`] if a source with the same name
    /// already exists.
    pub fn insert(&mut self, source: Source) -> Option<SourceId> {
        let curr_len = self.len();
        if curr_len.checked_add(source.contents().len()).is_none() {
            None
        } else if self.map.values().any(|v| v.name() == source.name()) {
            None
        } else {
            let id = SourceId(self.map.len());
            self.map.insert(id, source);
            self.span_sources.insert(curr_len, id);
            self.source_spans.insert(id, curr_len);
            Some(id)
        }
    }
    /// Get combined length (in bytes) of all contained spans.
    pub fn len(&self) -> usize {
        self.span_sources
            .upper_bound(Bound::Unbounded)
            .peek_prev()
            .map_or(0, |(span_start, source_id)| {
                // `source_id` lookup is infallible here since we're sure that `source_id` was
                // generated internally.
                #[allow(clippy::missing_panics_doc)]
                let span_len = self.map.get(source_id).unwrap().contents().len();
                // we know from `insert` that the addition will not overflow
                span_start + span_len
            })
    }
    /// Returns true if no sources have been inserted.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    /// Get a [`Source`] given its [`SourceId`].
    ///
    /// # Panics
    ///
    /// May panic if `source_id` was generated by a different `Sources`.
    pub fn get(&self, source_id: SourceId) -> &Source {
        self.map
            .get(&source_id)
            .expect("source_id is from different Sources object")
    }
    /// Given a [`Span`], calculate its virtual [`SourceSpan`] for use in diagnostics.
    ///
    /// # Panics
    ///
    /// May panic if `source_id` was generated by a different `Sources`.
    ///
    /// Will panic if `range` is out of range for `source_id`.
    pub fn translate_span(&self, span: Span<SourceId>) -> SourceSpan {
        let source_id = span.context();
        let source_len = self.get(source_id).contents().len();
        assert!(
            // note: span.end is exclusive, so we use <=
            span.end <= source_len,
            "translate_span failed: span {span} out of range for source with length {source_len}"
        );
        let virtual_source_start = self.source_spans.get(&source_id).unwrap();
        let virtual_span_start = virtual_source_start + span.start;
        SourceSpan::new(
            SourceOffset::from(virtual_span_start),
            span.end - span.start,
        )
    }
    /// An iterator visiting the [`SourceId`]s of the [`Source`]s in this `Sources` in an arbitrary
    /// order.
    pub fn ids(&self) -> impl IntoIterator<Item = SourceId> {
        self.map.keys().copied()
    }
}
impl SourceCode for Sources {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<Box<dyn SpanContents<'a> + 'a>, MietteError> {
        // gap after the greatest key smaller than or equal to span.offset()
        let cursor = self
            .span_sources
            .upper_bound(Bound::Included(&span.offset()));
        let Some((source_start, source_id)) = cursor.peek_prev() else {
            return Err(MietteError::OutOfBounds);
        };
        // note: upper_bound() guarantees that source_start <= span.offset(), so this will not wrap
        let offset_in_source = span.offset() - source_start;
        // In this case, SourceId is used internally, so it's infallible; no panic will result
        let source = self.map.get(source_id).unwrap();

        let new_source_span = SourceSpan::new(SourceOffset::from(offset_in_source), span.len());
        source.read_span(&new_source_span, context_lines_before, context_lines_after)
    }
}
