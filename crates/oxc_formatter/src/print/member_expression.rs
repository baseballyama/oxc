use oxc_ast::ast::*;
use oxc_formatter_core::Format;
use oxc_span::GetSpan;

use crate::{
    JsLabels,
    ast_nodes::{AstNode, AstNodes},
    format_args,
    formatter::{JsFormatter, prelude::*, trivia::FormatLeadingComments},
    utils::{
        expression::as_call_expression_without_chain_wrappers,
        member_chain::chain_member::FormatComputedMemberExpressionWithoutObject,
        typecast::is_cast_target,
    },
    write,
};

use super::FormatWrite;

impl<'a> FormatWrite<'a> for AstNode<'_, 'a, ComputedMemberExpression<'a>> {
    fn write(&self, f: &mut JsFormatter<'_, 'a>) {
        write!(f, self.object());
        FormatComputedMemberExpressionWithoutObject(self).fmt(f);
    }
}

impl<'a> FormatWrite<'a> for AstNode<'_, 'a, StaticMemberExpression<'a>> {
    fn write(&self, f: &mut JsFormatter<'_, 'a>) {
        write_non_computed_member(self.object(), self.optional(), self.property(), f);
    }
}

/// `a.#b` is laid out the same as `a.b`.
impl<'a> FormatWrite<'a> for AstNode<'_, 'a, PrivateFieldExpression<'a>> {
    fn write(&self, f: &mut JsFormatter<'_, 'a>) {
        write_non_computed_member(self.object(), self.optional(), self.field(), f);
    }
}

/// Writes `a.b` or `a.#b`.
fn write_non_computed_member<'b, 'a, P: GetSpan>(
    object: AstNode<'b, 'a, Expression<'a>>,
    optional: bool,
    property: AstNode<'b, 'a, P>,
    f: &mut JsFormatter<'_, 'a>,
) where
    AstNode<'b, 'a, P>: Format<'a, JsFormatContext<'a>>,
{
    // Before the recording: printing the object consumes its cast comment
    let object_is_cast_target = is_cast_target(object.span(), f);
    let is_member_chain = {
        let start = f.elements().len();
        write!(f, [object]);
        f.elements()[start..].has_label(LabelId::of(JsLabels::MemberChain))
    };
    let property_start = property.span().start;
    // The member expression itself
    let member = property.parent();

    match layout(&object, property_start, member, is_member_chain, object_is_cast_target, f) {
        StaticMemberLayout::NoBreak => {
            let format_no_break = format_with(|f| write!(f, [operator_token(optional), property]));

            if is_member_chain {
                write!(f, [labelled(LabelId::of(JsLabels::MemberChain), &format_no_break)]);
            } else {
                write!(f, [format_no_break]);
            }
        }
        StaticMemberLayout::BreakBeforeOperator(operator) => {
            // The comments after the line comment stay before the operator too
            let comments = f.context().comments().comments_before(operator);
            write!(
                f,
                [
                    hard_line_break(),
                    FormatLeadingComments::Comments(comments),
                    operator_token(optional),
                    property
                ]
            );
        }
        StaticMemberLayout::BreakAfterObject => {
            write!(
                f,
                [group(&indent(&format_args!(
                    soft_line_break(),
                    &format_with(|f| {
                        if f.comments().has_leading_own_line_comment(property_start) {
                            let comments = f.context().comments().comments_before(property_start);
                            write!(
                                f,
                                [FormatLeadingComments::Comments(comments), soft_line_break()]
                            );
                        }
                    }),
                    operator_token(optional),
                    property,
                )))]
            );
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum StaticMemberLayout {
    /// Forces that there's no line break between the object, operator, and member
    NoBreak,

    /// Breaks the static member expression after the object if the whole expression doesn't fit on a single line
    BreakAfterObject,

    /// Breaks before the operator at this offset, after a line comment ending the object's line
    /// (`a // c` + break + `.b`)
    BreakBeforeOperator(u32),
}

fn operator_token(optional: bool) -> &'static str {
    if optional { "?." } else { "." }
}

/// `object_is_cast_target`:
/// a cast-target object is neither a call nor an identifier to the rules below (see `is_cast_target`).
fn layout<'a>(
    object: &Expression<'a>,
    property_start: u32,
    member: &AstNodes<'_, 'a>,
    is_member_chain: bool,
    object_is_cast_target: bool,
    f: &JsFormatter<'_, 'a>,
) -> StaticMemberLayout {
    if f.comments().has_leading_own_line_comment(property_start) {
        return StaticMemberLayout::BreakAfterObject;
    }

    if let Some(operator) = operator_after_line_comment(object.span().end, property_start, f) {
        return StaticMemberLayout::BreakBeforeOperator(operator);
    }

    // `a.b.c!` and `a.b?.c`
    // `TSNonNullExpression` is a wrapper node for `!`, and `ChainExpression` is a wrapper node for `?.`,
    // so we need to skip them to find the real parent node.
    let mut parent = member.parent();
    while matches!(parent, AstNodes::TSNonNullExpression(_) | AstNodes::ChainExpression(_)) {
        parent = parent.parent();
    }

    let is_nested = match parent {
        AstNodes::AssignmentExpression(_) | AstNodes::VariableDeclarator(_) => {
            let no_break = !object_is_cast_target
                && as_call_expression_without_chain_wrappers(object)
                    .is_some_and(|call| !call.arguments.is_empty());

            if no_break || is_member_chain {
                return StaticMemberLayout::NoBreak;
            }

            false
        }
        _ => parent.is_member_expression(),
    };

    // A single-level member is not split when its object is a single word (an identifier, `this` or `super`):
    // the break would leave the word alone on its line for a single property.
    // A longer path (`a.b.c`) can break into a list of properties instead.
    // NOTE: `import.meta` and `new.target` cannot break inside either, so the same reasoning applies to them.
    // They are not included yet, and break like Prettier (`import.meta` + break + `.resolve(...)`).
    if !is_nested
        && !object_is_cast_target
        && matches!(
            object,
            Expression::Identifier(_) | Expression::ThisExpression(_) | Expression::Super(_)
        )
    {
        return StaticMemberLayout::NoBreak;
    }

    let mut first_non_static_member_ancestor = parent;
    while first_non_static_member_ancestor.is_member_expression()
        || matches!(
            first_non_static_member_ancestor,
            AstNodes::ChainExpression(_) | AstNodes::TSNonNullExpression(_)
        )
    {
        first_non_static_member_ancestor = first_non_static_member_ancestor.parent();
    }

    match first_non_static_member_ancestor {
        AstNodes::NewExpression(expr) if expr.callee.span().contains_inclusive(member.span()) => {
            StaticMemberLayout::NoBreak
        }
        AstNodes::AssignmentExpression(assignment) => {
            if matches!(assignment.left, AssignmentTarget::AssignmentTargetIdentifier(_)) {
                StaticMemberLayout::BreakAfterObject
            } else {
                StaticMemberLayout::NoBreak
            }
        }
        _ => StaticMemberLayout::BreakAfterObject,
    }
}

/// The offset of the member's `.` / `?.` when a line comment ends the line right before it (only blanks and comments follow):
/// its `line_suffix` would otherwise flush past the property, which is user content.
/// A comment before a `)` is not one: the parens hold it.
fn operator_after_line_comment(
    object_end: u32,
    property_start: u32,
    f: &JsFormatter<'_, '_>,
) -> Option<u32> {
    let source = f.source_text();
    let mut gap_start = object_end;
    let mut after_line_comment = false;
    // The operator is the gap's only `.` outside its comments (a `)` may come before it)
    let gaps = f
        .comments()
        .all_comments_in_range(object_end, property_start)
        .map(|comment| (comment.span.start, Some(comment)))
        .chain([(property_start, None)]);
    for (gap_end, comment) in gaps {
        let gap = source.bytes_range(gap_start, gap_end);
        let first = (gap_start..).zip(gap).find(|(_, byte)| !byte.is_ascii_whitespace());
        if let Some((first, _)) = first {
            let rest = source.bytes_range(first, gap_end);
            if rest.starts_with(b".") || rest.starts_with(b"?.") {
                return after_line_comment.then_some(first);
            }
            if rest.contains(&b'.') {
                return None;
            }
            after_line_comment = false;
        }
        let comment = comment?;
        after_line_comment |= comment.is_line();
        gap_start = comment.span.end;
    }
    None
}
