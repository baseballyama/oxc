#![expect(clippy::missing_panics_doc, clippy::literal_string_with_formatting_args)]
pub mod ascii_only;
pub mod buffer;
pub mod comment_hook;
pub mod comments;
pub mod esbuild;
pub mod js;
#[cfg(feature = "sourcemap")]
pub mod sourcemap;
pub mod statement_expansion;
pub mod ts;

mod tester;

pub use tester::*;
