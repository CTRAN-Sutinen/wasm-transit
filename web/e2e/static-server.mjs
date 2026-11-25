// A plain static host for e2e/static-deploy.spec.mjs (D-131). It serves files
// from a directory with a content type by extension, and does no content
// negotiation, ETag, Range, compression, or routing, so a page that boots here
// boots on any static host. Like Cloudflare Workers and Netlify, it sends the
// headers that the bundle's `_headers` file names. The specs then run each page
// under the policy that the export wrote.
//
//   node e2e/static-server.mjs <root> <port> [cors | forward=<server URL>]
//
// `cors` adds `Access-Control-Allow-Origin: *`, as an agency's feed host sends
// it (e2e/static-feed-url.spec.mjs).
//
// `forward=<server URL>` makes it a UI's own host (D-171; e2e/ui-origins.spec.mjs).
// It serves the UI's bundle from <root> and forwards the data paths to the
// server, so the page is same-origin with its data. The data paths are every
// route of the server's HTTP contract (server/openapi.json, each path's
// `x-mux`) except the static files and the basemap.

import http from "node:http";
import fs from "node:fs";
import path from "node:path";

const [root, port, mode = ""] = process.argv.slice(2);
const corsHeaders = mode === "cors" ? { "Access-Control-Allow-Origin": "*" } : {};
const upstream = mode.startsWith("forward=") ? mode.slice("forward=".length).replace(/\/+$/, "") : null;

/// A pattern that ends in "/" is a prefix.
function dataPaths() {
  const contract = JSON.parse(fs.readFileSync(new URL("../../server/openapi.json", import.meta.url), "utf8"));
  const patterns = new Set(Object.values(contract.paths).map((item) => item["x-mux"]));
  patterns.delete("/");
  return [...patterns, "/basemap.pmtiles", "/basemap-assets/"];
}
const forwarded = upstream ? dataPaths() : [];
const isData = (pathname) => forwarded.some((p) => (p.endsWith("/") ? pathname.startsWith(p) : pathname === p));

function forward(req, res) {
  const target = new URL(req.url, upstream);
  const out = http.request(target, { method: req.method, headers: { ...req.headers, host: target.host } }, (answer) => {
    res.writeHead(answer.statusCode ?? 502, answer.headers);
    answer.pipe(res);
  });
  out.on("error", () => res.writeHead(502).end());
  req.pipe(out);
}
const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".wasm": "application/wasm",
  ".gz": "application/gzip",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

function headerRules(dir) {
  let text = "";
  try {
    text = fs.readFileSync(path.join(dir, "_headers"), "utf8");
  } catch {
    return [];
  }
  const rules = [];
  for (const line of text.split("\n")) {
    if (!line.trim() || line.startsWith("#")) continue;
    if (/^\s/.test(line)) {
      const at = line.indexOf(":");
      rules.at(-1)?.headers.push([line.slice(0, at).trim(), line.slice(at + 1).trim()]);
    } else {
      const pattern = line.trim().replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
      rules.push({ match: new RegExp(`^${pattern}$`), headers: [] });
    }
  }
  return rules;
}

/// A rule's pattern is relative to its bundle, which is the host's root or the
/// directory under the path's first segment (/ride/, /display/).
function headersFor(pathname) {
  const segment = pathname.split("/")[1] ?? "";
  const sent = {};
  for (const [dir, relative] of [[root, pathname], [path.join(root, segment), pathname.slice(segment.length + 1) || "/"]]) {
    for (const rule of headerRules(dir)) {
      if (rule.match.test(relative)) for (const [name, value] of rule.headers) sent[name] = value;
    }
  }
  return sent;
}

http
  .createServer((req, res) => {
    const pathname = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (upstream && isData(pathname)) {
      forward(req, res);
      return;
    }
    let file = path.join(root, path.normalize(pathname));
    if (!file.startsWith(path.resolve(root))) {
      res.writeHead(400).end();
      return;
    }
    if (fs.existsSync(file) && fs.statSync(file).isDirectory()) file = path.join(file, "index.html");
    fs.readFile(file, (err, body) => {
      if (err) {
        res.writeHead(404).end();
        return;
      }
      res
        .writeHead(200, { "Content-Type": types[path.extname(file)] ?? "application/octet-stream", ...corsHeaders, ...headersFor(pathname) })
        .end(body);
    });
  })
  .listen(Number(port), "127.0.0.1");
