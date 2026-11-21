//! Hosts: programs that embed oxlint and lint the files of a language themselves.
//!
//! A host registers a [`HostPlugin`] with the [`ExternalPluginStore`](crate::ExternalPluginStore)
//! before the configuration is read. Its rules are then configured like any other rule, by
//! `rules`, `categories` and `overrides`, under the plugin's name, and need no `plugins` entry.
//! For each file with one of the plugin's extensions, oxlint asks the host which scripts its own
//! rules lint, and the host lints the whole file with the rules the configuration turns on for it.

use std::{fmt, path::Path, sync::Arc};

use serde_json::Value;

use crate::{AllowWarnDeny, Message, RuleCategory, loader::JavaScriptSource};

/// A rule a host implements under its plugin's name.
#[derive(Debug, Clone)]
pub struct HostRule {
    pub name: String,
    pub category: RuleCategory,
}

/// A rule the configuration turns on for the file a host lints.
#[derive(Debug, Clone, Copy)]
pub struct EnabledHostRule<'c> {
    /// The plugin's name for a host rule, or oxlint's plugin name for a built-in rule the host
    /// implements (`eslint` for `no-unused-vars`).
    pub plugin: &'c str,
    pub name: &'c str,
    pub severity: AllowWarnDeny,
    /// The rule's options from the configuration; empty for a built-in rule.
    pub options: &'c [Value],
}

pub trait HostLinter: Send + Sync {
    /// The scripts of `source_text` that oxlint's own rules lint, with their offsets in it.
    /// `None` keeps oxlint's partial loader.
    fn scripts<'a>(&self, path: &Path, source_text: &'a str) -> Option<Vec<JavaScriptSource<'a>>> {
        let _ = (path, source_text);
        None
    }

    /// Read-only globals the language declares in `path`, beside the configuration's, for the
    /// files whose scripts oxlint lints, such as the runes of a Svelte module.
    fn globals(&self, path: &Path) -> &[&str] {
        let _ = path;
        &[]
    }

    /// Lints `file` with its rules. A message's severity is its rule's.
    fn lint(&self, file: &HostFile<'_>) -> Vec<Message>;
}

/// A file a host lints, with what the configuration says for it.
#[derive(Debug)]
pub struct HostFile<'c> {
    pub path: &'c Path,
    pub source_text: &'c str,
    pub rules: &'c [EnabledHostRule<'c>],
    /// `settings`, as written: a host reads its own key, as a JS plugin does.
    pub settings: Option<&'c serde_json::Map<String, Value>>,
    /// The environments `env` turns on.
    pub env: Vec<&'c str>,
    /// `globals`: a name with `Some(writable)`, or `None` for `"off"`.
    pub globals: Vec<(&'c str, Option<bool>)>,
}

/// A language a host lints.
pub struct HostPlugin {
    /// The prefix of its rules in the configuration, as in `svelte/no-at-html-tags`.
    pub name: String,
    /// The extensions of the files it lints, without the dot.
    pub extensions: Vec<String>,
    /// Extensions of other files its own rules also run on, such as a module's, where oxlint
    /// keeps its scripts and built-in rules.
    pub module_extensions: Vec<String>,
    pub rules: Vec<HostRule>,
    /// Built-in rules, by oxlint's plugin and rule name, that the host runs in its files instead
    /// of oxlint, because they need what only the host sees (a template's references).
    pub builtin_rules: Vec<(String, String)>,
    pub linter: Arc<dyn HostLinter>,
}

impl HostPlugin {
    pub(crate) fn lints(&self, path: &Path) -> bool {
        Self::has_extension(path, &self.extensions)
    }

    pub(crate) fn reads(&self, path: &Path) -> bool {
        self.lints(path) || Self::has_extension(path, &self.module_extensions)
    }

    fn has_extension(path: &Path, extensions: &[String]) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.iter().any(|e| e == ext))
    }

    pub(crate) fn implements(&self, plugin: &str, rule: &str) -> bool {
        self.builtin_rules.iter().any(|(p, r)| p == plugin && r == rule)
    }
}

impl fmt::Debug for HostPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostPlugin")
            .field("name", &self.name)
            .field("extensions", &self.extensions)
            .field("module_extensions", &self.module_extensions)
            .field("rules", &self.rules.len())
            .field("builtin_rules", &self.builtin_rules)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{Arc, Mutex},
    };

    use rustc_hash::FxHashMap;

    use super::{HostFile, HostLinter, HostPlugin, HostRule};
    use crate::{
        AllowWarnDeny, ConfigStore, ConfigStoreBuilder, ExternalPluginStore, LintOptions, Linter,
        Message, Oxlintrc, RuleCategory,
    };

    /// Records the rules it is asked to run.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, AllowWarnDeny, String)>>);

    impl HostLinter for Recorder {
        fn lint(&self, file: &HostFile<'_>) -> Vec<Message> {
            let mut seen = self.0.lock().unwrap();
            for rule in file.rules {
                let options = serde_json::to_string(rule.options).unwrap();
                seen.push((format!("{}/{}", rule.plugin, rule.name), rule.severity, options));
            }
            Vec::new()
        }
    }

    fn rules_for(config: &str, path: &str) -> Vec<(String, AllowWarnDeny, String)> {
        let recorder = Arc::new(Recorder::default());
        let host = Arc::new(HostPlugin {
            name: "svelte".into(),
            extensions: vec!["svelte".into()],
            module_extensions: vec!["ts".into()],
            rules: vec![
                HostRule { name: "valid-compile".into(), category: RuleCategory::Correctness },
                HostRule { name: "no-at-html-tags".into(), category: RuleCategory::Restriction },
            ],
            builtin_rules: vec![("eslint".into(), "no-unused-vars".into())],
            linter: Arc::clone(&recorder) as Arc<dyn HostLinter>,
        });
        let mut store = ExternalPluginStore::new(false);
        store.register_host_plugin(host);
        let oxlintrc = Oxlintrc::from_string(config).unwrap();
        let builder =
            ConfigStoreBuilder::from_oxlintrc(false, oxlintrc, None, &mut store, None).unwrap();
        let config = builder.build(&mut store).unwrap();
        let linter = Linter::new(
            LintOptions::default(),
            ConfigStore::new(config, FxHashMap::default(), store),
            None,
        );
        let ran = linter.run_host(Path::new(path), "").is_some();
        let mut seen = std::mem::take(&mut *recorder.0.lock().unwrap());
        seen.sort_by(|a, b| a.0.cmp(&b.0));
        if !ran {
            assert_eq!(seen, []);
        }
        seen
    }

    #[test]
    fn a_host_rule_runs_by_its_category() {
        let seen = rules_for("{}", "App.svelte");
        assert_eq!(
            seen,
            [
                ("eslint/no-unused-vars".into(), AllowWarnDeny::Warn, "[]".into()),
                ("svelte/valid-compile".into(), AllowWarnDeny::Warn, "[]".into()),
            ]
        );
    }

    #[test]
    fn rules_and_overrides_configure_a_host_rule() {
        let config = r#"{
            "categories": { "restriction": "error" },
            "rules": { "svelte/valid-compile": "off", "no-unused-vars": "off" },
            "overrides": [{
                "files": ["*.svelte"],
                "rules": { "svelte/no-at-html-tags": ["warn", { "allow": true }] }
            }]
        }"#;
        assert_eq!(
            rules_for(config, "App.svelte"),
            [("svelte/no-at-html-tags".into(), AllowWarnDeny::Warn, r#"[{"allow":true}]"#.into())]
        );
    }

    #[test]
    fn a_host_runs_only_its_own_rules_on_a_module() {
        assert_eq!(
            rules_for("{}", "module.ts"),
            [("svelte/valid-compile".into(), AllowWarnDeny::Warn, "[]".into())]
        );
        assert_eq!(rules_for("{}", "module.js"), []);
    }

    /// Records what the configuration says for the file.
    #[derive(Default)]
    struct Context(Mutex<Option<String>>);

    impl HostLinter for Context {
        fn lint(&self, file: &HostFile<'_>) -> Vec<Message> {
            let mut env = file.env.clone();
            env.sort_unstable();
            let mut globals = file.globals.clone();
            globals.sort_unstable();
            let settings = file.settings.and_then(|s| s.get("svelte")).cloned();
            *self.0.lock().unwrap() = Some(format!("{settings:?} {env:?} {globals:?}"));
            Vec::new()
        }
    }

    #[test]
    fn a_host_reads_settings_env_and_globals() {
        let context = Arc::new(Context::default());
        let host = Arc::new(HostPlugin {
            name: "svelte".into(),
            extensions: vec!["svelte".into()],
            module_extensions: Vec::new(),
            rules: vec![HostRule { name: "a".into(), category: RuleCategory::Correctness }],
            builtin_rules: Vec::new(),
            linter: Arc::clone(&context) as Arc<dyn HostLinter>,
        });
        let mut store = ExternalPluginStore::new(false);
        store.register_host_plugin(host);
        let oxlintrc = Oxlintrc::from_string(
            r#"{
                "settings": { "svelte": { "ignoreWarnings": ["x"] } },
                "env": { "browser": true },
                "globals": { "one": "readonly", "two": "writable", "three": "off" }
            }"#,
        )
        .unwrap();
        let builder =
            ConfigStoreBuilder::from_oxlintrc(false, oxlintrc, None, &mut store, None).unwrap();
        let config = builder.build(&mut store).unwrap();
        let linter = Linter::new(
            LintOptions::default(),
            ConfigStore::new(config, FxHashMap::default(), store),
            None,
        );
        linter.run_host(Path::new("App.svelte"), "").unwrap();
        assert_eq!(
            context.0.lock().unwrap().as_deref(),
            Some(
                r#"Some(Object {"ignoreWarnings": Array [String("x")]}) ["browser"] [("one", Some(false)), ("three", None), ("two", Some(true))]"#
            )
        );
    }

    #[test]
    fn an_unknown_host_rule_is_an_error() {
        let host = Arc::new(HostPlugin {
            name: "svelte".into(),
            extensions: vec!["svelte".into()],
            module_extensions: Vec::new(),
            rules: Vec::new(),
            builtin_rules: Vec::new(),
            linter: Arc::new(Recorder::default()),
        });
        let mut store = ExternalPluginStore::new(false);
        store.register_host_plugin(host);
        let oxlintrc = Oxlintrc::from_string(r#"{ "rules": { "svelte/typo": "error" } }"#).unwrap();
        let error = ConfigStoreBuilder::from_oxlintrc(false, oxlintrc, None, &mut store, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("typo"), "{error}");
    }
}
