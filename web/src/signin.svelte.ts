/// Signing in to write (D-58: reads stay public; a write carries the
/// signed-in person's token). The pages are a public single-page client
/// of the agency's sign-in provider (OpenID Connect; Entra at C-TRAN):
/// the authorization code flow with PKCE — out to the provider, back
/// with a code, the code exchanged for tokens at its token endpoint,
/// refreshed with its refresh token. The provider and the client are the
/// manifest's (`auth`, the server's -oidc-client-id); with none, sign-in
/// is off and writes go as they are. Tokens live in this tab's session
/// storage, never anywhere else; nothing here decides who may write —
/// the server's policy does.
///
/// Every page write goes through `writeFetch`; a page calls `resumeSignIn`
/// at boot (it finishes a sign-in the provider redirected back from) and
/// shows `signin` (who is signed in) beside a sign-in control.

import { deploymentAuth } from "./engine";

type Tokens = { access: string; refresh?: string; expires: number; name: string };
type Pending = { verifier: string; state: string; back: string };

const TOKENS = "signin.tokens";
const PENDING = "signin.pending";

/// Who is signed in (their name, as the provider's id token gives it),
/// whether this deployment asks for sign-in at all, and why the last
/// sign-in failed.
export const signin = $state({ name: "", available: false, error: "" });

function read<T>(key: string): T | null {
  try {
    const text = sessionStorage.getItem(key);
    return text ? (JSON.parse(text) as T) : null;
  } catch {
    return null;
  }
}

function keep(key: string, value: unknown): void {
  try {
    if (value === null) sessionStorage.removeItem(key);
    else sessionStorage.setItem(key, JSON.stringify(value));
  } catch {
    // no storage (a private window's limits): signed in for this page only
  }
}

const b64u = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes)).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");

function claims(jwt: string): Record<string, unknown> {
  try {
    const body = jwt.split(".")[1].replaceAll("-", "+").replaceAll("_", "/");
    return JSON.parse(atob(body + "=".repeat((4 - (body.length % 4)) % 4))) as Record<string, unknown>;
  } catch {
    return {};
  }
}

let discovered: Promise<{ authorization_endpoint: string; token_endpoint: string }> | null = null;
function discovery(issuer: string) {
  discovered ??= fetch(`${issuer.replace(/\/$/, "")}/.well-known/openid-configuration`).then((r) => {
    if (!r.ok) throw new Error(`the sign-in provider did not answer (${r.status})`);
    return r.json();
  });
  return discovered;
}

/// The page's own address, which the provider sends the person back to
/// (registered with the provider as a redirect address).
const redirectUri = () => `${location.origin}${location.pathname}`;

async function tokenRequest(form: Record<string, string>): Promise<Tokens> {
  const auth = await deploymentAuth();
  if (!auth) throw new Error("sign-in is off here");
  const { token_endpoint } = await discovery(auth.issuer);
  const resp = await fetch(token_endpoint, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ client_id: auth.client_id, ...form }),
  });
  if (!resp.ok) throw new Error(`the sign-in provider refused (${resp.status})`);
  const t = (await resp.json()) as { access_token: string; refresh_token?: string; expires_in?: number; id_token?: string };
  const who = claims(t.id_token ?? t.access_token);
  const name = String(who.name ?? who.preferred_username ?? who.email ?? who.sub ?? "");
  return { access: t.access_token, refresh: t.refresh_token, expires: Date.now() + (t.expires_in ?? 3600) * 1000, name };
}

/// At boot: whether this deployment asks for sign-in, finishing a
/// sign-in the provider redirected back from (the page's address
/// restored, its hash included), and who is signed in.
export async function resumeSignIn(): Promise<void> {
  const auth = await deploymentAuth().catch(() => null);
  signin.available = !!auth;
  if (!auth) return;
  const q = new URLSearchParams(location.search);
  const pending = read<Pending>(PENDING);
  if (pending && q.get("code") && q.get("state") === pending.state) {
    keep(PENDING, null);
    history.replaceState(null, "", pending.back);
    try {
      keep(TOKENS, await tokenRequest({ grant_type: "authorization_code", code: q.get("code")!, redirect_uri: redirectUri(), code_verifier: pending.verifier }));
    } catch (e) {
      signin.error = (e as Error).message;
    }
  } else if (q.get("error")) {
    signin.error = q.get("error_description") ?? q.get("error") ?? "";
    history.replaceState(null, "", redirectUri() + location.hash);
  }
  signin.name = read<Tokens>(TOKENS)?.name ?? "";
}

/// Out to the provider; back to this page, where it was.
export async function signIn(): Promise<void> {
  const auth = await deploymentAuth();
  if (!auth) return;
  const { authorization_endpoint } = await discovery(auth.issuer);
  const verifier = b64u(crypto.getRandomValues(new Uint8Array(32)));
  const challenge = b64u(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))));
  const state = b64u(crypto.getRandomValues(new Uint8Array(16)));
  keep(PENDING, { verifier, state, back: location.href } satisfies Pending);
  const url = new URL(authorization_endpoint);
  for (const [k, v] of Object.entries({
    response_type: "code",
    client_id: auth.client_id,
    redirect_uri: redirectUri(),
    scope: auth.scope,
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
  }))
    url.searchParams.set(k, v);
  location.assign(url.toString());
}

export function signOut(): void {
  keep(TOKENS, null);
  signin.name = "";
}

/// The access token to write with: refreshed when it is about to expire,
/// null when no one is signed in (or the refresh is refused).
async function accessToken(): Promise<string | null> {
  const t = read<Tokens>(TOKENS);
  if (!t) return null;
  if (t.expires - Date.now() > 60_000) return t.access;
  if (!t.refresh) return null;
  try {
    const next = await tokenRequest({ grant_type: "refresh_token", refresh_token: t.refresh });
    keep(TOKENS, next);
    return next.access;
  } catch {
    signOut();
    return null;
  }
}

/// A write to the deployment, carrying the signed-in person's token when
/// there is one. A 401 means sign-in is needed (the caller says so);
/// what the server answers is otherwise the caller's to read.
export async function writeFetch(url: string, init: RequestInit): Promise<Response> {
  const token = await accessToken();
  const headers = new Headers(init.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const resp = await fetch(url, { ...init, headers });
  if (resp.status === 401) signOut();
  return resp;
}

/// A read that needs the signed-in person (attachments, D-162): the
/// token when there is one; a 401 means sign in to see it.
export async function readFetch(url: string): Promise<Response> {
  const token = await accessToken();
  return fetch(url, token ? { headers: { Authorization: `Bearer ${token}` } } : {});
}
