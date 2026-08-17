# Privacy

brdns is a multi-tenant DNS filtering service. Its privacy model is:

**No PII, ever.** There is no email address, name, phone number, or any other
personally identifying information anywhere in the system.

## Identity

- A tenant is an **account**, identified by an opaque account number (16 digits
  or a UUID). The account number is the only thing that links a query to a
  tenant.
- Identity is carried in the TLS **SNI**: `{account}.dns.yourdomain.com`, for
  both DoT and DoH. There are no URL tokens, no cookies, no mTLS client certs.
- Account numbers are meaningless outside brdns and cannot be reversed into a
  person.

## What is logged

- Structured logs record **outcomes, not content**: account number, rule
  decision, protocol, error kinds, and counters. **Query names (qnames) are
  never logged.**
- The raw SNI is never echoed into logs (it may carry arbitrary hostnames).
- Client IP addresses are not logged or stored.

## What is measured

- Prometheus metrics are labeled by **account number** (opaque) and by
  **outcome** (`allow`, `deny`, `limit_ok`, `limit_exceeded`, `error`) and
  **protocol** (`dot`, `doh`).
- Latency histograms carry the same labels.
- **No qname ever appears in a metric label or value.**

## What is traced

- OpenTelemetry spans carry `account` (opaque) and `protocol` only. No qnames,
  no IPs.

## What is stored

- Postgres holds: accounts (opaque number), rules, upstreams, quota counters,
  and the blocklist domain→category index. The blocklist is public data.
- Members (device labels) are opaque strings like `laptop` — no PII.

## What is *not* collected

- No query logs (no record of which domains a specific account resolved).
- No client IPs, User-Agents, or device fingerprints.
- No analytics/telemetry about individual users.
