//! Comments placed by the host instead of by [`Codegen`], see [`Codegen::with_comment_hook`].
//!
//! The printer reports where it is: before and after a node, at the start of each statement list,
//! after each statement or list item, and at the end of each list. The host decides which of its
//! comments to write there. A node whose span is [`SPAN`](oxc_span::SPAN) has no position in the
//! source, so the printer reports `None` for it. So that a program pays only where it has
//! comments, the printer asks [`CommentHook::next_comment`] where the next one starts, and reports
//! a node only when that is before the node.
//!
//! What the printer prints around a comment it decides before printing, by asking the hook: a
//! statement whose line a comment ends keeps that line open, a list that a comment breaks prints
//! one item per line, and an expression that opens with parentheses gets its comments inside
//! them. A line break the hook asks for is written where the next line starts, indented as that
//! line is, so nothing written is taken back.

use oxc_span::Span;

use crate::Codegen;

/// Where the printer is, reported to the host that places comments.
///
/// Positions are offsets in the source that the nodes' spans index; `None` is a node without one.
pub trait CommentHook {
    /// Before the node at `span` prints, when [`CommentHook::next_comment`] is before it.
    /// `expression` is `true` for an expression, whose comments go inside the parentheses it
    /// prints around itself.
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
    /// printer asks after each call.
    fn next_comment(&self) -> Option<u32>;

    /// Whether the comments the hook has left before `until` print over lines, so that the list
    /// they are in puts each item on its own line. The list's first item starts at `first`, and a
    /// comment before it follows the list's opening bracket. `trailing` is whether a comment on the
    /// line of the item before it counts too.
    fn breaks(&self, first: u32, until: u32, trailing: bool) -> bool {
        let _ = (first, until, trailing);
        false
    }

    /// Whether the comments before `to` end a line, so that what comes before them must not end a
    /// statement there.
    fn breaks_before(&self, to: u32) -> bool {
        let _ = to;
        false
    }

    /// Whether the hook writes a comment inside the body at `span`, before it prints, so that an
    /// empty body opens a line for it.
    fn holds(&self, span: Span) -> bool {
        let _ = span;
        false
    }
}

/// What a [`CommentHook`] may write.
pub struct CommentWriter<'p, 'a> {
    p: &'p mut Codegen<'a>,
}

impl CommentWriter<'_, '_> {
    /// Writes `text`, on a line of its own when a line break is due, and indented when it starts a
    /// line.
    pub fn write(&mut self, text: &str) {
        if self.p.line_due {
            self.p.end_line();
            self.p.print_indent();
        } else if self.p.code.last_byte() == Some(b'\n') {
            self.p.print_indent();
        }
        self.p.print_str(text);
    }

    /// Writes a `//` comment and ends its line, so nothing after it is commented out.
    pub fn line_comment(&mut self, text: &str) {
        self.write(text);
        self.newline();
    }

    /// Writes `text` at the end of the line the printer is on, before a line break that is due.
    pub fn append(&mut self, text: &str) {
        self.p.print_str(text);
    }

    /// A space, unless a line starts here, where [`CommentWriter::write`] indents instead.
    pub fn space(&mut self) {
        if !self.p.line_due && self.p.code.last_byte() != Some(b'\n') {
            self.p.print_hard_space();
        }
    }

    /// Ends the line: the line break is written where the next line starts.
    pub fn newline(&mut self) {
        self.p.line_due = true;
    }
}

/// What the printer does after a hook wrote, which decides how a line break it asked for is
/// written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Then {
    /// The printer goes on with code on this line, so a line break that is due is written now,
    /// with the indent.
    Code,
    /// The printer starts a line itself, which writes the line break that is due.
    Line,
}

/// A span as a position, or `None` for a node the host built.
fn located(span: Span) -> Option<Span> {
    (!span.is_unspanned()).then_some(span)
}

impl<'a> Codegen<'a> {
    #[inline(never)]
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
                self.move_starts(before, after);
            }
        }
    }

    fn move_starts(&mut self, from: usize, to: usize) {
        for start in [
            &mut self.start_of_stmt,
            &mut self.start_of_arrow_expr,
            &mut self.start_of_default_export,
        ] {
            if *start == from {
                *start = to;
            }
        }
    }

    /// Writes the line break that is due.
    #[inline]
    pub(crate) fn end_line(&mut self) {
        self.line_due = false;
        self.print_ascii_byte(b'\n');
    }

    /// Writes a line break that is due when code follows on the line.
    #[inline]
    fn after_hook(&mut self, then: Then) {
        if then == Then::Code && self.line_due {
            let before = self.code.len();
            self.end_line();
            self.print_indent();
            self.hook_indented_at = self.code.len();
            self.move_starts(before, self.code.len());
        }
    }

    /// Whether the hook has a comment to write before `span`'s node.
    #[inline]
    pub(crate) fn hook_comment_before(&self, span: Span) -> bool {
        located(span).is_some_and(|s| self.hook_next.is_some_and(|next| next < s.start))
    }

    /// Reports the node at `span` before it prints; [`Codegen::hook_after`] is then due.
    #[inline(never)]
    pub(crate) fn hook_before(&mut self, span: Span, expression: bool) {
        self.with_hook(|h, out| h.before(out, located(span), expression));
        self.after_hook(Then::Code);
    }

    /// Where an expression that opens with parentheses has printed them, or would have: the
    /// comments [`Codegen::hook_pending`] holds for it go here.
    #[inline]
    pub(crate) fn hook_open(&mut self) {
        if let Some(span) = self.hook_pending.take() {
            self.hook_before(span, true);
        }
    }

    #[inline(never)]
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

    /// Where a statement of a list that `owner` holds goes next: the next one's end, or the end of
    /// the list.
    #[inline]
    fn statement_next(next: Option<Span>, owner: Span) -> Option<u32> {
        next.map_or(located(owner), located).map(|s| s.end)
    }

    /// After a statement or class member of a list that `owner` holds; `next` is the next one's
    /// span. The statement's line ends here.
    #[inline]
    pub(crate) fn hook_after_statement(&mut self, end: Span, next: Option<Span>, owner: Span) {
        if self.hook_next.is_some() {
            let next = Self::statement_next(next, owner);
            self.hook_trailing(located(end).map(|s| s.end), next, Then::Line);
        }
        if self.line_due && !self.hold_line {
            self.end_line();
        }
    }

    /// A trailing comment ends by `next`, so none starts at or after it.
    #[inline]
    fn hook_trailing(&mut self, end: Option<u32>, next: Option<u32>, then: Then) {
        if end.is_some() && self.hook_next.is_some_and(|c| next.is_none_or(|n| c < n)) {
            self.with_hook(|h, out| h.trailing(out, end, next));
            self.after_hook(then);
        }
    }

    /// After an item of a list that ends at `until`; `next` is the next item's span. `broken` is
    /// whether the list puts each item on its own line.
    #[inline]
    pub(crate) fn hook_after_item(
        &mut self,
        item: Span,
        next: Option<Span>,
        until: Option<u32>,
        broken: bool,
    ) {
        if self.hook_next.is_some() {
            let next = next.map_or(until, |s| located(s).map(|s| s.start));
            let then = if broken { Then::Line } else { Then::Code };
            self.hook_trailing(located(item).map(|s| s.end), next, then);
        }
    }

    /// At the end of a list whose last item is `last`.
    #[inline]
    pub(crate) fn hook_until(&mut self, last: Option<Span>, to: Option<u32>, broken: bool) {
        if let Some(to) = to
            && self.hook_next.is_some_and(|c| c < to)
        {
            let from = last.and_then(located).map(|s| s.end);
            let before = self.code.len();
            self.with_hook(|h, out| h.until(out, from, Some(to)));
            if broken {
                // The list's end goes on a line of its own, after the comments.
                self.line_due |= self.code.len() > before;
            } else {
                self.after_hook(Then::Code);
            }
        }
    }

    /// Whether the comments of a list break it over lines, see [`CommentHook::breaks`]. `items`
    /// are the spans of its items and `until` where the comments that count stop. The first item
    /// that has a position is the one the comments before it lead.
    #[inline]
    pub(crate) fn hook_breaks(
        &self,
        items: impl IntoIterator<Item = Span>,
        until: Option<u32>,
        trailing: bool,
    ) -> bool {
        let (Some(hook), Some(until)) = (&self.comment_hook, until) else {
            return false;
        };
        self.hook_next.is_some_and(|c| c < until) && {
            let first = items.into_iter().find_map(located).map_or(until, |s| s.start);
            hook.breaks(first, until, trailing)
        }
    }

    /// Whether the comments before `span`'s node end a line, see [`CommentHook::breaks_before`].
    #[inline]
    pub(crate) fn hook_breaks_before(&self, span: Span) -> bool {
        match (&self.comment_hook, located(span)) {
            (Some(hook), Some(s)) if self.hook_next.is_some_and(|c| c < s.start) => {
                hook.breaks_before(s.start)
            }
            _ => false,
        }
    }

    /// Whether the hook writes a comment inside the body at `span`, see [`CommentHook::holds`].
    #[inline]
    pub(crate) fn hook_holds(&self, span: Span) -> bool {
        match (&self.comment_hook, located(span)) {
            (Some(hook), Some(span)) => hook.holds(span),
            _ => false,
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
