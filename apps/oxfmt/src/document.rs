//! Formats one document that is not read from disk, as `oxfmt --stdin-filepath` formats it, for a
//! host such as a language server.

use std::path::Path;

use ignore::gitignore::Gitignore;

use crate::core::{
    ConfigScopes, FormatResult, ResolveOutcome, SourceFormatter, build_global_ignore_matchers,
    host::HostFormatters, is_ignored, resolve_ignore_paths,
};

/// The configuration of every document below one directory, loaded once.
pub struct DocumentFormatter {
    scopes: ConfigScopes,
    /// `.prettierignore` below the directory.
    ignore: Vec<Gitignore>,
    formatter: SourceFormatter,
}

impl DocumentFormatter {
    /// Loads the configuration oxfmt would load when run in `cwd`, with nested configurations.
    ///
    /// # Errors
    /// The message oxfmt prints when the configuration or `.prettierignore` does not load.
    pub fn new(cwd: &Path, hosts: HostFormatters) -> Result<Self, String> {
        let scopes = ConfigScopes::load(
            cwd,
            None,
            true,
            hosts,
            #[cfg(feature = "napi")]
            None,
        )?;
        let ignore = resolve_ignore_paths(cwd, &[])
            .and_then(|paths| build_global_ignore_matchers(cwd, &[], &paths))?;
        // One allocator: a host formats on its own threads, and the pool makes more when they share it.
        Ok(Self { scopes, ignore, formatter: SourceFormatter::new(1) })
    }

    /// The text of the document at `path` formatted, or `None` when oxfmt would leave it as it is:
    /// an ignored path, a file type it does not format, or one a missing plugin formats.
    /// `.gitignore` does not count, as for `--stdin-filepath`: the document was asked for.
    ///
    /// # Errors
    /// The configuration of the file's directory does not load, or the text does not parse.
    pub fn format(&self, path: &Path, text: &str) -> Result<Option<String>, String> {
        if is_ignored(&self.ignore, path, false, true) {
            return Ok(None);
        }
        let resolver = self.scopes.resolve(path)?;
        if resolver.is_path_ignored(path, false) {
            return Ok(None);
        }
        let Some(strategy) = resolver.classify(path, None) else {
            return Ok(None);
        };
        let plan = match resolver.resolve(path, strategy)? {
            ResolveOutcome::Format(plan) => plan,
            ResolveOutcome::MissingPlugin(_) => return Ok(None),
        };
        match self.formatter.format(text, plan) {
            FormatResult::Success { code, .. } => Ok(Some(code)),
            FormatResult::Error(errors) => {
                Err(errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))
            }
        }
    }

    /// The names of the files whose change can change what [`Self::format`] does.
    pub fn configuration_files() -> Vec<&'static str> {
        let mut names = crate::core::config_discovery().config_file_names();
        names.extend([".editorconfig", ".prettierignore"]);
        names
    }
}
