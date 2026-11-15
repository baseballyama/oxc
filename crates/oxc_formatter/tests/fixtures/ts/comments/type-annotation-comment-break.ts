// A comment between a type's `:` and the type breaks and indents it, as after `=`:
// a line comment stays on the `:` line, an own-line comment stays own-line
let v1: // c
  T;
let v2:
  // c
  T;
let v3: /* c */ T;
let v4:
  // prettier-ignore
  C  |  D;
let v5: (
  // c
  any
);
let v6: // c
  | Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb;
let v7: // c
  [string] & [string, ExpressionNode] & [string, ExpressionNode, ExpressionNode] & [string, ExpressionNode];

interface I {
  p1: // c
    T;
  p2?:
    // c
    T;
  p3:
    // prettier-ignore
    C  |  D;
  [key: string]: // c
    T;
}

class A {
  p: // c
    T;
  m(a: // c
    T): // c
    T {}
}

function f(a: // c
  T): // c
  T {}

type M1 = {
  [A in B]: // c
    C;
};
type M2 = {
  [A in B as `x${A}`]?:
    // c
    C;
};
type M3 = {
  [A in B]:
    // prettier-ignore
    C  |  D;
};
type M5 = {
  [A in B]?:
    /** c */
    C;
};
// Before the `:`, the comment stays between the `]` and the `:` (DIVERGENCES.md#member-operator-comment-side)
type M4 = {
  [A in B] /* c */: C;
};

// No `:` after a function type's `=>`
type F = () => Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb;
