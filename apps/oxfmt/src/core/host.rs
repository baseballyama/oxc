//! Formatters an embedding program supplies for the languages oxfmt hands to Prettier, and the
//! Tailwind CSS sorter it hands to JavaScript, so that a native build can do without Node.js or
//! choose how to reach it.

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
    /// Prettier hands them back to it: `sort_imports` and `jsdoc` for a whole file, neither for
    /// a part of one (a Markdown code block), whose scripts Prettier formats itself.
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

/// Sorts Tailwind CSS classes as prettier-plugin-tailwindcss's `sortClassAttributes` does.
pub trait HostTailwindSorter: Send + Sync {
    /// `options` are the Prettier options oxfmt would give the plugin for the file, with
    /// `filepath` and the plugin's payload. The result has one sorted class list per given one.
    ///
    /// # Errors
    /// A message ready to print: oxfmt reports the file as not formatted.
    fn sort(&self, options: &Value, classes: Vec<String>) -> Result<Vec<String>, String>;
}

/// The host formatters of one run, by the language each serves, and its Tailwind CSS sorter.
/// Cloning is shallow.
#[derive(Clone, Default)]
pub struct HostFormatters {
    formatters: Arc<FxHashMap<PrettierLanguage, Arc<dyn HostFormatter>>>,
    tailwind: Option<Arc<dyn HostTailwindSorter>>,
}

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
        Arc::make_mut(&mut self.formatters).insert(language, Arc::new(formatter));
        self
    }

    /// Sort Tailwind CSS classes with `sorter` where the configuration asks for it.
    #[must_use]
    pub fn with_tailwind_sorter(mut self, sorter: impl HostTailwindSorter + 'static) -> Self {
        self.tailwind = Some(Arc::new(sorter));
        self
    }

    pub(super) fn tailwind_sorter(&self) -> Option<&Arc<dyn HostTailwindSorter>> {
        self.tailwind.as_ref()
    }

    pub(super) fn get(&self, language: PrettierLanguage) -> Option<HostedLanguage> {
        let formatter = Arc::clone(self.formatters.get(&language)?);
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
        f.debug_set().entries(self.formatters.keys()).finish()
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
        let mut options = prettier_options(config, path);
        inject_parser(&mut options, self.language.parser());
        inject_opt_in_plugin_payloads(&mut options, self.language, config);
        self.formatter.format(source_text, &options, javascript)
    }
}

/// The Prettier options of the file at `path`, with its `filepath` and the Tailwind CSS plugin's
/// payload: what a host gets for any language.
pub(super) fn prettier_options(config: &FormatConfig, path: &Path) -> Value {
    let mut options = to_prettier(config);
    inject_filepath(&mut options, path);
    inject_tailwind_plugin_payload(&mut options, config);
    options
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

    /// Prints the text it receives with its spaces collapsed, then whether it would sort imports
    /// or format JSDoc.
    #[cfg(not(feature = "napi"))]
    struct Collapse;

    #[cfg(not(feature = "napi"))]
    impl HostFormatter for Collapse {
        fn format(
            &self,
            source_text: &str,
            _options: &Value,
            javascript: &JsFormatOptions,
        ) -> Result<String, String> {
            let words: Vec<&str> = source_text.split_whitespace().collect();
            let (sorts, jsdoc) = (javascript.sort_imports.is_some(), javascript.jsdoc.is_some());
            Ok(format!("{}\n(hosted, {sorts}, {jsdoc})\n", words.join(" ")))
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
        let hosted = "# A\n\n- item\n\n  ```svelte\n  <p class=\"x\">a</p>\n  (hosted, false, false)\n  ```\n";
        for on in ["{}", r#"{ "sortImports": {}, "jsdoc": {} }"#] {
            assert_eq!(formatted(&scopes_with(on), &path, markdown), hosted, "{on}");
        }
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

    /// Sorts each class list backwards, or fails when told to.
    #[cfg(not(feature = "napi"))]
    struct Backwards(bool);

    #[cfg(not(feature = "napi"))]
    impl HostTailwindSorter for Backwards {
        fn sort(&self, options: &Value, classes: Vec<String>) -> Result<Vec<String>, String> {
            if self.0 {
                return Err(format!("no sorter for {}", options["filepath"]));
            }
            assert_eq!(options["_useTailwindPlugin"], json!(1));
            Ok(classes
                .iter()
                .map(|list| list.split_whitespace().rev().collect::<Vec<_>>().join(" "))
                .collect())
        }
    }

    #[cfg(not(feature = "napi"))]
    fn result(scopes: &ConfigScopes, path: &Path, source_text: &str) -> FormatResult {
        let resolver = scopes.root();
        let strategy = resolver.classify(path, None).unwrap();
        let ResolveOutcome::Format(plan) = resolver.resolve(path, strategy).unwrap() else {
            panic!("the file formats");
        };
        SourceFormatter::new(1).format(source_text, plan)
    }

    #[cfg(not(feature = "napi"))]
    #[test]
    fn a_host_sorts_tailwind_classes_and_its_failure_fails_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.tsx");
        let source = "<div className=\"a  b\" />;\n";
        let on = r#"{ "sortTailwindcss": {} }"#;
        let sorting = |fails| HostFormatters::default().with_tailwind_sorter(Backwards(fails));
        let FormatResult::Success { code, .. } =
            result(&scopes(dir.path(), on, sorting(false)), &path, source)
        else {
            panic!("the file formats");
        };
        assert_eq!(code, "<div className=\"b a\" />;\n");
        // Without a sorter, the classes are not collected, whose whitespace collecting normalizes
        let FormatResult::Success { code, .. } =
            result(&scopes(dir.path(), on, HostFormatters::default()), &path, source)
        else {
            panic!("the file formats");
        };
        assert_eq!(code, source);
        let FormatResult::Error(errors) =
            result(&scopes(dir.path(), on, sorting(true)), &path, source)
        else {
            panic!("a failed sort fails the file");
        };
        assert!(errors[0].to_string().contains("no sorter for"), "{}", errors[0]);
        let FormatResult::Success { code, .. } =
            result(&scopes(dir.path(), "{}", sorting(true)), &path, source)
        else {
            panic!("an unconfigured sort is not asked for");
        };
        assert_eq!(code, source);
    }

    #[test]
    #[should_panic(expected = "formats Vue itself")]
    fn a_native_language_takes_no_host() {
        let _ = HostFormatters::default().with(Language::Vue, Echo);
    }
}
