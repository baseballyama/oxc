//! tsc's error for an expression statement that is a lone identifier followed by a token on the
//! same line: usually a misspelled or unknown keyword.

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

/// The longest of [`KEYWORDS`], in UTF-16 units.
const LONGEST_KEYWORD: usize = 10;

/// tsc's `getSpellingSuggestion` over [`KEYWORDS`], whose names are all longer than the two
/// characters it compares without case.
#[expect(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn spelling_suggestion(name: &str) -> Option<&'static str> {
    // A name this long is further from every keyword than tsc's length limit.
    let mut units = [0u16; 2 * LONGEST_KEYWORD];
    let mut length = 0;
    for unit in name.encode_utf16() {
        if length == units.len() {
            return None;
        }
        units[length] = unit;
        length += 1;
    }
    let name = &units[..length];
    let maximum_length_difference = 2.max((length as f64 * 0.34).floor() as usize);
    let mut best_distance = (length as f64 * 0.4).floor() + 1.0;
    let mut best = None;
    for keyword in KEYWORDS {
        // Every keyword is ASCII, so its bytes are its UTF-16 units.
        let candidate = keyword.as_bytes();
        if candidate.len().abs_diff(length) > maximum_length_difference
            || candidate.iter().map(|&byte| u16::from(byte)).eq(name.iter().copied())
        {
            continue;
        }
        if let Some(distance) = levenshtein_with_max(name, candidate, best_distance - 0.1) {
            best_distance = distance;
            best = Some(keyword);
        }
    }
    best
}

/// Whether JavaScript's `toLowerCase` gives one UTF-16 unit and one ASCII byte the same string.
fn same_lowercase(unit: u16, byte: u8) -> bool {
    // A lone surrogate lowercases to itself, which is no ASCII byte.
    char::from_u32(u32::from(unit))
        .is_some_and(|c| c.to_lowercase().eq(char::from(byte).to_lowercase()))
}

/// tsc's `levenshteinWithMax`: a substitution that only changes case costs 0.1, any other 2.
#[expect(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn levenshtein_with_max(s1: &[u16], s2: &[u8], max: f64) -> Option<f64> {
    let mut previous = [0.0; LONGEST_KEYWORD + 1];
    let mut current = [0.0; LONGEST_KEYWORD + 1];
    for (j, value) in previous.iter_mut().enumerate().take(s2.len() + 1) {
        *value = j as f64;
    }
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
            let c2 = s2[j - 1];
            let substitution =
                if same_lowercase(c1, c2) { previous[j - 1] + 0.1 } else { previous[j - 1] + 2.0 };
            let distance = if c1 == u16::from(c2) {
                previous[j - 1]
            } else {
                (previous[j] + 1.0).min(current[j - 1] + 1.0).min(substitution)
            };
            current[j] = distance;
            column_min = column_min.min(distance);
        }
        for value in current.iter_mut().take(s2.len() + 1).skip(max_j + 1) {
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

#[cfg(test)]
mod test {
    use oxc_allocator::Allocator;
    use oxc_span::SourceType;

    use crate::Parser;

    #[test]
    fn suggestions_compare_case_as_javascript_does() {
        // tsc 6.0.3's message for `<name> x;`. `İ` lowercases to two units and the Kelvin sign
        // to `k`, which decides each suggestion at the length limit.
        let cases = [
            ("\u{212A}eyofs", Some("keyof")),
            ("İnfers", None),
            ("ınfers", None),
            ("Infers", Some("infer")),
            ("İnfer", Some("infer")),
            ("İnfe", None),
            ("Get", Some("get")),
            ("GEt", Some("get")),
            ("enuM", Some("enum")),
            ("abcdefghijklmnopqrstu", None),
        ];
        for (name, suggestion) in cases {
            let allocator = Allocator::default();
            let source = format!("{name} x;");
            let ret = Parser::new(&allocator, &source, SourceType::ts()).parse();
            let message = ret.diagnostics.first().map(|error| error.message.to_string());
            let expected = suggestion.map_or_else(
                || "Unexpected keyword or identifier.".to_string(),
                |keyword| format!("Unknown keyword or identifier. Did you mean '{keyword}'?"),
            );
            assert_eq!(message.as_deref(), Some(expected.as_str()), "{name}");
        }
    }
}
