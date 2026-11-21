#[cfg(feature = "napi")]
mod api;
pub mod cli;
mod core;
#[cfg(feature = "napi")]
mod prettier_compat;

pub use core::{
    Language,
    host::{HostFormatter, HostFormatters},
    oxfmtrc,
};
#[cfg(feature = "napi")]
pub mod lsp;
#[cfg(feature = "napi")]
mod main_napi;
#[cfg(feature = "napi")]
pub use main_napi::*;

#[cfg(all(
    feature = "allocator",
    not(any(
        target_arch = "arm",
        target_arch = "riscv64",
        miri,
        target_os = "freebsd",
        target_family = "wasm"
    ))
))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

/// What an embedder needs to format files it already parsed, while oxfmt still walks, writes and
/// reports.
pub mod share {
    pub use crate::cli::Preformatted;
    pub use crate::core::{FormatPlan, FormatResult, SourceFormatter};
}
