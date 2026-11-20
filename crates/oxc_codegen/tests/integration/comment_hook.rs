use std::{cell::RefCell, rc::Rc};

use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, Statement};
use oxc_codegen::{Codegen, CodegenOptions, CommentHook, CommentOptions, CommentWriter};
use oxc_parser::Parser;
use oxc_span::{SPAN, SourceType, Span};

/// Records what the printer reports, and writes `/*@N*/` before the node that starts at `N` for
/// each `N` in `mark`.
struct Recorder {
    log: Rc<RefCell<Vec<String>>>,
    mark: Vec<u32>,
}

fn position(at: Option<u32>) -> String {
    at.map_or_else(|| "-".to_owned(), |at| at.to_string())
}

fn range(span: Option<Span>) -> String {
    span.map_or_else(|| "-".to_owned(), |s| format!("{}..{}", s.start, s.end))
}

impl CommentHook for Recorder {
    fn before(&mut self, out: &mut CommentWriter<'_, '_>, span: Option<Span>, expression: bool) {
        if let Some(span) = span
            && let Some(i) = self.mark.iter().position(|&m| m == span.start)
        {
            self.mark.remove(i);
            out.write(&format!("/*@{}*/", span.start));
            out.space();
        }
        let kind = if expression { "expr" } else { "node" };
        self.log.borrow_mut().push(format!("{kind} {}", range(span)));
    }

    fn after(&mut self, _out: &mut CommentWriter<'_, '_>, _span: Option<Span>) {}

    fn body(&mut self, owner: Option<Span>) {
        self.log.borrow_mut().push(format!("body {}", range(owner)));
    }

    fn trailing(&mut self, _out: &mut CommentWriter<'_, '_>, end: Option<u32>, next: Option<u32>) {
        self.log.borrow_mut().push(format!("trailing {} {}", position(end), position(next)));
    }

    fn until(&mut self, _out: &mut CommentWriter<'_, '_>, from: Option<u32>, to: Option<u32>) {
        self.log.borrow_mut().push(format!("until {} {}", position(from), position(to)));
    }

    /// Before everything, so that every node after the first byte is reported.
    fn next_comment(&self) -> Option<u32> {
        Some(0)
    }
}

fn print(program: &Program<'_>, mark: &[u32]) -> (String, Vec<String>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let hook = Recorder { log: Rc::clone(&log), mark: mark.to_vec() };
    let options =
        CodegenOptions { comments: CommentOptions::disabled(), ..CodegenOptions::default() };
    let code =
        Codegen::new().with_options(options).with_comment_hook(Box::new(hook)).build(program).code;
    let log = log.borrow().clone();
    (code, log)
}

fn hooked(source: &str, mark: &[u32]) -> (String, Vec<String>) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    assert!(parsed.diagnostics.is_empty());
    print(&parsed.program, mark)
}

fn lines(log: &[String], prefix: &str) -> Vec<String> {
    log.iter().filter(|l| l.starts_with(prefix)).cloned().collect()
}

#[test]
fn a_statement_list_reports_its_owner_each_statement_and_its_end() {
    let (_, log) = hooked("\na;\nb;\n", &[]);
    // A statement's `next` is where the next statement ends, the last one's the owner's end.
    assert_eq!(lines(&log, "body"), ["body 0..7"]);
    assert_eq!(lines(&log, "trailing"), ["trailing 3 6", "trailing 6 7"]);
    assert_eq!(lines(&log, "until"), ["until 6 7"]);
}

#[test]
fn a_node_after_the_next_comment_is_reported_before_it_prints() {
    let (_, log) = hooked(" f(a);", &[]);
    let mut expressions = lines(&log, "expr");
    expressions.dedup();
    assert_eq!(expressions, ["expr 1..5", "expr 1..2", "expr 3..4"]);
    // The statement and the expression statement share the span.
    assert_eq!(lines(&log, "node 1..6").len(), 2);
}

#[test]
fn a_node_before_the_next_comment_is_not_reported() {
    struct Late(Rc<RefCell<Vec<u32>>>);
    impl CommentHook for Late {
        fn before(&mut self, _: &mut CommentWriter<'_, '_>, span: Option<Span>, _: bool) {
            self.0.borrow_mut().extend(span.map(|s| s.start));
        }
        fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
        fn body(&mut self, _: Option<Span>) {}
        fn trailing(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn until(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn next_comment(&self) -> Option<u32> {
            Some(4)
        }
    }
    let seen = Rc::new(RefCell::new(Vec::new()));
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "f(a, b);", SourceType::mjs()).parse();
    let _ =
        Codegen::new().with_comment_hook(Box::new(Late(Rc::clone(&seen)))).build(&parsed.program);
    // `b` and its wrappers; nothing before the comment.
    assert!(!seen.borrow().is_empty() && seen.borrow().iter().all(|&start| start == 5), "{seen:?}");
}

#[test]
fn a_list_item_reports_the_next_item_s_start_and_the_last_one_the_list_s_end() {
    let (_, log) = hooked(" f(a, b);", &[]);
    assert_eq!(lines(&log, "trailing 4"), ["trailing 4 6"]);
    assert_eq!(lines(&log, "trailing 7"), ["trailing 7 8"]);
    let (_, log) = hooked(" [a, b];", &[]);
    assert_eq!(lines(&log, "trailing 3"), ["trailing 3 5"]);
    assert_eq!(lines(&log, "until"), ["until 6 7", "until 8 8"]);
}

#[test]
fn what_the_hook_writes_lands_before_the_node() {
    let (code, _) = hooked("f(a, b);\n", &[5]);
    assert_eq!(code, "f(a, /*@5*/ b);\n");
}

#[test]
fn a_line_comment_ends_its_line() {
    struct Line;
    impl CommentHook for Line {
        fn before(&mut self, out: &mut CommentWriter<'_, '_>, span: Option<Span>, _: bool) {
            if span.is_some_and(|s| s.start == 5) {
                out.line_comment("// c");
            }
        }
        fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
        fn body(&mut self, _: Option<Span>) {}
        fn trailing(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn until(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn next_comment(&self) -> Option<u32> {
            Some(0)
        }
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "f(a, b);", SourceType::mjs()).parse();
    let code = Codegen::new().with_comment_hook(Box::new(Line)).build(&parsed.program).code;
    let reparsed = Parser::new(&allocator, &code, SourceType::mjs()).parse();
    assert!(reparsed.diagnostics.is_empty(), "{code}");
    assert!(code.contains("// c\n"), "{code}");
    assert!(code.contains('b'), "{code}");
}

#[test]
fn a_node_without_a_span_has_no_position() {
    let allocator = Allocator::default();
    let mut parsed = Parser::new(&allocator, "\na;\nb;\n", SourceType::mjs()).parse();
    if let Statement::ExpressionStatement(s) = &mut parsed.program.body[1] {
        s.span = SPAN;
    }
    let (_, log) = print(&parsed.program, &[]);
    assert!(!log.iter().any(|l| l.starts_with("node -")), "{log:?}");
    // The statement after it has no end to go to, and the one without a span is not reported.
    assert_eq!(lines(&log, "trailing"), ["trailing 3 -"]);
}

#[test]
fn a_space_at_the_start_of_a_line_is_the_indent() {
    struct Trailing;
    impl CommentHook for Trailing {
        fn before(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>, _: bool) {}
        fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
        fn body(&mut self, _: Option<Span>) {}
        fn trailing(&mut self, out: &mut CommentWriter<'_, '_>, end: Option<u32>, _: Option<u32>) {
            if end == Some(17) {
                out.space();
                out.line_comment("// c");
            }
        }
        fn until(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn next_comment(&self) -> Option<u32> {
            Some(0)
        }
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "function f() { a; b; }", SourceType::mjs()).parse();
    let code = Codegen::new().with_comment_hook(Box::new(Trailing)).build(&parsed.program).code;
    assert_eq!(code, "function f() {\n\ta;\n\t// c\n\tb;\n}\n");
}

#[test]
fn a_held_line_takes_a_trailing_comment() {
    struct Trailing;
    impl CommentHook for Trailing {
        fn before(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>, _: bool) {}
        fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
        fn body(&mut self, _: Option<Span>) {}
        fn trailing(&mut self, out: &mut CommentWriter<'_, '_>, end: Option<u32>, _: Option<u32>) {
            if end == Some(17) {
                out.append(" // c");
                out.newline();
            }
        }
        fn until(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn next_comment(&self) -> Option<u32> {
            Some(17)
        }
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "function f() { a; b; }", SourceType::mjs()).parse();
    let code = Codegen::new().with_comment_hook(Box::new(Trailing)).build(&parsed.program).code;
    assert_eq!(code, "function f() {\n\ta; // c\n\tb;\n}\n");
    let options =
        CodegenOptions { source_map_path: Some("x.js".into()), ..CodegenOptions::default() };
    let mapped = Codegen::new()
        .with_options(options)
        .with_comment_hook(Box::new(Trailing))
        .build(&parsed.program);
    assert_eq!(mapped.code, code);
}

/// Writes `/*c*/` or `//c` before the node that starts at `at`.
struct Before {
    at: u32,
    text: &'static str,
    done: bool,
}

impl CommentHook for Before {
    fn before(&mut self, out: &mut CommentWriter<'_, '_>, span: Option<Span>, _: bool) {
        if !self.done && span.is_some_and(|s| s.start == self.at) {
            self.done = true;
            if self.text.starts_with("//") {
                out.line_comment(self.text);
            } else {
                out.write(self.text);
                out.space();
            }
        }
    }
    fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
    fn body(&mut self, _: Option<Span>) {}
    fn trailing(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
    fn until(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
    fn next_comment(&self) -> Option<u32> {
        (!self.done).then(|| self.at - 1)
    }
    fn breaks_before(&self, to: u32) -> bool {
        !self.done && to == self.at && self.text.starts_with("//")
    }
}

fn before(source: &str, at: u32, text: &'static str) -> String {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let hook = Before { at, text, done: false };
    Codegen::new().with_comment_hook(Box::new(hook)).build(&parsed.program).code
}

#[test]
fn a_comment_before_an_arrow_body_keeps_the_object_s_parentheses() {
    // `() => ({ n })` with a comment before the object, which goes inside its parentheses.
    let code = before("f(() => ({ n }));", 9, "/*c*/");
    assert_eq!(code, "f(() => (/*c*/ { n }));\n");
}

#[test]
fn a_comment_before_a_statement_keeps_its_parentheses() {
    let code = before(" ({ a } = b);", 1, "/*c*/");
    assert_eq!(code, "/*c*/ ({a} = b);\n");
}

#[test]
fn a_returned_value_after_a_comment_is_parenthesized() {
    let code = before("function f() { return a; }", 22, "// c");
    let allocator = Allocator::default();
    let reparsed = Parser::new(&allocator, &code, SourceType::mjs()).parse();
    assert!(reparsed.diagnostics.is_empty(), "{code}");
    assert!(code.contains("return (// c\n"), "{code}");
    let code = before("function f() { throw a; }", 21, "// c");
    assert!(code.contains("throw (// c\n"), "{code}");
}

#[test]
fn a_node_s_own_parentheses_go_before_its_comment() {
    let code = before("async function f() { return (await x)(); }", 29, "// c");
    assert!(code.contains("return (// c\n"), "{code}");
    assert!(code.contains("await x)()"), "{code}");
    let allocator = Allocator::default();
    let reparsed = Parser::new(&allocator, &code, SourceType::mjs()).parse();
    assert!(reparsed.diagnostics.is_empty(), "{code}");
}

#[test]
fn a_thrown_value_is_parenthesized_only_after_a_line_comment() {
    let code = before("function f() { throw a; }", 21, "/*c*/");
    assert!(code.contains("throw /*c*/ a;"), "{code}");
}

#[test]
fn the_line_after_a_comment_is_indented_as_the_code_on_it() {
    // A line comment before the second argument ends its line; the argument goes on the next line
    // at the call's indent.
    let code = before("function f() { g(a, b); }", 20, "// c");
    assert_eq!(code, "function f() {\n\tg(a, // c\n\tb);\n}\n");
}

/// Breaks every list that has a comment: the arguments of a call go one per line.
/// Writes `// c`, a comment that starts at `from`, before the first node after it.
struct Breaking {
    from: u32,
    done: bool,
}

impl CommentHook for Breaking {
    fn before(&mut self, out: &mut CommentWriter<'_, '_>, span: Option<Span>, _: bool) {
        if !self.done && span.is_some_and(|s| s.start > self.from) {
            self.done = true;
            out.line_comment("// c");
        }
    }
    fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
    fn body(&mut self, _: Option<Span>) {}
    fn trailing(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
    fn until(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
    fn next_comment(&self) -> Option<u32> {
        (!self.done).then_some(self.from)
    }
    fn breaks(&self, _: u32, until: u32, _: bool) -> bool {
        !self.done && self.from < until
    }
}

#[test]
fn a_list_a_comment_breaks_puts_each_item_on_its_own_line() {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, "function f() { g(a, b); }", SourceType::mjs()).parse();
    let hook = Breaking { from: 19, done: false };
    let code = Codegen::new().with_comment_hook(Box::new(hook)).build(&parsed.program).code;
    assert_eq!(code, "function f() {\n\tg(\n\t\ta,\n\t\t// c\n\t\tb\n\t);\n}\n");
}

#[test]
fn parameters_and_an_object_pattern_a_comment_breaks_put_each_item_on_its_own_line() {
    let allocator = Allocator::default();
    // The second comment starts before the parentheses, and the first parameter writes it.
    for (source, from, expected) in [
        ("((a, b) => a);", 4, "((\n\ta,\n\t// c\n\tb\n) => a);\n"),
        ("(<T,>(a, b) => a);", 2, "(<T,>(\n\t// c\n\ta,\n\tb\n) => a);\n"),
        ("let { a, ...b } = c;", 8, "let {\n\ta,\n\t// c\n\t...b\n} = c;\n"),
    ] {
        let parsed = Parser::new(&allocator, source, SourceType::tsx()).parse();
        let hook = Breaking { from, done: false };
        let code = Codegen::new().with_comment_hook(Box::new(hook)).build(&parsed.program).code;
        assert_eq!(code, expected, "{source}");
    }
}

#[test]
fn an_empty_body_that_holds_a_comment_opens_a_line_for_it() {
    /// One comment inside the bodies that start at 13, which ends its line if it is a line comment.
    struct Inside(bool, &'static str);
    impl CommentHook for Inside {
        fn before(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>, _: bool) {}
        fn after(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<Span>) {}
        fn body(&mut self, _: Option<Span>) {}
        fn trailing(&mut self, _: &mut CommentWriter<'_, '_>, _: Option<u32>, _: Option<u32>) {}
        fn until(&mut self, out: &mut CommentWriter<'_, '_>, _: Option<u32>, to: Option<u32>) {
            if self.0 && to.is_some_and(|to| to > 13) {
                self.0 = false;
                out.write(self.1);
                if self.1.starts_with("//") {
                    out.newline();
                }
            }
        }
        fn next_comment(&self) -> Option<u32> {
            self.0.then_some(14)
        }
        fn holds(&self, span: Span) -> bool {
            self.0 && span.start == 13
        }
    }
    let allocator = Allocator::default();
    for (source, comment, expected) in [
        ("function f() {}", "// c", "function f() {\n\t// c\n}\n"),
        ("function f() {}", "/* c */", "function f() {\n\t/* c */\n}\n"),
        ("if (a) f();  { }", "// c", "if (a) f();\n{\n\t// c\n}\n"),
        ("class A      { }", "/* c */", "class A {\n\t/* c */\n}\n"),
    ] {
        let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
        let hook = Box::new(Inside(true, comment));
        let code = Codegen::new().with_comment_hook(hook).build(&parsed.program).code;
        assert_eq!(code, expected, "{source}");
    }
}
