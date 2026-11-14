// A cast callee breaks its parens the same way for `new` as for a call
new (require("./webpack/plugins/next-trace-entrypoints-plugin").TraceEntryPointsPlugin as P)({
  rootDir: dir,
});
(require("./webpack/plugins/next-trace-entrypoints-plugin").TraceEntryPointsPlugin as P)({
  rootDir: dir,
});
new (someVeryLongObjectName.someVeryLongPropertyName satisfies SomeVeryLongTypeName)();
new (a as B)();
