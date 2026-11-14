//! tsc's error for an expression statement that is a lone identifier followed by a token on the
//! same line: usually a misspelled or unknown keyword.

use cow_utils::CowUtils;
use oxc_span::Span;

use crate::{ParserConfig as Config, ParserImpl, diagnostics, lexer::Kind};

/// tsc's keywords longer than two characters, in its order, which decides between suggestions
/// at the same distance.
const KEYWORDS: [&str; 77] = [
    "abstract",
    "accessor",
    "any",
    "asserts",
    "assert",
    "bigint",
    "boolean",
    "break",
    "case",
    "catch",
    "class",
    "continue",
    "const",
    "debugger",
    "declare",
    "default",
    "defer",
    "delete",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "implements",
    "import",
    "infer",
    "instanceof",
    "interface",
    "intrinsic",
    "keyof",
    "let",
    "module",
    "namespace",
    "never",
    "new",
    "null",
    "number",
    "object",
    "package",
    "private",
    "protected",
    "public",
    "override",
    "out",
    "readonly",
    "require",
    "global",
    "return",
    "satisfies",
    "set",
    "static",
    "string",
    "super",
    "switch",
    "symbol",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "unique",
    "unknown",
    "using",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "async",
    "await",
];

impl<'a, C: Config> ParserImpl<'a, C> {
    /// Reports `name`, a statement's whole expression at `span`, that a token on its line follows.
    /// The names tsc gives errors of their own keep the missing semicolon.
    #[cold]
    pub(crate) fn missing_semicolon_after_identifier(&mut self, span: Span, name: &'a str) {
        if matches!(
            name,
            "const"
                | "let"
                | "var"
                | "declare"
                | "interface"
                | "is"
                | "module"
                | "namespace"
                | "type"
        ) || self.cur_kind() == Kind::Undetermined
        {
            return;
        }
        let error = if let Some(keyword) = spelling_suggestion(name) {
            diagnostics::unknown_keyword_or_identifier(keyword, "", span)
        } else if let Some(keyword) = KEYWORDS.iter().find(|keyword| {
            name.encode_utf16().count() > keyword.len() + 2 && name.starts_with(**keyword)
        }) {
            diagnostics::unknown_keyword_or_identifier(keyword, &name[keyword.len()..], span)
        } else {
            diagnostics::unexpected_keyword_or_identifier(span)
        };
        self.set_fatal_error(error);
    }
}

/// tsc's `getSpellingSuggestion` over [`KEYWORDS`], whose names are all longer than the two
/// characters it compares without case.
#[expect(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn spelling_suggestion(name: &str) -> Option<&'static str> {
    let name: Vec<u16> = name.encode_utf16().collect();
    let length = name.len() as f64;
    let maximum_length_difference = 2.max((length * 0.34).floor() as usize);
    let mut best_distance = (length * 0.4).floor() + 1.0;
    let mut best = None;
    for keyword in KEYWORDS {
        let candidate: Vec<u16> = keyword.encode_utf16().collect();
        if candidate.len().abs_diff(name.len()) > maximum_length_difference || candidate == name {
            continue;
        }
        if let Some(distance) = levenshtein_with_max(&name, &candidate, best_distance - 0.1) {
            best_distance = distance;
            best = Some(keyword);
        }
    }
    best
}

/// JavaScript's `toLowerCase` of one UTF-16 unit.
fn lower(unit: u16) -> String {
    String::from_utf16_lossy(&[unit]).cow_to_lowercase().into_owned()
}

/// tsc's `levenshteinWithMax`: a substitution that only changes case costs 0.1, any other 2.
#[expect(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn levenshtein_with_max(s1: &[u16], s2: &[u16], max: f64) -> Option<f64> {
    let mut previous: Vec<f64> = (0..=s2.len()).map(|i| i as f64).collect();
    let mut current = vec![0.0; s2.len() + 1];
    let big = max + 0.01;
    for i in 1..=s1.len() {
        let c1 = s1[i - 1];
        let i_f = i as f64;
        let min_j = (if i_f > max { i_f - max } else { 1.0 }).ceil() as usize;
        let max_j = (if s2.len() as f64 > max + i_f { max + i_f } else { s2.len() as f64 }).floor()
            as usize;
        current[0] = i_f;
        let mut column_min = i_f;
        for value in current.iter_mut().take(min_j).skip(1) {
            *value = big;
        }
        for j in min_j..=max_j {
            let substitution = if lower(c1) == lower(s2[j - 1]) {
                previous[j - 1] + 0.1
            } else {
                previous[j - 1] + 2.0
            };
            let distance = if c1 == s2[j - 1] {
                previous[j - 1]
            } else {
                (previous[j] + 1.0).min(current[j - 1] + 1.0).min(substitution)
            };
            current[j] = distance;
            column_min = column_min.min(distance);
        }
        for value in current.iter_mut().skip(max_j + 1) {
            *value = big;
        }
        if column_min > max {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let result = previous[s2.len()];
    (result <= max).then_some(result)
}
