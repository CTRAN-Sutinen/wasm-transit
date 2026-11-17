// A plain static host for e2e/static-deploy.spec.mjs (D-131): files
// from a directory, a content type by extension, and nothing else — no
// content negotiation, no ETag or Last-Modified, no Range, no
// compression, no routes. What a bucket or a bare web server does at
// its least, so a page that boots here boots on any static host. One
// thing it does beyond that, as Cloudflare Workers and Netlify do: it
// sends the headers the bundle's `_headers` file names (a path
// pattern, then indented `Name: value` lines; `*` matches anything),
// so the specs run each page under the policy the export wrote.
//
//   node e2e/static-server.mjs <root> <port> [cors]
//
// `cors` adds `Access-Control-Allow-Origin: *`, as an agency's feed
// host sends it (e2e/static-feed-url.spec.mjs).

import http from "node:http";
import fs from "node:fs";
import path from "node:path";

const [root, port, cors] = process.argv.slice(2);
const corsHeaders = cors === "cors" ? { "Access-Control-Allow-Origin": "*" } : {};
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

/// The `_headers` rules of the bundle in `dir`, read on each request.
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

/// The headers for a path: the rules of the bundle at the host's root,
/// or of the one under the path's first segment (/ride/, /display/).
/// A rule's pattern is relative to its bundle.
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
