#[cfg(feature = "napi")]
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use oxc_diagnostics::OxcDiagnostic;
use oxc_linter::Oxlintrc;

#[cfg(feature = "napi")]
use crate::run::JsLoadJsConfigsCb;

/// Evaluates a JavaScript or TypeScript configuration file, so that a build without Node.js can
/// read one as the npm package does.
pub trait HostConfigLoader: Send + Sync {
    /// `path` is absolute. The result is the configuration the file exports: its default export,
    /// or a `vite.config`'s `lint`, with `null` when it has none.
    ///
    /// # Errors
    /// A message ready to print: oxlint reports the configuration as invalid.
    fn load(&self, path: &Path) -> Result<serde_json::Value, String>;
}

/// The loader of configuration files `loader` evaluates.
pub fn host_config_loader(loader: Arc<dyn HostConfigLoader>) -> JsConfigLoaderCb {
    Box::new(move |paths: Vec<String>| {
        let mut configs = Vec::with_capacity(paths.len());
        let mut errors = Vec::new();
        for path in paths {
            match loader.load(Path::new(&path)) {
                Ok(config) => match js_config_result(&path, config) {
                    Ok(config) => configs.push(config),
                    Err(error) => errors.push(error),
                },
                Err(error) => errors.push(OxcDiagnostic::error(format!(
                    "Failed to load config: {path}\n\n{error}"
                ))),
            }
        }
        if errors.is_empty() { Ok(configs) } else { Err(errors) }
    })
}

/// Callback type for loading JavaScript/TypeScript config files.
pub type JsConfigLoaderCb =
    Box<dyn Fn(Vec<String>) -> Result<Vec<JsConfigResult>, Vec<OxcDiagnostic>> + Send + Sync>;

/// Result of loading a single JavaScript/TypeScript config file.
/// `config` is `None` when the JS side signals "skip" (e.g., vite.config.ts without `.lint` field).
#[derive(Debug, Clone)]
pub struct JsConfigResult {
    pub path: PathBuf,
    pub config: Option<Oxlintrc>,
}

/// Response from JS side when loading JS configs.
#[cfg(feature = "napi")]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum LoadJsConfigsResponse {
    Success {
        #[serde(rename = "Success")]
        success: Vec<JsConfigResultJson>,
    },
    Failure {
        #[serde(rename = "Failures")]
        failures: Vec<LoadJsConfigsResponseFailure>,
    },
    Error {
        #[serde(rename = "Error")]
        error: String,
    },
}

#[cfg(feature = "napi")]
#[derive(Debug, Deserialize)]
struct LoadJsConfigsResponseFailure {
    path: String,
    error: String,
}

#[cfg(feature = "napi")]
#[derive(Debug, Deserialize)]
struct JsConfigResultJson {
    path: String,
    config: serde_json::Value,
}

/// Create a JS config loader callback from the JS callback.
///
/// The returned function blocks the current thread until the JS callback resolves.
/// It will panic if called outside of a Tokio runtime.
#[cfg(feature = "napi")]
pub fn create_js_config_loader(cb: JsLoadJsConfigsCb) -> JsConfigLoaderCb {
    Box::new(move |paths: Vec<String>| {
        let cb = &cb;
        let res = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { cb.call_async(paths).await?.into_future().await })
        });

        match res {
            Ok(json) => parse_js_config_response(&json),
            Err(err) => {
                Err(vec![OxcDiagnostic::error(format!("`loadJsConfigs` threw an error: {err}"))])
            }
        }
    })
}

fn parse_js_oxlintrc(mut value: serde_json::Value) -> Result<Oxlintrc, OxcDiagnostic> {
    let Some(map) = value.as_object_mut() else {
        return Err(OxcDiagnostic::error(
            "Configuration file must have a default export that is an object.",
        ));
    };

    let extends_value = map.remove("extends");
    let extends_configs = if let Some(extends_value) = extends_value {
        let serde_json::Value::Array(items) = extends_value else {
            return Err(OxcDiagnostic::error(
                "`extends` must be an array of config objects (strings/paths are not supported).",
            ));
        };

        items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                if !item.is_object() {
                    return Err(OxcDiagnostic::error(format!(
                        "`extends[{idx}]` must be a config object (strings/paths are not supported).",
                    )));
                }
                parse_js_oxlintrc(item)
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    let mut oxlintrc =
        Oxlintrc::from_json_value(&value).map_err(|err| OxcDiagnostic::error(err.to_string()))?;
    oxlintrc.extends_configs = extends_configs;
    Ok(oxlintrc)
}

/// Parse the JSON response from JS side into `JsConfigResult` structs.
#[cfg(feature = "napi")]
fn parse_js_config_response(json: &str) -> Result<Vec<JsConfigResult>, Vec<OxcDiagnostic>> {
    let response: LoadJsConfigsResponse = serde_json::from_str(json).map_err(|e| {
        vec![OxcDiagnostic::error(format!("Failed to parse JS config response: {e}"))]
    })?;

    match response {
        LoadJsConfigsResponse::Success { success } => {
            let mut configs = Vec::with_capacity(success.len());
            let mut errors = Vec::new();
            for entry in success {
                match js_config_result(&entry.path, entry.config) {
                    Ok(config) => configs.push(config),
                    Err(error) => errors.push(error),
                }
            }

            if errors.is_empty() { Ok(configs) } else { Err(errors) }
        }
        LoadJsConfigsResponse::Failure { failures } => Err(failures
            .into_iter()
            .map(|failure| {
                OxcDiagnostic::error(format!(
                    "Failed to load config: {}\n\n{}",
                    failure.path, failure.error
                ))
            })
            .collect()),
        LoadJsConfigsResponse::Error { error } => {
            Err(vec![OxcDiagnostic::error(format!("Failed to load config files:\n\n{error}"))])
        }
    }
}

/// The configuration `config` holds, which the file at `path` exports; `null` skips the file (a
/// `vite.config` without `lint`).
fn js_config_result(
    path: &str,
    config: serde_json::Value,
) -> Result<JsConfigResult, OxcDiagnostic> {
    let path_buf = PathBuf::from(path);
    if config.is_null() {
        return Ok(JsConfigResult { path: path_buf, config: None });
    }
    let mut oxlintrc = parse_js_oxlintrc(config).map_err(|err| {
        OxcDiagnostic::error(format!("Failed to parse config from {path}"))
            .with_note(err.to_string())
    })?;
    oxlintrc.path.clone_from(&path_buf);
    let Some(config_dir) = oxlintrc.path.parent().map(Path::to_path_buf) else {
        return Err(OxcDiagnostic::error(format!("Config path has no parent directory: {path}")));
    };
    oxlintrc.set_config_dir(&config_dir);
    Ok(JsConfigResult { path: path_buf, config: Some(oxlintrc) })
}
