# Known divergences

Admission reasons and rules: see `FORMATTER_POLICY.md` "Known divergences".

## fill-separator-measure

- Why: uniform-rule (a printed space counts toward the print width wherever it sits)
- Pin: `src/printer/mod.rs::tests::fill_wraps_the_word_after_a_separator_that_would_pass_the_width`

```text
/* input (print width 10) */
["xxxxxxx", fill([line, "aa", line, "bb", line, "cc"])]

/* ours */
xxxxxxx aa
bb cc

/* prettier */
xxxxxxx aa bb
cc
```

A fill measures the space of the line before its next item, and wraps that item when the space and the item pass the print width.
Prettier's `fits` charges a line's space only when a string follows it in the measured doc,
so in a fill whose words sit in the separator slots (prettier-plugin-svelte's text) it lets the word after the line run past the width.

## line-start-line-collapse

- Why: uniform-rule (a blank line comes only from `empty_line` and `exact_line_breaks`)
- Pin: `src/printer/mod.rs::tests::a_soft_line_at_the_start_of_a_line_prints_nothing`

```text
/* input */
group(["a", hardline, softline, "b"])

/* ours */
a
b

/* prettier */
a

b
```

A line printed broken starts a new line only when the current line has content, so a line right after a line break prints nothing.
Prettier prints every broken line, so a soft line after a hard line leaves a blank line.
