use oxc_diagnostics::{
    Error, GraphicalReportHandler,
    reporter::{DiagnosticReporter, DiagnosticResult},
};

// This reporter is used with stderr and displays diagnostics only in a graphical way.
// For stdout, we display them manually in `format.rs`.

#[derive(Debug)]
pub struct DefaultReporter {
    handler: GraphicalReportHandler,
    diagnostics: Vec<Error>,
}

impl Default for DefaultReporter {
    fn default() -> Self {
        Self { handler: GraphicalReportHandler::new(), diagnostics: Vec::new() }
    }
}

impl DiagnosticReporter for DefaultReporter {
    fn render_error(&mut self, error: Error) -> Option<String> {
        // Collect diagnostics for rendering in finish() at once
        self.diagnostics.push(error);
        None
    }

    fn finish(&mut self, _result: &DiagnosticResult) -> Option<String> {
        let mut output = String::new();

        // Files finish in parallel, so order by file to print the same report on every run.
        // The sort is stable, which keeps each file's own diagnostics in their order.
        self.diagnostics.sort_by_cached_key(|diagnostic| {
            diagnostic.source_code().and_then(|source| source.name()).map(str::to_owned)
        });

        // Render all diagnostics (errors only, no warnings)
        for diagnostic in &self.diagnostics {
            self.handler.render_report(&mut output, diagnostic.as_ref()).unwrap();
        }

        Some(output)
    }
}

#[cfg(test)]
mod test {
    use oxc_diagnostics::{
        DiagnosticService, OxcDiagnostic,
        reporter::{DiagnosticReporter, DiagnosticResult},
    };

    use super::DefaultReporter;

    #[test]
    fn orders_diagnostics_by_file_and_keeps_each_files_order() {
        let mut reporter = DefaultReporter::default();
        let errors = |path: &str, messages: &[&str]| {
            let diagnostics =
                messages.iter().map(|message| OxcDiagnostic::error(message.to_string())).collect();
            DiagnosticService::wrap_diagnostics("/", path, "", diagnostics)
        };
        for error in errors("/b.js", &["b second", "b first"])
            .into_iter()
            .chain(errors("/a.js", &["a only"]))
        {
            reporter.render_error(error);
        }
        let output = reporter.finish(&DiagnosticResult::default()).unwrap();
        let at = |needle: &str| output.find(needle).unwrap();
        assert!(at("a only") < at("b second"), "{output}");
        assert!(at("b second") < at("b first"), "{output}");
    }
}
