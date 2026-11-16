use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, Statement};
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Prints `source` with each statement `"m";` replaced by the statements of `prelude`, which are
/// parsed once into an arena that is never freed, as a constant tree is.
fn expanded(source: &str, prelude: &'static str) -> String {
    let prelude_allocator: &'static Allocator = Box::leak(Box::default());
    let parsed = Parser::new(prelude_allocator, prelude, SourceType::mjs()).parse();
    let body: &'static [Statement<'static>] = prelude_allocator.alloc(parsed.program.body);
    let expand: &dyn for<'s, 'x> Fn(&'s Statement<'x>) -> Option<&'s [Statement<'s>]> =
        &move |stmt| match stmt {
            Statement::ExpressionStatement(s)
                if matches!(
                    s.expression.without_parentheses(),
                    Expression::StringLiteral(l) if l.value.as_str() == Some("m")
                ) =>
            {
                Some(body)
            }
            _ => None,
        };
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    assert!(parsed.diagnostics.is_empty());
    Codegen::new().with_statement_expansion(expand).build(&parsed.program).code
}

fn printed(source: &str) -> String {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    Codegen::new().build(&parsed.program).code
}

#[test]
fn an_expanded_statement_prints_as_the_statements_written_in_its_place() {
    let prelude = "const $$a = 1;\nfunction $$f(x) { return x + $$a; }";
    for (source, inline) in [
        (
            "import a from 'a';\n'm';\nf(a);",
            "import a from 'a';\nconst $$a = 1;\nfunction $$f(x) { return x + $$a; }\nf(a);",
        ),
        (
            "function h() { x(); 'm'; }",
            "function h() { x(); const $$a = 1; function $$f(x) { return x + $$a; } }",
        ),
        ("('m');\nf();", "const $$a = 1;\nfunction $$f(x) { return x + $$a; }\nf();"),
        ("if (c) { 'm'; }", "if (c) { const $$a = 1; function $$f(x) { return x + $$a; } }"),
    ] {
        assert_eq!(expanded(source, prelude), printed(inline), "{source}");
    }
}

#[test]
fn without_an_expansion_a_statement_prints_as_itself() {
    let source = "f();\n'm';";
    assert_eq!(expanded(source, "g();"), printed("f();\ng();"));
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    assert_eq!(Codegen::new().build(&parsed.program).code, "f();\n\"m\";\n");
}
