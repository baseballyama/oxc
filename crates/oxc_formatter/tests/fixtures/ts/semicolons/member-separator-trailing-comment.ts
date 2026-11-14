interface P {
  a?: string; /* c */
  b?: string; /*
   * multi
   **/
  c?: string; /** MARK */
}
type T = { m0?: string; /** MARK */ };
interface P {
  a?: string /* c */;
  b?: string /* c */ ; /* d */
  e?: string /* only */
  f?: string
}
type L = {
  m0?: string; /** MARK */
  m1?: number;
};
