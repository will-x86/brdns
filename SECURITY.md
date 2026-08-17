# Security

## Threat model

brdns serves a handful of trusted friends. The primary risks are:

1. Someone reaching the management API and tampering with rules/upstreams.
2. Someone reaching the DNS endpoints with a forged SNI to impersonate an
   account (the account number is the only identity — see below).
3. Secrets (the management token, TLS keys, database URL) leaking.

## Management API

- The axum management API is **off by default** (`control_plane.enabled`).
- When enabled, every `/api/*` route requires `Authorization: Bearer <token>`.
  The token comes from `control_plane.api_token` (or the
  `BRDNS__CONTROL_PLANE__API_TOKEN` env var).
- **With no token configured, the API refuses all management requests.**
- Token comparison is constant-time.
- `/healthz` and `/metrics` are unauthenticated by design (no sensitive data).
- Default listen address is `127.0.0.1:8080`. If you bind it elsewhere, put it
  behind TLS (a reverse proxy) — the management API itself is plain HTTP.

## DNS endpoints

- DoT (853) and DoH (443) authenticate the *account* only, via the SNI
  subdomain `{account}.dns.yourdomain.com`.
- This is **not authentication of a person**. Account numbers are bearer
  identifiers: whoever knows the number can use the account's policy. Treat
  account numbers like passwords — do not put them in browser history, logs,
  or share them beyond the account owner.
- For a real wildcard certificate for `*.dns.yourdomain.com`, use Let's
  Encrypt (DNS-01) — self-signed certs will not work for real clients.
- Unknown/unrecognized SNI falls back to `server.fallback_account` (or is
  refused when set to `""`). Keep the fallback restrictive.

## Storage

- All SQL uses parameterized queries (sqlx `query_as` with `.bind`), so rule
  values and account numbers cannot inject SQL.
- API inputs are validated: account numbers are `[a-zA-Z0-9-]` (1-64 chars);
  rule `target_value` is length-capped; `limit_count >= 0`.
- The control plane trait is the only storage surface, so swapping backends
  does not expose raw SQL elsewhere.

## Secrets & crypto

- Account numbers are generated from the OS CSPRNG with rejection sampling
  (uniform digits, no bias).
- The `api_token` is a config secret; keep it out of the repository and pass it
  via env var in deployment.
- TLS private keys are read from files or generated in memory (`certs.in_mem`);
  never log them.

## Known limitations

- There is no per-account authentication beyond the bearer account number.
  Anyone with the account number can consume its quota and see its policy.
  This matches the original design (no mTLS, no login) and is acceptable for a
  small trusted deployment; revisit if accounts are ever shared publicly.
- The management API has no per-user accounts or audit log yet.
