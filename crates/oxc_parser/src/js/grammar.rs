//! Cover Grammar for Destructuring Assignment

use oxc_allocator::ArenaVec;
use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};

use crate::{ParserConfig as Config, ParserImpl, diagnostics};

pub trait CoverGrammar<'a, T, C: Config>: Sized {
    fn cover(value: T, p: &mut ParserImpl<'a, C>) -> Self;
}

/// What assigns a target, which names the error for an expression that cannot be one. The error
/// labels the outermost parentheses around the target, which a tree without them passes as a span.
#[derive(Clone, Copy)]
pub enum AssignedBy {
    Assignment,
    Update,
    ForIn,
    ForOf,
}

impl AssignedBy {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Assignment => "2364",
            Self::Update => "2357",
            Self::ForIn => "2406",
            Self::ForOf => "2487",
        }
    }
}

impl<'a, C: Config> CoverGrammar<'a, Expression<'a>, C> for AssignmentTarget<'a> {
    fn cover(expr: Expression<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        Self::cover((expr, AssignedBy::Assignment, None), p)
    }
}

type Assigned<'a> = (Expression<'a>, AssignedBy, Option<Span>);

impl<'a, C: Config> CoverGrammar<'a, Assigned<'a>, C> for AssignmentTarget<'a> {
    fn cover((expr, by, parentheses): Assigned<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        match expr {
            Expression::ArrayExpression(array_expr) => {
                let pat = ArrayAssignmentTarget::cover(array_expr.unbox(), p);
                AssignmentTarget::ArrayAssignmentTarget(p.alloc(pat))
            }
            Expression::ObjectExpression(object_expr) => {
                let pat = ObjectAssignmentTarget::cover(object_expr.unbox(), p);
                AssignmentTarget::ObjectAssignmentTarget(p.alloc(pat))
            }
            _ => AssignmentTarget::from(SimpleAssignmentTarget::cover((expr, by, parentheses), p)),
        }
    }
}

impl<'a, C: Config> CoverGrammar<'a, Expression<'a>, C> for SimpleAssignmentTarget<'a> {
    fn cover(expr: Expression<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        Self::cover((expr, AssignedBy::Assignment, None), p)
    }
}

impl<'a, C: Config> CoverGrammar<'a, Assigned<'a>, C> for SimpleAssignmentTarget<'a> {
    fn cover((expr, by, parentheses): Assigned<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        let code = by.code();
        match expr {
            Expression::Identifier(ident) => {
                SimpleAssignmentTarget::AssignmentTargetIdentifier(ident)
            }
            match_member_expression!(Expression) => {
                let member_expr = expr.into_member_expression();
                SimpleAssignmentTarget::from(member_expr)
            }
            Expression::ParenthesizedExpression(expr) => {
                let span = parentheses.unwrap_or(expr.span);
                match expr.unbox().expression {
                    Expression::ObjectExpression(_) | Expression::ArrayExpression(_) => {
                        p.fatal_error(diagnostics::invalid_assignment(code, span))
                    }
                    expr => SimpleAssignmentTarget::cover((expr, by, Some(span)), p),
                }
            }
            Expression::TSAsExpression(expr) => match expr.expression.get_inner_expression() {
                Expression::Identifier(_)
                | Expression::StaticMemberExpression(_)
                | Expression::ComputedMemberExpression(_)
                | Expression::PrivateFieldExpression(_) => {
                    SimpleAssignmentTarget::TSAsExpression(expr)
                }
                _ => p.fatal_error(diagnostics::invalid_assignment(
                    code,
                    parentheses.unwrap_or(expr.span()),
                )),
            },
            Expression::TSSatisfiesExpression(expr) => {
                match expr.expression.get_inner_expression() {
                    Expression::Identifier(_)
                    | Expression::StaticMemberExpression(_)
                    | Expression::ComputedMemberExpression(_)
                    | Expression::PrivateFieldExpression(_) => {
                        SimpleAssignmentTarget::TSSatisfiesExpression(expr)
                    }
                    _ => p.fatal_error(diagnostics::invalid_assignment(
                        code,
                        parentheses.unwrap_or(expr.span()),
                    )),
                }
            }
            Expression::TSNonNullExpression(expr) => match expr.expression.get_inner_expression() {
                Expression::Identifier(_)
                | Expression::StaticMemberExpression(_)
                | Expression::ComputedMemberExpression(_)
                | Expression::PrivateFieldExpression(_) => {
                    SimpleAssignmentTarget::TSNonNullExpression(expr)
                }
                _ => p.fatal_error(diagnostics::invalid_assignment(
                    code,
                    parentheses.unwrap_or(expr.span()),
                )),
            },
            Expression::TSTypeAssertion(expr) => match expr.expression.get_inner_expression() {
                Expression::Identifier(_)
                | Expression::StaticMemberExpression(_)
                | Expression::ComputedMemberExpression(_)
                | Expression::PrivateFieldExpression(_) => {
                    SimpleAssignmentTarget::TSTypeAssertion(expr)
                }
                _ => p.fatal_error(diagnostics::invalid_assignment(
                    code,
                    parentheses.unwrap_or(expr.span()),
                )),
            },
            Expression::TSInstantiationExpression(expr) => {
                p.fatal_error(diagnostics::invalid_lhs_assignment(expr.span()))
            }
            expr => p.fatal_error(diagnostics::invalid_assignment(
                code,
                parentheses.unwrap_or(expr.span()),
            )),
        }
    }
}

impl<'a, C: Config> CoverGrammar<'a, ArrayExpression<'a>, C> for ArrayAssignmentTarget<'a> {
    // Destructuring-target conversion is comparatively rare and large. Keeping it out of line
    // stops it being inlined into `AssignmentTarget::cover`, whose common arm (simple targets)
    // would otherwise carry this body's large stack frame + callee-saved spills on every call.
    #[inline(never)]
    fn cover(expr: ArrayExpression<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        let len = expr.elements.len();
        // A final spread becomes `rest`, so it needs no slot in `elements`.
        let capacity = len
            - usize::from(matches!(
                expr.elements.last(),
                Some(ArrayExpressionElement::SpreadElement(_))
            ));
        let mut elements = ArenaVec::with_capacity_in(capacity, p);
        let mut rest = None;

        for (i, elem) in expr.elements.into_iter().enumerate() {
            match elem {
                match_expression!(ArrayExpressionElement) => {
                    let expr = elem.into_expression();
                    let target = AssignmentTargetMaybeDefault::cover(expr, p);
                    elements.push(Some(target));
                }
                ArrayExpressionElement::SpreadElement(elem) => {
                    if i == len - 1 {
                        let span = elem.span;
                        let argument = elem.unbox().argument;
                        if !matches!(
                            argument.get_inner_expression(),
                            Expression::Identifier(_)
                                | Expression::ArrayExpression(_)
                                | Expression::ObjectExpression(_)
                                | Expression::StaticMemberExpression(_)
                                | Expression::ComputedMemberExpression(_)
                                | Expression::PrivateFieldExpression(_)
                        ) {
                            p.error(diagnostics::invalid_rest_assignment_target(
                                "2364",
                                argument.span(),
                            ));
                        }
                        let target = AssignmentTarget::cover(argument, p);
                        rest = Some(AssignmentTargetRest::boxed(span, target, p));
                        if let Some(span) = p.state.trailing_commas.remove(&expr.span.start) {
                            p.error(diagnostics::rest_element_trailing_comma(span));
                        }
                    } else {
                        let error = diagnostics::spread_last_element(elem.span);
                        return p.fatal_error(error);
                    }
                }
                ArrayExpressionElement::Elision(_) => elements.push(None),
            }
        }

        ArrayAssignmentTarget::new(expr.span, elements, rest, p)
    }
}

impl<'a, C: Config> CoverGrammar<'a, Expression<'a>, C> for AssignmentTargetMaybeDefault<'a> {
    fn cover(expr: Expression<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        match expr {
            Expression::AssignmentExpression(assignment_expr) => {
                if assignment_expr.operator != AssignmentOperator::Assign {
                    p.error(diagnostics::invalid_assignment_target_default_value_operator(
                        assignment_expr.span,
                    ));
                }
                let target = AssignmentTargetWithDefault::cover(assignment_expr.unbox(), p);
                AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(p.alloc(target))
            }
            expr => {
                let target = AssignmentTarget::cover(expr, p);
                AssignmentTargetMaybeDefault::from(target)
            }
        }
    }
}

impl<'a, C: Config> CoverGrammar<'a, AssignmentExpression<'a>, C>
    for AssignmentTargetWithDefault<'a>
{
    fn cover(expr: AssignmentExpression<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        AssignmentTargetWithDefault::new(expr.span, expr.left, expr.right, p)
    }
}

impl<'a, C: Config> CoverGrammar<'a, ObjectExpression<'a>, C> for ObjectAssignmentTarget<'a> {
    // Kept out of line for the same reason as `ArrayAssignmentTarget::cover` above: avoid
    // inlining this large body into the hot `AssignmentTarget::cover` dispatcher.
    #[inline(never)]
    fn cover(expr: ObjectExpression<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        let len = expr.properties.len();
        // A final spread becomes `rest`, so it needs no slot in `properties`.
        let capacity = len
            - usize::from(matches!(
                expr.properties.last(),
                Some(ObjectPropertyKind::SpreadProperty(_))
            ));
        let mut properties = ArenaVec::with_capacity_in(capacity, p);
        let mut rest = None;

        for (i, elem) in expr.properties.into_iter().enumerate() {
            match elem {
                ObjectPropertyKind::ObjectProperty(property) => {
                    let target = AssignmentTargetProperty::cover(property.unbox(), p);
                    properties.push(target);
                }
                ObjectPropertyKind::SpreadProperty(spread) => {
                    if i == len - 1 {
                        let span = spread.span;
                        let argument = spread.unbox().argument;
                        if !matches!(
                            argument.get_inner_expression(),
                            Expression::Identifier(_)
                                | Expression::StaticMemberExpression(_)
                                | Expression::ComputedMemberExpression(_)
                                | Expression::PrivateFieldExpression(_)
                        ) {
                            p.error(diagnostics::invalid_rest_assignment_target(
                                "2701",
                                argument.span(),
                            ));
                        }
                        if let Some(span) = p.state.trailing_commas.remove(&expr.span.start) {
                            p.error(diagnostics::rest_element_trailing_comma(span));
                        }
                        let target = AssignmentTarget::cover(argument, p);
                        rest = Some(AssignmentTargetRest::boxed(span, target, p));
                    } else {
                        return p.fatal_error(diagnostics::spread_last_element(spread.span));
                    }
                }
            }
        }

        ObjectAssignmentTarget::new(expr.span, properties, rest, p)
    }
}

impl<'a, C: Config> CoverGrammar<'a, ObjectProperty<'a>, C> for AssignmentTargetProperty<'a> {
    fn cover(property: ObjectProperty<'a>, p: &mut ParserImpl<'a, C>) -> Self {
        if property.shorthand {
            let binding = match property.value {
                Expression::Identifier(ident) => ident.unbox(),
                _ => return p.unexpected(),
            };
            // convert `CoverInitializedName`
            let init = p
                .state
                .cover_initialized_name
                .remove(&property.span.start)
                .map(|cover| cover.initializer);
            AssignmentTargetProperty::new_assignment_target_property_identifier(
                property.span,
                binding,
                init,
                p,
            )
        } else {
            let binding = AssignmentTargetMaybeDefault::cover(property.value, p);
            AssignmentTargetProperty::new_assignment_target_property_property(
                property.span,
                property.key,
                binding,
                property.computed,
                p,
            )
        }
    }
}
