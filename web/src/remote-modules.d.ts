// CDN-loaded basemap tooling (see MapView): dynamic imports of remote
// ES modules have no type declarations — typed as any at the boundary.
declare module "https://*";
