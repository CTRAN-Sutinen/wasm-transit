// A plain static host for e2e/static-deploy.spec.mjs (D-131): files
// from a directory, a content type by extension, and nothing else — no
// content negotiation, no ETag or Last-Modified, no Range, no
// compression, no routes. What a bucket or a bare web server does at
// its least, so a page that boots here boots on any static host.
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
      res.writeHead(200, { "Content-Type": types[path.extname(file)] ?? "application/octet-stream", ...corsHeaders }).end(body);
    });
  })
  .listen(Number(port), "127.0.0.1");
