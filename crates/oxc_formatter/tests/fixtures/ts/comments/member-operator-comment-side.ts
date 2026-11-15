// A comment keeps its source side of a member's `.` / `?.`, and of a mapped type's `]`:
// after the operator it prints after it, and a line or own-line comment there breaks and indents the property
a. // c
  b;
a?. // c
  b;
a. /* c */ b;
a./* c */b;
a. /* c */ // d
  b;
a. // c
  // d
  b;
a.
  // c
  b;
foo(). // c
  b;
(a + b). // c
  d;
x = a. // c
  b;
class A {
  #b;
  m() {
    this. // c
      #b;
  }
}

// Before the operator, the object keeps it
a // c
  .b;
a /* c */ .b;

// In a member chain
foo.bar. // c
  baz();
foo. // c
  bar.baz().qux();
a.b().c. // c
  d();

type M1 = {
  [A in B] /* c */: C;
};
type M2 = {
  [A in B] // c
  : C;
};
type M3 = {
  [A in B /* c */]: C;
};
