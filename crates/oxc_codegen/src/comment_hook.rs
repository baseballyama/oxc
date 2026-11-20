//! Comments placed by the host instead of by [`Codegen`], see [`Codegen::with_comment_hook`].
//!
//! The printer reports where it is: before and after a node, at the start of each statement list,
//! after each statement or list item, and at the end of each list. The host decides which of its
//! comments to write there. A node whose span is [`SPAN`](oxc_span::SPAN) has no position in the
//! source, so the printer reports `None` for it. So that a program pays only where it has
//! comments, the printer asks [`CommentHook::next_comment`] where the next one starts, and reports
//! a node only when that is before the node.

use oxc_span::Span;

use crate::Codegen;

/// Where the printer is, reported to the host that places comments.
///
/// Positions are offsets in the source that the nodes' spans index; `None` is a node without one.
pub trait CommentHook {
    /// Before the node at `span` prints, when [`CommentHook::next_comment`] is before it.
    /// `expression` is `true` for an expression. A node that wraps another of the same span, such
    /// as an `Expression` around a call, may report both, and each `before` has its own
    /// [`CommentHook::after`].
    fn before(&mut self, out: &mut CommentWriter<'_, '_>, span: Option<Span>, expression: bool);

    /// After the node at `span` printed, for each [`CommentHook::before`].
    fn after(&mut self, out: &mut CommentWriter<'_, '_>, span: Option<Span>);

    /// A list of statements or class members is about to print; `owner` is the program, block,
    /// function body, class body or static block that holds it.
    fn body(&mut self, owner: Option<Span>);

    /// After a statement, a class member or a list item printed with its separator. `end` is
    /// where it ends, `next` where the printer goes next: the next item's start in a list, the
    /// next statement's end in a statement list, or the end of the list. Reported only when the
    /// item has a position and the next comment starts before `next`.
    fn trailing(&mut self, out: &mut CommentWriter<'_, '_>, end: Option<u32>, next: Option<u32>);

    /// At the end of a list: `from` is where its last item ends and `to` where the list ends.
    /// Reported only when the list has an end and the next comment starts before it.
    fn until(&mut self, out: &mut CommentWriter<'_, '_>, from: Option<u32>, to: Option<u32>);

    /// Where the next comment the hook would write starts, or `None` when it has none left. The
    /// printer asks after each call. It also wraps the argument of `return` and `throw` in
    /// parentheses when this is before the argument, so that a line comment cannot end the
    /// statement.
    fn next_comment(&self) -> Option<u32>;
}

/// What a [`CommentHook`] may write.
pub struct CommentWriter<'p, 'a> {
    p: &'p mut Codegen<'a>,
}

impl CommentWriter<'_, '_> {
    /// Writes `text` as it is, indented when it starts a line.
    pub fn write(&mut self, text: &str) {
        if self.p.code.last_byte() == Some(b'\n') {
            self.p.print_indent();
        }
        self.p.print_str(text);
    }

    /// Writes a `//` comment and ends its line, so nothing after it is commented out.
    pub fn line_comment(&mut self, text: &str) {
        self.write(text);
        self.newline();
    }

    /// The last byte of the output that is not a space or a tab.
    pub fn last_byte(&self) -> Option<u8> {
        let bytes = self.p.code.as_bytes();
        bytes.iter().rev().find(|b| !matches!(b, b' ' | b'\t')).copied()
    }

    /// Takes back the line break that ends the output, as a statement prints its own, so that a
    /// comment can end that line; `true` when it did, and the hook then ends the line itself.
    pub fn reopen_line(&mut self) -> bool {
        if self.p.mappings_read_until() >= self.p.code.len() {
            return false;
        }
        if self.p.code.last_byte() != Some(b'\n') {
            return false;
        }
        self.p.code.pop_ascii_byte();
        true
    }

    /// A space, unless a line starts here, where [`CommentWriter::write`] indents instead.
    pub fn space(&mut self) {
        if self.p.code.last_byte() != Some(b'\n') {
            self.p.print_hard_space();
        }
    }

    pub fn newline(&mut self) {
        self.p.print_hard_newline();
    }
}

/// A span as a position, or `None` for a node the host built.
fn located(span: Span) -> Option<Span> {
    (!span.is_unspanned()).then_some(span)
}

impl<'a> Codegen<'a> {
    #[inline]
    fn with_hook(&mut self, f: impl FnOnce(&mut dyn CommentHook, &mut CommentWriter<'_, 'a>)) {
        if let Some(mut hook) = self.comment_hook.take() {
            let before = self.code.len();
            f(&mut *hook, &mut CommentWriter { p: self });
            self.hook_next = hook.next_comment();
            self.comment_hook = Some(hook);
            // What starts a statement, an arrow body or a default export still starts it after a
            // comment, which decides whether an object or a function there needs parentheses.
            let after = self.code.len();
            if after != before {
                for start in [
                    &mut self.start_of_stmt,
                    &mut self.start_of_arrow_expr,
                    &mut self.start_of_default_export,
                ] {
                    if *start == before {
                        *start = after;
                    }
                }
            }
        }
    }

    /// Whether the hook has a comment to write before `span`'s node.
    #[inline]
    pub(crate) fn hook_comment_before(&self, span: Span) -> bool {
        located(span).is_some_and(|s| self.hook_next.is_some_and(|next| next < s.start))
    }

    /// Reports the node at `span` when the hook's next comment is before it; `true` when it did,
    /// and [`Codegen::hook_after`] is then due.
    #[inline]
    pub(crate) fn hook_before(&mut self, span: Span, expression: bool) -> bool {
        if !self.hook_comment_before(span) {
            return false;
        }
        let start = self.code.len();
        self.with_hook(|h, out| h.before(out, located(span), expression));
        if expression && self.code.len() != start {
            self.hook_wrote = Some((start, self.code.len()));
        }
        true
    }

    /// The parentheses an expression prints around itself, which go before the comments the hook
    /// has just written for it, so those stay next to the expression, as upstream's printer has
    /// them.
    #[inline]
    pub(crate) fn print_open_parenthesis(&mut self) {
        if let Some((start, end)) = self.hook_wrote.take()
            && end == self.code.len()
            && self.mappings_read_until() <= start
        {
            self.code.insert_ascii_byte(start, b'(');
            for marker in [
                &mut self.start_of_stmt,
                &mut self.start_of_arrow_expr,
                &mut self.start_of_default_export,
            ] {
                if *marker > start {
                    *marker += 1;
                }
            }
        } else {
            self.print_ascii_byte(b'(');
        }
    }

    #[inline]
    fn mappings_read_until(&self) -> usize {
        #[cfg(feature = "sourcemap")]
        if let Some(builder) = &self.sourcemap_builder {
            return builder.read_until();
        }
        0
    }

    #[inline]
    pub(crate) fn hook_after(&mut self, span: Span) {
        self.with_hook(|h, out| h.after(out, located(span)));
    }

    #[inline]
    pub(crate) fn hook_body(&mut self, owner: Span) {
        if let Some(hook) = self.comment_hook.as_mut() {
            hook.body(located(owner));
            self.hook_next = hook.next_comment();
        }
    }

    /// After a statement or class member of a list that `owner` holds; `next` is the next one's
    /// span.
    #[inline]
    pub(crate) fn hook_after_statement(&mut self, end: Span, next: Option<Span>, owner: Span) {
        if self.hook_next.is_some() {
            let next = next.map_or(located(owner), located).map(|s| s.end);
            self.hook_trailing(located(end).map(|s| s.end), next);
        }
    }

    /// A trailing comment ends by `next`, so none starts at or after it.
    #[inline]
    fn hook_trailing(&mut self, end: Option<u32>, next: Option<u32>) {
        if end.is_some() && self.hook_next.is_some_and(|c| next.is_none_or(|n| c < n)) {
            self.with_hook(|h, out| h.trailing(out, end, next));
        }
    }

    /// After an item of a list that ends at `until`; `next` is the next item's span.
    #[inline]
    pub(crate) fn hook_after_item(&mut self, item: Span, next: Option<Span>, until: Option<u32>) {
        if self.hook_next.is_some() {
            let next = next.map_or(until, |s| located(s).map(|s| s.start));
            self.hook_trailing(located(item).map(|s| s.end), next);
        }
    }

    /// At the end of a list whose last item is `last`.
    #[inline]
    pub(crate) fn hook_until(&mut self, last: Option<Span>, to: Option<u32>) {
        if let Some(to) = to
            && self.hook_next.is_some_and(|c| c < to)
        {
            let from = last.and_then(located).map(|s| s.end);
            self.with_hook(|h, out| h.until(out, from, Some(to)));
        }
    }

    /// Where a list that ends with `span`'s end stops, or `None` when `span` has no position.
    #[inline]
    pub(crate) fn end_of(span: Span) -> Option<u32> {
        located(span).map(|s| s.end)
    }

    /// Where a list that stops before `span` stops, or `None` when `span` has no position.
    #[inline]
    pub(crate) fn start_of(span: Span) -> Option<u32> {
        located(span).map(|s| s.start)
    }
}
