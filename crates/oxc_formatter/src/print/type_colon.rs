//! The type after a `:` that introduces it: a type annotation's, and a mapped type's value.

use oxc_ast::{
    Comment,
    ast::{TSMappedType, TSTypeAnnotation},
};
use oxc_formatter_core::Format;
use oxc_span::GetSpan;

use crate::{
    ast_nodes::{AstNode, AstNodes},
    format_args,
    formatter::{JsFormatter, prelude::*},
    write,
};

/// Whether the comments between a `:` and its type break the type off the `:` line,
/// the rule the type after `=` follows (`AssignmentLike`):
/// a line comment stays on the `:` line and an own-line comment stays own-line, and either one indents the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColonBreak {
    None,
    /// `: // c` + break + `T`
    AfterLineComment,
    /// `:` + break + `// c` + break + `T`
    BeforeOwnLineComment,
}

impl ColonBreak {
    /// Position-based, so the type's own printer (a union deciding its indent) sees what the `:` site saw.
    pub fn of(colon_end: u32, type_start: u32, f: &JsFormatter<'_, '_>) -> Self {
        let mut comments = f.comments().all_comments_in_range(colon_end, type_start).peekable();
        match comments.peek() {
            None => Self::None,
            Some(first) if first.preceded_by_newline() => Self::BeforeOwnLineComment,
            Some(_) => {
                if comments.any(Comment::is_line) {
                    Self::AfterLineComment
                } else {
                    Self::None
                }
            }
        }
    }

    /// The break after a type annotation's `:` (its span starts at the `:`),
    /// `None` for an annotation without one
    pub fn of_annotation(
        annotation: &AstNode<'_, '_, TSTypeAnnotation<'_>>,
        f: &JsFormatter<'_, '_>,
    ) -> Self {
        if !has_colon(annotation) {
            return Self::None;
        }
        let colon_end =
            f.context().comments().position_after_character(annotation.span.start, b':');
        Self::of(colon_end, annotation.type_annotation.span().start, f)
    }

    /// The break after a mapped type's value `:`, `None` without a value
    pub fn of_mapped_value(mapped: &TSMappedType<'_>, f: &JsFormatter<'_, '_>) -> Self {
        match (mapped_value_delimiters(mapped, f), &mapped.type_annotation) {
            (Some((_, colon_end)), Some(value)) => Self::of(colon_end, value.span().start, f),
            _ => Self::None,
        }
    }

    /// Writes the `:` and the type, whose leading-comments pass prints the comments in between
    /// (a suppression among them keeps the type as its target).
    pub fn write<'a>(
        self,
        type_: &impl Format<'a, JsFormatContext<'a>>,
        f: &mut JsFormatter<'_, 'a>,
    ) {
        let type_ = format_with(|f| type_.fmt(f));
        match self {
            Self::None => write!(f, [":", space(), type_]),
            Self::AfterLineComment => write!(f, [":", space(), indent(&type_)]),
            Self::BeforeOwnLineComment => {
                write!(f, [":", indent(&format_args!(hard_line_break(), type_))]);
            }
        }
    }
}

/// The positions after a mapped type's key `]` and after its value `:`, `None` without a value
pub fn mapped_value_delimiters(
    mapped: &TSMappedType<'_>,
    f: &JsFormatter<'_, '_>,
) -> Option<(u32, u32)> {
    mapped.type_annotation.as_ref()?;
    let key_end =
        mapped.name_type.as_ref().map_or(mapped.constraint.span().end, |name| name.span().end);
    let comments = f.context().comments();
    let close_bracket_end = comments.position_after_character(key_end, b']');
    Some((close_bracket_end, comments.position_after_character(close_bracket_end, b':')))
}

/// Whether a type annotation starts with a `:`: not after a function type's `=>`, nor in a type predicate
pub fn has_colon(annotation: &AstNode<'_, '_, TSTypeAnnotation<'_>>) -> bool {
    !matches!(
        annotation.parent(),
        AstNodes::TSFunctionType(_) | AstNodes::TSConstructorType(_) | AstNodes::TSTypePredicate(_)
    )
}
