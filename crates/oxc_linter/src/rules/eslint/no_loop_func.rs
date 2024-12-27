use crate::{context::LintContext, rule::Rule, AstNode};
use oxc_ast::AstKind;
use oxc_diagnostics::OxcDiagnostic;
use oxc_macros::declare_oxc_lint;
use oxc_span::{GetSpan, Span};

fn no_loop_func_diagnostic(span: Span) -> OxcDiagnostic {
    OxcDiagnostic::warn(
        "Function declared in a loop contains unsafe references to variable(s) {{ varNames }}.",
    )
    .with_label(span)
}

#[derive(Debug, Default, Clone)]
pub struct NoLoopFunc;

declare_oxc_lint!(
    /// ### What it does
    ///
    /// Disallow function declarations that contain unsafe references inside loop statements.
    ///
    /// ### Why is this bad?
    ///
    /// Defining functions inside loops can lead to unexpected behavior, such as capturing the loop variable incorrectly due to closures.
    /// It may also affect performance, as functions are re-created on each iteration instead of being reused.
    ///
    /// ### Examples
    ///
    /// Examples of **incorrect** code for this rule:
    /// ```js
    /// for (let i = 0; i < 10; i++) {
    ///   function foo() {
    ///     console.log(i);
    ///   }
    ///   foo();
    /// }
    /// ```
    ///
    /// Examples of **correct** code for this rule:
    /// ```js
    /// function foo(i) {
    ///   console.log(i);
    /// }
    ///
    /// for (let i = 0; i < 10; i++) {
    ///   foo(i);
    /// }
    /// ```
    NoLoopFunc,
    correctness,
    suggestion
);

impl Rule for NoLoopFunc {
    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        match node.kind() {
            AstKind::IdentifierReference(identifier_reference) => {
                println!("--------------------");
                if let Some(root) = ctx.nodes().root() {
                    let root_node = ctx.nodes().get_node(root);
                    println!("Root: {:?}", ctx.source_range(root_node.span()));
                }
                println!("Code: {:?}", ctx.source_range(node.span()));
                if let Some(function) = get_function(&node, &ctx) {
                    println!("function: {:?}", ctx.source_range(function.span()));
                    let function_span = function.span();
                    let reference_id = identifier_reference.reference_id();
                    let reference = ctx.symbols().get_reference(reference_id);
                    println!("reference_id: {:?}", reference_id);
                    let scopes =
                        ctx.scopes().find_binding(node.scope_id(), &identifier_reference.name);
                    if let Some(scope) = scopes {
                        println!("scope: {:?}", scope);
                        let symbol_node = ctx.symbols().get_declaration(scope);
                        let symbol_span = ctx.nodes().get_node(symbol_node).span();
                        println!("symbol: {:?}", ctx.source_range(symbol_span));
                        let mut parent_node = function;
                        while let Some(node) = ctx.nodes().parent_node(parent_node.id()) {
                            if let AstKind::Program(_) = node.kind() {
                                break;
                            }

                            let node_span = node.span();
                            println!("node_span: {:?}", ctx.source_range(node_span));
                            println!("node: {:?}", ctx.source_range(node.span()));
                            println!("symbol_span: {:?}", ctx.source_range(symbol_span));
                            println!("symbol: {:?}", ctx.source_range(symbol_span));
                            if node_span.start <= symbol_span.start
                                && symbol_span.end <= node_span.end
                            {
                                ctx.diagnostic(no_loop_func_diagnostic(function_span));
                                break;
                            }

                            parent_node = *node;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn get_function<'a>(node: &'a AstNode<'a>, ctx: &'a LintContext) -> Option<AstNode<'a>> {
    let mut current_node = Some(node);
    while let Some(node) = current_node {
        if let AstKind::Function(_) = node.kind() {
            return Some(*node);
        }
        current_node = ctx.nodes().parent_node(node.id());
    }
    None
}

// fn is_iife(node: &AstNode, ctx: &LintContext) -> bool {
//     let parent = ctx.nodes().parent_node(node.id());
//     matches!(node.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_))
//         && parent.map_or(false, |parent| {
//             matches!(parent.kind(), AstKind::CallExpression(_)) && parent.id() == node.id()
//         })
// }
//
// fn get_containing_loop_node<'a>(
//     node: &'a AstNode,
//     ctx: &'a LintContext,
// ) -> Option<&'a AstNode<'a>> {
//     let mut current_node = node;
//
//     while let Some(parent) = ctx.nodes().parent_node(current_node.id()) {
//         match parent.kind() {
//             AstKind::WhileStatement(_) | AstKind::DoWhileStatement(_) => {
//                 return Some(parent);
//             }
//
//             AstKind::ForStatement(parent_statement) => {
//                 // `init` is outside of the loop.
//                 if parent_statement.init?.span() != current_node.span() {
//                     return Some(parent);
//                 }
//             }
//
//             AstKind::ForInStatement(parent_statement) => {
//                 if parent_statement.right.span() != current_node.span() {
//                     return Some(parent);
//                 }
//             }
//
//             AstKind::ForOfStatement(parent_statement) => {
//                 if parent_statement.right.span() != current_node.span() {
//                     return Some(parent);
//                 }
//             }
//
//             AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
//                 if is_iife(parent, ctx) {
//                     break;
//                 }
//                 return None;
//             }
//
//             _ => {}
//         }
//
//         current_node = parent;
//     }
//
//     None
// }
//
// fn get_top_loop_node<'a>(
//     node: &'a AstNode,
//     excluded_node: Option<&'a AstNode>,
//     ctx: &'a LintContext,
// ) -> &'a AstNode<'a> {
//     let border = excluded_node.map_or(0, |n| n.span().end);
//     let mut retv = node;
//     let mut containing_loop_node = Some(node);
//
//     while let Some(current_node) = containing_loop_node {
//         if current_node.span().start < border {
//             break;
//         }
//         retv = current_node;
//         containing_loop_node = get_containing_loop_node(current_node, ctx);
//     }
//
//     retv
// }
//
// fn is_safe<'a>(loop_node: &'a AstNode, reference: &'a Reference, ctx: &'a LintContext) -> bool {
//     let variable = ctx.nodes().get_node(reference.node_id());
//     let declaration = ctx.nodes().parent_node(reference.node_id());
//     let kind = match declaration.and_then(|decl| decl.kind().as_variable_declaration()) {
//         Some(variable_decl) => variable_decl.kind.as_str(),
//         None => "",
//     };
//
//     if kind == "const" {
//         return true;
//     }
//
//     if kind == "let"
//         && declaration.map_or(false, |decl| {
//             let decl_span = decl.span();
//             let loop_span = loop_node.span();
//             decl_span.start > loop_span.start && decl_span.end < loop_span.end
//         })
//     {
//         return true;
//     }
//
//     let border = get_top_loop_node(loop_node, if kind == "let" { declaration } else { None }, ctx)
//         .span()
//         .start;
//
//     let is_safe_reference = |upper_ref: &Reference| {
//         let id = upper_ref.node_id();
//         let node = ctx.nodes().get_node(id);
//
//         !upper_ref.is_write()
//             || (variable.id() == upper_ref.node_id()) && node.span().start < border
//     };
//
//     false
//
//     // variable.map_or(false, |v| v.references().iter().all(is_safe_reference))
// }
//
// fn check_for_loops<'a>(node: &'a AstNode, ctx: &'a LintContext, source_code: &'a str) {
//     let Some(loop_node) = get_containing_loop_node(node, ctx) else {
//         return;
//     };
//
//     let references = ctx.scopes().get_bindings(node.scope_id());
//
//     if let AstKind::Function(function) = node.kind() {
//         if function.generator || function.r#async {
//             return;
//         }
//     }
//
//     if is_iife(node, ctx) {
//         if let AstKind::Function(function) = node.kind() {
//             if function.generator || function.r#async {
//                 return;
//             }
//         }
//
//         let is_function_referenced = if let AstKind::Function(function) = node.kind() {
//             if let Some(id) = &function.id {
//                 let id_name = &id.name;
//                 references.iter().any(|r| r.0 == id_name)
//             } else {
//                 false
//             }
//         } else {
//             false
//         };
//
//         // if !is_function_referenced {
//         //     mark_skipped_iife(node);
//         //     return;
//         // }
//     }
//
//     let unsafe_refs: Vec<_> = references
//         .iter()
//         .filter(|r| !is_safe(loop_node, r., ctx))
//         .map(|r| r.identifier().name())
//         .collect();
//
//     if !unsafe_refs.is_empty() {
//         ctx.report(node, "unsafeRefs", Some(format!("'{}'", unsafe_refs.join("', '"))));
//     }
// }

#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec![
        "string = 'function a() {}';",
        "for (var i=0; i<l; i++) { } var a = function() { i; };",
        "for (var i=0, a=function() { i; }; i<l; i++) { }",
        "for (var x in xs.filter(function(x) { return x != upper; })) { }",
        "for (var x of xs.filter(function(x) { return x != upper; })) { }", // { "ecmaVersion": 6 },
        "for (var i=0; i<l; i++) { (function() {}) }",
        "for (var i in {}) { (function() {}) }",
        "for (var i of {}) { (function() {}) }", // { "ecmaVersion": 6 },
        "for (let i=0; i<l; i++) { (function() { i; }) }", // { "ecmaVersion": 6 },
        "for (let i in {}) { i = 7; (function() { i; }) }", // { "ecmaVersion": 6 },
        "for (const i of {}) { (function() { i; }) }", // { "ecmaVersion": 6 },
        "for (let i = 0; i < 10; ++i) { for (let x in xs.filter(x => x != i)) {  } }", // { "ecmaVersion": 6 },
        "let a = 0; for (let i=0; i<l; i++) { (function() { a; }); }", // { "ecmaVersion": 6 },
        "let a = 0; for (let i in {}) { (function() { a; }); }",       // { "ecmaVersion": 6 },
        "let a = 0; for (let i of {}) { (function() { a; }); }",       // { "ecmaVersion": 6 },
        "let a = 0; for (let i=0; i<l; i++) { (function() { (function() { a; }); }); }", // { "ecmaVersion": 6 },
        "let a = 0; for (let i in {}) { function foo() { (function() { a; }); } }", // { "ecmaVersion": 6 },
        "let a = 0; for (let i of {}) { (() => { (function() { a; }); }); }", // { "ecmaVersion": 6 },
        "var a = 0; for (let i=0; i<l; i++) { (function() { a; }); }", // { "ecmaVersion": 6 },
        "var a = 0; for (let i in {}) { (function() { a; }); }",       // { "ecmaVersion": 6 },
        "var a = 0; for (let i of {}) { (function() { a; }); }",       // { "ecmaVersion": 6 },
        "let result = {};
			for (const score in scores) {
			  const letters = scores[score];
			  letters.split('').forEach(letter => {
			    result[letter] = score;
			  });
			}
			result.__default = 6;", // { "ecmaVersion": 6 },
        "while (true) {
			    (function() { a; });
			}
			let a;",      // { "ecmaVersion": 6 },
        "while(i) { (function() { i; }) }",
        "do { (function() { i; }) } while (i)",
        "var i; while(i) { (function() { i; }) }",
        "var i; do { (function() { i; }) } while (i)",
        "for (var i=0; i<l; i++) { (function() { undeclared; }) }", // { "ecmaVersion": 6 },
        "for (let i=0; i<l; i++) { (function() { undeclared; }) }", // { "ecmaVersion": 6 },
        "for (var i in {}) { i = 7; (function() { undeclared; }) }", // { "ecmaVersion": 6 },
        "for (let i in {}) { i = 7; (function() { undeclared; }) }", // { "ecmaVersion": 6 },
        "for (const i of {}) { (function() { undeclared; }) }",     // { "ecmaVersion": 6 },
        "for (let i = 0; i < 10; ++i) { for (let x in xs.filter(x => x != undeclared)) {  } }", // { "ecmaVersion": 6 },
        "
			            let current = getStart();
			            while (current) {
			            (() => {
			                current;
			                current.a;
			                current.b;
			                current.c;
			                current.d;
			            })();
			            
			            current = current.upper;
			            }
			            ", // { "ecmaVersion": 6 },
        "for (var i=0; (function() { i; })(), i<l; i++) { }",
        "for (var i=0; i<l; (function() { i; })(), i++) { }",
        "for (var i = 0; i < 10; ++i) { (()=>{ i;})() }", // { "ecmaVersion": 6 },
        "for (var i = 0; i < 10; ++i) { (function a(){i;})() }", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((f => f)((() => i)()));
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((() => {
			                    return (() => i)();
			                })());
			            }
			            ", // { "ecmaVersion": 6 }
    ];

    let fail = vec![
        "for (var i=0; i<l; i++) { (function() { i; }) }",
        "for (var i=0; i<l; i++) { for (var j=0; j<m; j++) { (function() { i+j; }) } }",
        "for (var i in {}) { (function() { i; }) }",
        "for (var i of {}) { (function() { i; }) }", // { "ecmaVersion": 6 },
        "for (var i=0; i < l; i++) { (() => { i; }) }", // { "ecmaVersion": 6 },
        "for (var i=0; i < l; i++) { var a = function() { i; } }",
        "for (var i=0; i < l; i++) { function a() { i; }; a(); }",
        "let a; for (let i=0; i<l; i++) { a = 1; (function() { a; });}", // { "ecmaVersion": 6 },
        "let a; for (let i in {}) { (function() { a; }); a = 1; }",      // { "ecmaVersion": 6 },
        "let a; for (let i of {}) { (function() { a; }); } a = 1; ",     // { "ecmaVersion": 6 },
        "let a; for (let i=0; i<l; i++) { (function() { (function() { a; }); }); a = 1; }", // { "ecmaVersion": 6 },
        "let a; for (let i in {}) { a = 1; function foo() { (function() { a; }); } }", // { "ecmaVersion": 6 },
        "let a; for (let i of {}) { (() => { (function() { a; }); }); } a = 1;", // { "ecmaVersion": 6 },
        "for (var i = 0; i < 10; ++i) { for (let x in xs.filter(x => x != i)) {  } }", // { "ecmaVersion": 6 },
        "for (let x of xs) { let a; for (let y of ys) { a = 1; (function() { a; }); } }", // { "ecmaVersion": 6 },
        "for (var x of xs) { for (let y of ys) { (function() { x; }); } }", // { "ecmaVersion": 6 },
        "for (var x of xs) { (function() { x; }); }",                       // { "ecmaVersion": 6 },
        "var a; for (let x of xs) { a = 1; (function() { a; }); }",         // { "ecmaVersion": 6 },
        "var a; for (let x of xs) { (function() { a; }); a = 1; }",         // { "ecmaVersion": 6 },
        "let a; function foo() { a = 10; } for (let x of xs) { (function() { a; }); } foo();", // { "ecmaVersion": 6 },
        "let a; function foo() { a = 10; for (let x of xs) { (function() { a; }); } } foo();", // { "ecmaVersion": 6 },
        "let a; for (var i=0; i<l; i++) { (function* (){i;})() }", // { "ecmaVersion": 6 },
        "let a; for (var i=0; i<l; i++) { (async function (){i;})() }", // { "ecmaVersion": 2022 },
        "
			            let current = getStart();
			            const arr = [];
			            while (current) {
			                (function f() {
			                    current;
			                    arr.push(f);
			                })();
			                
			                current = current.upper;
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                (function fun () {
			                    if (arr.includes(fun)) return i;
			                    else arr.push(fun);
			                })();
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            let current = getStart();
			            const arr = [];
			            while (current) {
			                const p = (async () => {
			                    await someDelay();
			                    current;
			                })();
			
			                arr.push(p);
			                current = current.upper;
			            }
			            ", // { "ecmaVersion": 2022 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((f => f)(
			                    () => i
			                ));
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((() => {
			                    return () => i;
			                })());
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((() => {
			                    return () => { return i };
			                })());
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((() => {
			                    return () => {
			                        return () => i
			                    };
			                })());
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i++) {
			                arr.push((() => {
			                    return () => 
			                        (() => i)();
			                })());
			            }
			            ", // { "ecmaVersion": 6 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i ++) {
			                (() => {
			                    arr.push((async () => {
			                        await 1;
			                        return i;
			                    })());
			                })();
			            }
			            ", // { "ecmaVersion": 2022 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i ++) {
			                (() => {
			                    (function f() {
			                        if (!arr.includes(f)) {
			                            arr.push(f);
			                        }
			                        return i;
			                    })();
			                })();
			            
			            }
			            ", // { "ecmaVersion": 2022 },
        r#"
			            var arr1 = [], arr2 = [];
			
			            for (var [i, j] of ["a", "b", "c"].entries()) {
			                (() => {
			                    arr1.push((() => i)());
			                    arr2.push(() => j);
			                })();
			            }
			            "#, // { "ecmaVersion": 2022 },
        "
			            var arr = [];
			
			            for (var i = 0; i < 5; i ++) {
			                ((f) => {
			                    arr.push(f);
			                })(() => {
			                    return (() => i)();
			                });
			
			            }
			            ", // { "ecmaVersion": 2022 },
        "
			            for (var i = 0; i < 5; i++) {
			                (async () => {
			                    () => i;
			                })();
			            }
			            ", // { "ecmaVersion": 2022 }
    ];

    Tester::new(NoLoopFunc::NAME, NoLoopFunc::CATEGORY, pass, fail).test_and_snapshot();
}
