//! Formatters an embedding program supplies for the languages oxfmt hands to Prettier,
//! so that a native build can format them without Node.js.

use std::{fmt, path::Path, sync::Arc};

use oxc_formatter::JsFormatOptions;
use rustc_hash::FxHashMap;
use serde_json::Value;

use super::{
    language::Language,
    options::{
        inject_filepath, inject_opt_in_plugin_payloads, inject_parser,
        inject_tailwind_plugin_payload, to_prettier,
    },
    oxfmtrc::FormatConfig,
    support::PrettierLanguage,
};

/// Formats a whole file of one language.
pub trait HostFormatter: Send + Sync {
    /// `options` are the Prettier options oxfmt would pass to Prettier for this text,
    /// with `parser`, `filepath` and the payloads of the Tailwind CSS and the language's opt-in
    /// plugin. `javascript` are the options oxfmt formats the language's scripts with, when
    /// Prettier hands them back to it, `sort_imports` and `jsdoc` included.
    ///
    /// # Errors
    /// A message ready to print.
    fn format(
        &self,
        source_text: &str,
        options: &Value,
        javascript: &JsFormatOptions,
    ) -> Result<String, String>;
}

/// The host formatters of one run, by the language each serves. Cloning is shallow.
#[derive(Clone, Default)]
pub struct HostFormatters(Arc<FxHashMap<PrettierLanguage, Arc<dyn HostFormatter>>>);

impl HostFormatters {
    /// Serve `language` with `formatter` instead of Prettier.
    ///
    /// # Panics
    /// Panics if `language` has a Rust formatter in oxfmt, which always serves it.
    #[must_use]
    pub fn with(mut self, language: Language, formatter: impl HostFormatter + 'static) -> Self {
        let Some(language) = language.prettier_language() else {
            panic!("oxfmt formats {language:?} itself");
        };
        Arc::make_mut(&mut self.0).insert(language, Arc::new(formatter));
        self
    }

    pub(super) fn get(&self, language: PrettierLanguage) -> Option<HostedLanguage> {
        let formatter = Arc::clone(self.0.get(&language)?);
        Some(HostedLanguage { language, formatter })
    }

    /// The host of `language`, unless `config` turns the language off.
    pub(super) fn serving(
        &self,
        language: PrettierLanguage,
        config: &FormatConfig,
    ) -> Option<HostedLanguage> {
        self.get(language).filter(|hosted| !hosted.language.disabled_by(config))
    }
}

impl fmt::Debug for HostFormatters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.0.keys()).finish()
    }
}

/// A file a host formatter serves.
#[derive(Clone)]
pub struct HostedLanguage {
    pub(super) language: PrettierLanguage,
    pub(super) formatter: Arc<dyn HostFormatter>,
}

impl HostedLanguage {
    /// Formats `source_text`, a whole file at `path` or a part of it, as `config` says.
    pub(super) fn format(
        &self,
        source_text: &str,
        path: &Path,
        config: &FormatConfig,
        javascript: &JsFormatOptions,
    ) -> Result<String, String> {
        let mut options = to_prettier(config);
        inject_parser(&mut options, self.language.parser());
        inject_filepath(&mut options, path);
        inject_tailwind_plugin_payload(&mut options, config);
        inject_opt_in_plugin_payloads(&mut options, self.language, config);
        self.formatter.format(source_text, &options, javascript)
    }
}

impl fmt::Debug for HostedLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HostedLanguage").field(&self.language).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;
    use crate::core::{
        ConfigScopes, FormatResult, ResolveOutcome, SourceFormatter, support::FormatStrategy,
    };

    /// Echoes the options it receives, so a test reads what oxfmt hands a host.
    struct Echo;

    impl HostFormatter for Echo {
        fn format(
            &self,
            _source_text: &str,
            options: &Value,
            javascript: &JsFormatOptions,
        ) -> Result<String, String> {
            let mut options = options.clone();
            options["sortsImports"] = json!(javascript.sort_imports.is_some());
            Ok(format!("{options}\n"))
        }
    }

    /// Prints the text it receives with its spaces collapsed, on two lines.
    #[cfg(not(feature = "napi"))]
    struct Collapse;

    #[cfg(not(feature = "napi"))]
    impl HostFormatter for Collapse {
        fn format(
            &self,
            source_text: &str,
            _options: &Value,
            _javascript: &JsFormatOptions,
        ) -> Result<String, String> {
            let words: Vec<&str> = source_text.split_whitespace().collect();
            Ok(format!("{}\n(hosted)\n", words.join(" ")))
        }
    }

    #[cfg(not(feature = "napi"))]
    fn formatted(scopes: &ConfigScopes, path: &Path, source_text: &str) -> String {
        let resolver = scopes.root();
        let strategy = resolver.classify(path, None).unwrap();
        let ResolveOutcome::Format(plan) = resolver.resolve(path, strategy).unwrap() else {
            panic!("the file formats");
        };
        let FormatResult::Success { code, .. } = SourceFormatter::new(1).format(source_text, plan)
        else {
            panic!("the file formats");
        };
        code
    }

    fn scopes(dir: &Path, config: &str, hosts: HostFormatters) -> ConfigScopes {
        std::fs::write(dir.join(".oxfmtrc.json"), config).unwrap();
        ConfigScopes::load(
            dir,
            None,
            false,
            hosts,
            #[cfg(feature = "napi")]
            None,
        )
        .unwrap()
    }

    fn options(scopes: &ConfigScopes, path: &Path) -> Option<Value> {
        let resolver = scopes.root();
        let strategy = resolver.classify(path, None)?;
        let ResolveOutcome::Format(plan) = resolver.resolve(path, strategy).unwrap() else {
            return None;
        };
        let FormatResult::Success { code, .. } = SourceFormatter::new(1).format("<p />", plan)
        else {
            panic!("the host formats");
        };
        Some(serde_json::from_str(&code).unwrap())
    }

    #[test]
    fn a_host_formats_a_language_prettier_would_with_prettier_options() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = HostFormatters::default().with(Language::Svelte, Echo);
        let scopes = scopes(
            dir.path(),
            r#"{ "semi": false, "svelte": { "sortOrder": "scripts-markup-styles-options" } }"#,
            hosts,
        );
        let path = dir.path().join("App.svelte");
        let options = options(&scopes, &path).expect("the host serves `.svelte`");
        assert_eq!(options["parser"], json!("svelte"));
        assert_eq!(options["printWidth"], json!(100));
        assert_eq!(options["semi"], json!(false));
        assert_eq!(options["svelteSortOrder"], json!("scripts-markup-styles-options"));
        assert_eq!(options["filepath"], json!(path.to_string_lossy()));
        assert_eq!(options["sortsImports"], json!(false));
    }

    #[test]
    fn a_host_gets_the_javascript_options_oxfmt_formats_its_scripts_with() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = HostFormatters::default().with(Language::Svelte, Echo);
        let scopes = scopes(dir.path(), r#"{ "sortImports": {} }"#, hosts);
        let options = options(&scopes, &dir.path().join("App.svelte")).unwrap();
        assert_eq!(options["sortsImports"], json!(true));
    }

    // A napi build formats Markdown with the JS side's services, which a unit test has not.
    #[cfg(not(feature = "napi"))]
    #[test]
    fn a_host_formats_a_code_block_in_its_language() {
        let markdown = "# A\n\n- item\n\n  ```svelte\n  <p   class=\"x\">a</p>\n  ```\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let hosts = || HostFormatters::default().with(Language::Svelte, Collapse);
        let scopes_with = |config| scopes(dir.path(), config, hosts());
        assert_eq!(
            formatted(&scopes_with("{}"), &path, markdown),
            "# A\n\n- item\n\n  ```svelte\n  <p class=\"x\">a</p>\n  (hosted)\n  ```\n"
        );
        for off in [r#"{ "svelte": false }"#, r#"{ "embeddedLanguageFormatting": "off" }"#] {
            assert_eq!(formatted(&scopes_with(off), &path, markdown), markdown, "{off}");
        }
        let unhosted = scopes(dir.path(), "{}", HostFormatters::default());
        assert_eq!(formatted(&unhosted, &path, markdown), markdown);
    }

    #[test]
    fn a_language_without_a_host_is_not_hosted() {
        let dir = tempfile::tempdir().unwrap();
        let scopes = scopes(dir.path(), "{}", HostFormatters::default());
        let strategy = scopes.root().classify(&dir.path().join("App.svelte"), None);
        assert!(!matches!(strategy, Some(FormatStrategy::Host(_))));
    }

    #[test]
    fn a_config_turns_a_hosted_language_off() {
        let dir = tempfile::tempdir().unwrap();
        let hosts = HostFormatters::default().with(Language::Svelte, Echo);
        let scopes = scopes(dir.path(), r#"{ "svelte": false }"#, hosts);
        assert_eq!(options(&scopes, &dir.path().join("App.svelte")), None);
    }

    #[test]
    #[should_panic(expected = "formats Vue itself")]
    fn a_native_language_takes_no_host() {
        let _ = HostFormatters::default().with(Language::Vue, Echo);
    }
}
