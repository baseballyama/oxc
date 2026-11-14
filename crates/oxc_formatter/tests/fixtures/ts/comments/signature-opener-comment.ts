// DIVERGENCES.md#signature-opener-comment
interface I {
  a /* c1 */ ? /* c2 */ (/* c3 */) /* c4 */;
  b /* c1 */ (/* c3 */) /* c4 */
  /* c1 */ new /* c2 */ (a: string): string;
}
let c: new /* c2 */ (/* c3 */) /* c4 */ => string;
