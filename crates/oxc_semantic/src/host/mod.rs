//! Code that a host language evaluates inside a program.
//!
//! A Svelte or Vue component evaluates its template's expressions in the scope of its `<script>`,
//! and its template syntax opens scopes of its own (`{#each items as item}`, `v-for`). A [`Host`]
//! visits those nodes with a [`HostVisitor`], so that one [`Scoping`](crate::Scoping) holds the
//! script's and the template's symbols and references.

use std::cell::Cell;

use oxc_allocator::ArenaVec;
use oxc_ast::{
    AstKind,
    ast::{
        BindingIdentifier, Expression, FormalParameter, FormalParameters, Statement,
        VariableDeclaration,
    },
};
use oxc_ast_visit::Visit;
use oxc_syntax::{
    scope::{ScopeFlags, ScopeId},
    symbol::SymbolFlags,
};

use crate::{SemanticBuilder, stats::Counter};

/// What a host language evaluates inside a program.
///
/// [`SemanticBuilder::build_with_host`] calls [`Host::visit`] after the program's body, in the
/// program's scope, and resolves the host's references with the program's. Since references are
/// resolved only after both are visited, a name the host declares is visible in its whole scope,
/// before and after the declaration.
pub trait Host<'a> {
    /// Visits the host's nodes, in the order of the source text.
    fn visit(&self, visitor: &mut HostVisitor<'_, 'a>);
}

/// Visits the nodes of a [`Host`].
///
/// Every node must live in the program's arena, so its methods take `&'a` references.
pub struct HostVisitor<'v, 'a> {
    inner: Inner<'v, 'a>,
}

enum Inner<'v, 'a> {
    Builder(&'v mut SemanticBuilder<'a>),
    Counter(&'v mut Counter),
}

impl<'v, 'a> HostVisitor<'v, 'a> {
    pub(crate) fn builder(builder: &'v mut SemanticBuilder<'a>) -> Self {
        Self { inner: Inner::Builder(builder) }
    }

    pub(crate) fn counter(counter: &'v mut Counter) -> Self {
        Self { inner: Inner::Counter(counter) }
    }

    /// Opens a scope the host's syntax creates, such as the body of `{#each}`, and stores its id
    /// in `scope_id`.
    ///
    /// Its node is the program, because the syntax that opens it is not a JavaScript node.
    pub fn enter_scope(&mut self, flags: ScopeFlags, scope_id: &Cell<Option<ScopeId>>) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.enter_scope(flags, scope_id),
            Inner::Counter(counter) => counter.enter_scope(flags, scope_id),
        }
    }

    /// Closes the scope [`HostVisitor::enter_scope`] opened last.
    pub fn leave_scope(&mut self) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.leave_scope(),
            Inner::Counter(counter) => counter.leave_scope(),
        }
    }

    /// Declares `identifier` in the current scope, such as the name of `{#snippet name()}`.
    pub fn declare(&mut self, identifier: &'a BindingIdentifier<'a>, flags: SymbolFlags) {
        match &mut self.inner {
            Inner::Builder(builder) => {
                let kind = AstKind::BindingIdentifier(identifier);
                builder.enter_node(kind);
                let symbol_id = builder.declare_symbol(
                    identifier.span,
                    identifier.name,
                    flags,
                    SymbolFlags::empty(),
                );
                identifier.symbol_id.set(Some(symbol_id));
                builder.leave_node(kind);
            }
            Inner::Counter(counter) => counter.visit_binding_identifier(identifier),
        }
    }

    /// Visits an expression evaluated in the current scope.
    pub fn visit_expression(&mut self, expression: &'a Expression<'a>) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.visit_expression(expression),
            Inner::Counter(counter) => counter.visit_expression(expression),
        }
    }

    /// Visits statements that run in the current scope, such as a second script's body.
    pub fn visit_statements(&mut self, statements: &'a ArenaVec<'a, Statement<'a>>) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.visit_statements(statements),
            Inner::Counter(counter) => counter.visit_statements(statements),
        }
    }

    /// Declares a parameter in the current scope, such as the `item` of `{#each items as item}`.
    pub fn visit_formal_parameter(&mut self, parameter: &'a FormalParameter<'a>) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.visit_formal_parameter(parameter),
            Inner::Counter(counter) => counter.visit_formal_parameter(parameter),
        }
    }

    /// Declares a parameter list in the current scope, such as that of `{#snippet name(a, b)}`.
    pub fn visit_formal_parameters(&mut self, parameters: &'a FormalParameters<'a>) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.visit_formal_parameters(parameters),
            Inner::Counter(counter) => counter.visit_formal_parameters(parameters),
        }
    }

    /// Visits a declaration in the current scope, such as `{@const a = b}`.
    pub fn visit_variable_declaration(&mut self, declaration: &'a VariableDeclaration<'a>) {
        match &mut self.inner {
            Inner::Builder(builder) => builder.visit_variable_declaration(declaration),
            Inner::Counter(counter) => counter.visit_variable_declaration(declaration),
        }
    }
}

#[cfg(test)]
mod tests;
