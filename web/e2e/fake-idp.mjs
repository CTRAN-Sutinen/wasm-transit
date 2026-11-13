// A stand-in OpenID Connect provider for the e2e sign-in tests: the
// discovery document, an authorize endpoint that signs "Rosa Ortiz" in
// at once (no login screen) and redirects back with a code, a token
// endpoint that checks the PKCE verifier and answers RS256 tokens (and
// refreshes them), and the JWKS the server verifies them with. CORS is
// open, as a real provider's is for a single-page application client.
//
//   node e2e/fake-idp.mjs <port> <audience>

import http from "node:http";
import crypto from "node:crypto";

const port = Number(process.argv[2] ?? 8089);
const audience = process.argv[3] ?? "transit-server";
const issuer = `http://127.0.0.1:${port}`;
const { publicKey, privateKey } = crypto.generateKeyPairSync("rsa", { modulusLength: 2048 });
const jwk = { ...publicKey.export({ format: "jwk" }), kid: "k1", alg: "RS256", use: "sig" };
const codes = new Map(); // code → { challenge, client_id }

const b64u = (b) => Buffer.from(b).toString("base64url");
function sign(claims) {
  const head = b64u(JSON.stringify({ alg: "RS256", typ: "JWT", kid: "k1" }));
  const body = b64u(JSON.stringify(claims));
  const sig = crypto.sign("RSA-SHA256", Buffer.from(`${head}.${body}`), privateKey);
  return `${head}.${body}.${b64u(sig)}`;
}

function tokens(clientId) {
  const now = Math.floor(Date.now() / 1000);
  const person = { sub: "u-rosa", name: "Rosa Ortiz", preferred_username: "rosa.ortiz@agency.example" };
  return {
    token_type: "Bearer",
    expires_in: 3600,
    access_token: sign({ iss: issuer, aud: audience, iat: now, exp: now + 3600, roles: ["planner"], ...person }),
    id_token: sign({ iss: issuer, aud: clientId, iat: now, exp: now + 3600, ...person }),
    refresh_token: crypto.randomUUID(),
  };
}

function send(res, code, body, type = "application/json") {
  res.writeHead(code, { "Content-Type": type, "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "*" });
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

http
  .createServer(async (req, res) => {
    const url = new URL(req.url, issuer);
    if (req.method === "OPTIONS") return send(res, 204, "");
    if (url.pathname === "/.well-known/openid-configuration")
      return send(res, 200, { issuer, authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`, jwks_uri: `${issuer}/jwks` });
    if (url.pathname === "/jwks") return send(res, 200, { keys: [jwk] });
    if (url.pathname === "/authorize") {
      const q = url.searchParams;
      if (q.get("code_challenge_method") !== "S256" || !q.get("code_challenge")) return send(res, 400, { error: "invalid_request" });
      const code = crypto.randomUUID();
      codes.set(code, { challenge: q.get("code_challenge"), client_id: q.get("client_id") });
      const back = new URL(q.get("redirect_uri"));
      back.searchParams.set("code", code);
      back.searchParams.set("state", q.get("state"));
      res.writeHead(302, { Location: back.toString() });
      return res.end();
    }
    if (url.pathname === "/token" && req.method === "POST") {
      let raw = "";
      for await (const chunk of req) raw += chunk;
      const form = new URLSearchParams(raw);
      if (form.get("grant_type") === "refresh_token") return send(res, 200, tokens(form.get("client_id")));
      const held = codes.get(form.get("code"));
      codes.delete(form.get("code"));
      const challenge = b64u(crypto.createHash("sha256").update(form.get("code_verifier") ?? "").digest());
      if (!held || held.challenge !== challenge || held.client_id !== form.get("client_id")) return send(res, 400, { error: "invalid_grant" });
      return send(res, 200, tokens(held.client_id));
    }
    send(res, 404, { error: "not_found" });
  })
  .listen(port, "127.0.0.1");
