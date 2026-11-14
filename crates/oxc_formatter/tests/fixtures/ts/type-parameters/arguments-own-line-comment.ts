// An own-line comment before a single type argument keeps its own line
foo<
  // c
  Type
>(value);
foo<
  /* c */
  Type
>(value);
new Foo<
  // c
  { a: string }
>();
let x: Array<
  // c
  Type
>;

// A same-line block comment keeps the hug
foo</* c */ Type>(value);
