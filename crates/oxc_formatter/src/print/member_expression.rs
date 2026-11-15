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
        member_operator::{operator_start, with_comments_before, write_operator_and_property},
        typecast::{cast_target_end, is_cast_target},
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
    // A cast target prints its parens and the comments inside them
    let object_end = cast_target_end(object.span(), f).unwrap_or(object.span().end);
    let operator = operator_start(object.span().end, optional, f);
    let is_member_chain = {
        let start = f.elements().len();
        with_comments_before(Some(operator), f, |f| write!(f, [object]));
        f.elements()[start..].has_label(LabelId::of(JsLabels::MemberChain))
    };
    // The member expression itself
    let member = property.parent();

    let property_start = property.span().start;
    match layout(
        &object,
        object_end,
        operator,
        property_start,
        member,
        is_member_chain,
        object_is_cast_target,
        f,
    ) {
        StaticMemberLayout::NoBreak => {
            let format_no_break =
                format_with(|f| write_operator_and_property(optional, &property, f));

            if is_member_chain {
                write!(f, [labelled(LabelId::of(JsLabels::MemberChain), &format_no_break)]);
            } else {
                write!(f, [format_no_break]);
            }
        }
        StaticMemberLayout::BreakBeforeOperator => {
            // The comments after the line comment stay before the operator too
            let comments = f.context().comments().comments_before(operator);
            write!(
                f,
                [
                    // The pending line comment breaks here, and no group before it fits on its line
                    line_suffix_boundary(),
                    FormatLeadingComments::Comments(comments)
                ]
            );
            write_operator_and_property(optional, &property, f);
        }
        StaticMemberLayout::BreakAfterObject => {
            write!(
                f,
                [group(&indent(&format_args!(
                    soft_line_break(),
                    &format_with(|f| {
                        if f.comments().has_leading_own_line_comment(operator) {
                            let comments = f.context().comments().comments_before(operator);
                            write!(
                                f,
                                [FormatLeadingComments::Comments(comments), soft_line_break()]
                            );
                        }
                    }),
                    format_with(|f| write_operator_and_property(optional, &property, f)),
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

    /// Breaks before the operator, after a line comment ending a line between the object and the property
    /// (`a // c` + break + `.b`)
    BreakBeforeOperator,
}

/// `object_is_cast_target`:
/// a cast-target object is neither a call nor an identifier to the rules below (see `is_cast_target`).
fn layout<'a>(
    object: &Expression<'a>,
    object_end: u32,
    operator: u32,
    property_start: u32,
    member: &AstNodes<'_, 'a>,
    is_member_chain: bool,
    object_is_cast_target: bool,
    f: &JsFormatter<'_, 'a>,
) -> StaticMemberLayout {
    if f.comments().has_leading_own_line_comment(operator) {
        return StaticMemberLayout::BreakAfterObject;
    }

    if has_line_comment_before_operator(object_end, operator, f) {
        return StaticMemberLayout::BreakBeforeOperator;
    }

    // The break follows the comment after the operator, not the object
    if f.comments()
        .all_comments_in_range(operator, property_start)
        .any(|comment| comment.is_line() || comment.preceded_by_newline())
    {
        return StaticMemberLayout::NoBreak;
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

/// Whether a line comment ends a line between the object (after any cast parens it prints) and the operator
/// (only the object's dropped parens sit there):
/// its `line_suffix` would otherwise flush past the operator and the property, which are user content.
fn has_line_comment_before_operator(
    object_end: u32,
    operator: u32,
    f: &JsFormatter<'_, '_>,
) -> bool {
    f.comments().all_comments_in_range(object_end, operator).any(Comment::is_line)
}
