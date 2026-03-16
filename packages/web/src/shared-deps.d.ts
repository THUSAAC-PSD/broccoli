declare module 'virtual:shared-deps-map' {
  /** Mapping of bare specifiers to browser-resolvable URLs for shared deps. */
  const sharedDepsMap: Record<string, string>;
  export default sharedDepsMap;
}
