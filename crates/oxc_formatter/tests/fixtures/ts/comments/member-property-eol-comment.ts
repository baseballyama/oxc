// A line comment between a member's object and its property never flushes past the property
class A {
  #b;
  m() {
    x = a // c
      .b;
    x = a // c
      ?.b;
    x = a // c
      .#b;
    x = a // c
      .b!;
    x = a /* c */ // d
      .b.c;
    x = ((a // c
      .b as T) // d
      .e as U) // f
      .g;
    x = `${a // c
      .b}`;
    // DIVERGENCES.md#member-operator-comment-side
    x = a. // c
      b;
    x = a?. // c
      b;
    x = this. // c
      #b;
    x = a /* c */ . // d
      b;
  }
}

// DIVERGENCES.md#member-operator-comment-side
a. // c
  b;
foo(). // c
  b;
(a). // c
  b;
(a // c
).b;
a.b. // c
  c;
a.b // c
  .c;
(a as B) // c
  .b;
(a as B // c
).b;
(a + b). // c
  d;

x = a /* c */
  .b;
// DIVERGENCES.md#member-property-own-line-comment
x = a // c
  /* d */ .b;
