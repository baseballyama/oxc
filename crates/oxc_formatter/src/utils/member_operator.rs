//! The operator of a non-computed member (`.`, `?.`) and the comments on its two sides.
//! A comment keeps its source side of the operator: the object's passes stop before it,
//! and the comments after it print after it.

use oxc_ast::Comment;
use oxc_formatter_core::Format;
use oxc_span::{GetSpan, Span};

use crate::{
    ast_nodes::AstNodes,
    format_args,
    formatter::{
        JsFormatter,
        prelude::*,
        trivia::{FormatLeadingComments, FormatTrailingComments},
    },
    write,
};

/// The start of the `.` / `?.` after a member's object (only its parens and comments sit between)
pub fn operator_start(object_end: u32, optional: bool, f: &JsFormatter<'_, '_>) -> u32 {
    let dot_end = f.context().comments().position_after_character(object_end, b'.');
    dot_end - 1 - u32::from(optional)
}

/// The operator start of the non-computed member whose object is the node at `span`, `None` for any other parent
pub fn operator_after_object(
    span: Span,
    parent: &AstNodes<'_, '_>,
    f: &JsFormatter<'_, '_>,
) -> Option<u32> {
    let (object, optional) = match parent {
        AstNodes::StaticMemberExpression(member) => (member.object.span(), member.optional),
        AstNodes::PrivateFieldExpression(member) => (member.object.span(), member.optional),
        _ => return None,
    };
    (object == span).then(|| operator_start(span.end, optional, f))
}

/// Runs `write` with the comment view ending at `end`, when there is one
pub fn with_comments_before<'a>(
    end: Option<u32>,
    f: &mut JsFormatter<'_, 'a>,
    write: impl FnOnce(&mut JsFormatter<'_, 'a>),
) {
    let view_limit = end.map(|end| f.context_mut().comments_mut().limit_comments_up_to(end));
    write(f);
    if let Some(view_limit) = view_limit {
        f.context_mut().comments_mut().restore_view_limit(view_limit);
    }
}

/// Writes the operator, the comments after it, and the property:
/// a same-line run stays on the operator's line, an own-line comment stays own-line,
/// and a line comment or an own-line comment breaks and indents the property, as after `=`
pub fn write_operator_and_property<'a>(
    optional: bool,
    property: &(impl Format<'a, JsFormatContext<'a>> + GetSpan),
    f: &mut JsFormatter<'_, 'a>,
) {
    write!(f, [if optional { "?." } else { "." }]);
    let comments = f.context().comments().comments_before(property.span().start);
    let own_line_start =
        comments.iter().position(Comment::preceded_by_newline).unwrap_or(comments.len());
    let (same_line, own_line) = comments.split_at(own_line_start);
    if same_line.iter().any(Comment::is_line) || !own_line.is_empty() {
        write!(
            f,
            [
                FormatTrailingComments::Comments(same_line),
                indent(&format_args!(
                    hard_line_break(),
                    FormatLeadingComments::Comments(own_line),
                    property
                ))
            ]
        );
    } else {
        // Blocks lead the property, glued to the operator (`a./* c */ b`, Prettier too)
        write!(f, [FormatLeadingComments::Comments(same_line), property]);
    }
}
