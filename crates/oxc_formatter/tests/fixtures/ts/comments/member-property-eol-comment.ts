// A line comment ending the object's line keeps its side of the member's operator
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
  }
}

x = a /* c */
  .b;
// DIVERGENCES.md#member-property-own-line-comment
x = a // c
  /* d */ .b;
