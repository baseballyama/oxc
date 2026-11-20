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
    let (_, log) = hooked("a;\nb;\n", &[]);
    // A statement's `next` is where the next statement ends, the last one's the owner's end.
    assert_eq!(lines(&log, "body"), ["body 0..6"]);
    assert_eq!(lines(&log, "trailing"), ["trailing 2 5", "trailing 5 6"]);
    assert_eq!(lines(&log, "until"), ["until 5 6"]);
}

#[test]
fn every_node_and_expression_is_reported_before_it_prints() {
    let (_, log) = hooked("f(a);", &[]);
    let mut expressions = lines(&log, "expr");
    expressions.dedup();
    assert_eq!(expressions, ["expr 0..4", "expr 0..1", "expr 2..3"]);
    // The program, the statement and the expression statement share the span.
    assert_eq!(lines(&log, "node 0..5").len(), 3);
}

#[test]
fn a_list_item_reports_the_next_item_s_start_and_the_last_one_the_list_s_end() {
    let (_, log) = hooked("f(a, b);", &[]);
    assert_eq!(lines(&log, "trailing 3"), ["trailing 3 5"]);
    assert_eq!(lines(&log, "trailing 6"), ["trailing 6 7"]);
    let (_, log) = hooked("[a, b];", &[]);
    assert_eq!(lines(&log, "trailing 2"), ["trailing 2 4"]);
    assert_eq!(lines(&log, "until"), ["until 5 6", "until 7 7"]);
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
    let mut parsed = Parser::new(&allocator, "a;\nb;\n", SourceType::mjs()).parse();
    if let Statement::ExpressionStatement(s) = &mut parsed.program.body[1] {
        s.span = SPAN;
    }
    let (_, log) = print(&parsed.program, &[]);
    assert!(log.contains(&"node -".to_owned()), "{log:?}");
    assert_eq!(lines(&log, "trailing"), ["trailing 2 -", "trailing - 6"]);
}
