use std::cell::Cell;

use oxc_allocator::Allocator;
use oxc_ast::builder::AstBuilder;
use oxc_ast::ast::{
    BindingIdentifier, BindingPattern, Expression, FormalParameters, IdentifierReference, Program,
};
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::{
    scope::{ScopeFlags, ScopeId},
    symbol::SymbolFlags,
};

use super::{Host, HostVisitor};
use crate::{SemanticBuilder, Stats};

/// A component: a `<script>`, and `{#each items as item}{item + total}{/each}` after it.
struct Each<'a> {
    items: Expression<'a>,
    item: FormalParameters<'a>,
    body: Expression<'a>,
    scope_id: Cell<Option<ScopeId>>,
}

impl<'a> Host<'a> for Each<'a> {
    fn visit(&'a self, visitor: &mut HostVisitor<'_, 'a>) {
        visitor.visit_expression(&self.items);
        visitor.enter_scope(ScopeFlags::empty(), &self.scope_id);
        visitor.visit_formal_parameter(&self.item.items[0]);
        visitor.visit_expression(&self.body);
        visitor.leave_scope();
    }
}

fn program<'a>(allocator: &'a Allocator, source_text: &'a str) -> &'a Program<'a> {
    allocator.alloc(Parser::new(allocator, source_text, SourceType::mjs()).parse().program)
}

fn expression<'a>(allocator: &'a Allocator, source_text: &'a str) -> Expression<'a> {
    Parser::new(allocator, source_text, SourceType::mjs()).parse_expression().unwrap()
}

fn parameters<'a>(allocator: &'a Allocator, source_text: &'a str) -> FormalParameters<'a> {
    let Expression::ArrowFunctionExpression(arrow) = expression(allocator, source_text) else {
        unreachable!()
    };
    arrow.unbox().params.unbox()
}

fn identifiers<'b, 'a>(expression: &'b Expression<'a>) -> Vec<&'b IdentifierReference<'a>> {
    match expression {
        Expression::Identifier(identifier) => vec![identifier],
        Expression::BinaryExpression(binary) => {
            let mut all = identifiers(&binary.left);
            all.extend(identifiers(&binary.right));
            all
        }
        _ => unreachable!(),
    }
}

#[test]
fn host_references_resolve_to_the_program_and_to_host_scopes() {
    let allocator = Allocator::default();
    let program = program(&allocator, "let items = [], total = 0;");
    let each: &Each = allocator.alloc(Each {
        items: expression(&allocator, "items"),
        item: parameters(&allocator, "(item) => 0"),
        body: expression(&allocator, "item + total + missing"),
        scope_id: Cell::new(None),
    });
    let semantic = SemanticBuilder::new().build_with_host(program, each).semantic;
    let scoping = semantic.scoping();

    let items = scoping.get_binding(scoping.root_scope_id(), "items".into()).unwrap();
    let total = scoping.get_binding(scoping.root_scope_id(), "total".into()).unwrap();
    let each_scope = each.scope_id.get().unwrap();
    assert_eq!(scoping.scope_parent_id(each_scope), Some(scoping.root_scope_id()));
    let BindingPattern::BindingIdentifier(item_binding) = &each.item.items[0].pattern else {
        unreachable!()
    };
    let item = item_binding.symbol_id();
    assert_eq!(scoping.symbol_scope_id(item), each_scope);

    let Expression::Identifier(items_reference) = &each.items else { unreachable!() };
    let resolved = |identifier: &IdentifierReference| {
        scoping.get_reference(identifier.reference_id()).symbol_id()
    };
    assert_eq!(resolved(items_reference), Some(items));
    let body = identifiers(&each.body);
    assert_eq!(resolved(body[0]), Some(item));
    assert_eq!(resolved(body[1]), Some(total));
    assert_eq!(resolved(body[2]), None);
    assert!(scoping.root_unresolved_references().contains_key("missing"));
}

/// `<script module>` as the program, and the instance `<script>` and a snippet inside it.
struct Component<'a> {
    instance: &'a Program<'a>,
    instance_scope_id: Cell<Option<ScopeId>>,
    use_before: Expression<'a>,
    snippet: &'a BindingIdentifier<'a>,
}

impl<'a> Host<'a> for Component<'a> {
    fn visit(&'a self, visitor: &mut HostVisitor<'_, 'a>) {
        visitor.enter_scope(ScopeFlags::Function, &self.instance_scope_id);
        visitor.visit_statements(&self.instance.body);
        visitor.visit_expression(&self.use_before);
        visitor.declare(self.snippet, SymbolFlags::BlockScopedVariable);
        visitor.leave_scope();
    }
}

#[test]
fn a_second_program_and_a_hoisted_name_in_a_host_scope() {
    let allocator = Allocator::default();
    let module = program(&allocator, "const shared = 1;");
    let instance = program(&allocator, "var local = shared;");
    let Expression::Identifier(snippet) = expression(&allocator, "row") else { unreachable!() };
    let ast = AstBuilder::new(&allocator);
    let snippet = allocator.alloc(BindingIdentifier::new(snippet.span, snippet.name, &ast));
    let component: &Component = allocator.alloc(Component {
        instance,
        instance_scope_id: Cell::new(None),
        use_before: expression(&allocator, "row"),
        snippet,
    });
    let host: &dyn Host = component;
    let stats = Stats::count_with_host(module, host);
    let semantic = SemanticBuilder::new().build_with_host(module, host).semantic;
    let scoping = semantic.scoping();
    assert_eq!(stats.scopes, 2);

    let instance_scope = component.instance_scope_id.get().unwrap();
    let local = scoping.get_binding(instance_scope, "local".into()).unwrap();
    assert!(scoping.get_binding(scoping.root_scope_id(), "local".into()).is_none());
    let shared = scoping.get_binding(scoping.root_scope_id(), "shared".into()).unwrap();
    assert_eq!(scoping.get_resolved_reference_ids(shared).len(), 1);
    assert_eq!(scoping.symbol_scope_id(local), instance_scope);

    let Expression::Identifier(row) = &component.use_before else { unreachable!() };
    let reference = scoping.get_reference(row.reference_id());
    assert_eq!(reference.symbol_id(), Some(snippet.symbol_id()));
}
