// A line comment on the operator's line breaks an object pattern on the left, on either side of the operator
let { a, ...z } = // c
  f();
let { b } // c
  = f();
({ c } = // c
  f());
let { d } = /* c */ // e
  f();
// An array pattern keeps its own group, and nothing else on the left breaks
let [g] = // c
  f();
let h = // c
  f();
a.b.c = // c
  f();
let { i } = /* c */ f();
