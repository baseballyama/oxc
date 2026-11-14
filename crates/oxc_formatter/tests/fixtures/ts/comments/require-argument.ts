// A comment in `require()` breaks its parens only when it has to, as in an `import()` type
import type A = require( // a
  "a"
);
import type B = require(
// b
  "a"
);
import type C = require( /* c */
  "a"
);
import type D = require( /* d */"a"
);
import type E = require("a" /* e */);
type F = import( /* f */
  "a"
);
