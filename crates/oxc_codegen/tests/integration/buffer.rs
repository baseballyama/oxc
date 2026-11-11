use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;

#[test]
fn with_buffer_prints_into_the_given_memory() {
    let allocator = Allocator::default();
    let program = Parser::new(&allocator, "let a = 1;", SourceType::mjs()).parse().program;

    let mut buffer = String::with_capacity(1 << 20);
    buffer.push_str("left over");
    let pointer = buffer.as_ptr();
    let code = Codegen::new()
        .with_options(CodegenOptions::default())
        .with_buffer(buffer)
        .build(&program)
        .code;
    assert_eq!(code, "let a = 1;\n");
    assert_eq!(code.as_ptr(), pointer);
    assert!(code.capacity() >= 1 << 20);
}
