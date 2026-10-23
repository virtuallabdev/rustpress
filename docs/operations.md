# Operations and release gates

This is a development prototype. The Compose stack is not a production recipe.

## Trust boundaries

Keep PostgreSQL and Axum on a private network. Only the public Nuxt theme and
the separately protected administration origin should be reachable by browsers.
Do not publish the API container port as a shortcut. The public theme proxies
only the public content API; its media directory contains generated images only.

For production:

1. Resolve the dependency advisories listed in the [README](../README.md).
2. Build and test all three container images on the deployment OS. Local WSL
   smoke tests passed with PostgreSQL 18.6 and non-root, read-only application
   containers. The API image was built offline from locked vendored sources
   because Rust download hosts were unreachable from this WSL network. Verify
   the standard online API Dockerfile in the deployment network as well.
3. Terminate TLS at a trusted reverse proxy. Set `RUSTPRESS_ENV=production`,
   exact HTTPS `ADMIN_ORIGIN` and `SITE_ORIGIN`, and matching
   `NUXT_PUBLIC_SITE_ORIGIN`. Preserve the browser's `Origin`, `Cookie`,
   `Set-Cookie` and OAuth redirect headers through the admin proxy.
4. Restrict administration at the edge, configure request/upload timeouts,
   request-size limits and per-client rate limits. Never trust arbitrary
   forwarded headers supplied by external clients.
5. Use a non-superuser runtime PostgreSQL role and a separate migrator role.
   The local Compose database role owns the schema and is not a least-privilege
   production configuration. The current executable migrates at startup, so a
   production role split also requires separating migration from application
   startup before deployment. Configure statement/lock/idle transaction timeouts
   at the role/database level.
6. Configure secrets through the deployment platform, not image layers or git.
   Environment variables are convenient locally but visible to host/container
   administrators. Protect host access and secret-manager permissions.
7. Configure CPU, memory, disk quotas and upload lifecycle/retention. Two image
   jobs are allowed per API process; originals and generated assets consume disk.
8. Verify OAuth with each registered app and an explicitly granted identity;
   test denial, expiry, role change, logout and recovery procedures.
9. Establish and test backup/restore, monitoring, dependency updates and incident
   response before handling production data.

## Rate limits and performance

The API has an in-memory 120 requests/minute limit per direct TCP peer, at most
10,000 active peers. It deliberately ignores forwarded IP headers. **Behind
Nuxt, this groups visitors by Nuxt server IP**, so it is a coarse safety bound,
not a production per-user rate limiter. It also applies to readiness probes.
Production needs explicit trusted-proxy handling and edge limits before this
default is suitable for meaningful load.

The database pool is bounded to 20 connections. Pagination, full-text indexes,
bounded image decoding and release LTO provide a starting point, not proven
latency/throughput targets. Text APIs are `no-store` to avoid leaking drafts and
stale scheduled content. Successfully served immutable generated images receive
one-year public cache headers. There is no distributed cache or invalidation.

Agree on hardware, dataset size, traffic shape, p50/p95/p99 latency, throughput,
error rate, database utilization and image queue behavior before benchmarking.
Measure public reads separately from editorial transactions and image uploads.
No performance numbers have been claimed without measurements.

## Data visibility and retention

Every `fields` value of a published content item is public. Do not use custom
fields for secrets, personal administration notes or internal-only attributes.
All uploaded generated image variants are publicly addressable immediately,
even when referenced only by drafts. Originals are private files and may retain
EXIF, GPS or other personal metadata; generated variants are re-encoded.
Use filesystem encryption/access control and a deliberate retention policy.

Relations validate identifier existence but are not expanded into other
documents, and are not an authorization mechanism. The initial release lacks
private media and field-level visibility.

Markdown is stored as source text, not executable markup. The shared Rust
renderer escapes raw HTML, filters unsafe link schemes, restricts image sources
to generated local WebP URLs, then applies an HTML sanitizer allowlist.
Do not replace it with permissive client Markdown/MDX rendering. The visual
editor also receives server-sanitized HTML. Future plugins must not bypass this
boundary. Preview requires editorial authentication and is not a public draft
sharing mechanism.

Locale is part of content identity and unique slug constraints. English URLs
use `/en/`; the Italian slug `en` is reserved and must be renamed before
migration if already present. Existing bodies and historical revision JSON
remain intact. Translation publication is independent and missing translations
return 404. Public alternate links never include draft/scheduled translations.

Sessions expire after eight hours; OAuth state after ten minutes; agent tokens
after thirty days. Expired credentials/states are opportunistically removed
during login. Revisions and audit records currently have no retention job.
Agree retention, deletion and legal-basis requirements before GDPR production
use. No analytics or non-essential tracking is installed by the project.

No browser password fallback is provided. Keep an audited operator recovery
path through the `grant` command and database access. Back up provider app
configuration and protect the app secrets. Email is not used for account linking.
MFA must currently be enforced at the identity provider.

## Backups and recovery

Choose RPO/RTO, encryption, storage region, retention and responsible operators.
Back up PostgreSQL **and** the media volume; a database-only backup cannot
restore referenced image files.

- Use PostgreSQL 18-compatible `pg_dump`/`pg_restore` tooling and a protected
  passfile or platform credential injection, never passwords in command history.
- Coordinate database and filesystem snapshots or temporarily stop editorial
  writes to obtain a consistent pair.
- Encrypt backups, isolate their credentials and copy them outside the host.
- Restore into a new isolated database and media directory, not over production.
- Run migrations, readiness checks and public/editorial smoke tests against the
  restored instance. Check image references and revisions.
- Revoke restored credentials where appropriate and rotate secrets after an
  incident. Record restore duration and data-loss window.

These are required operating procedures, **not an implemented or tested backup
service**. The current project does not claim verified RPO/RTO.

## Monitoring and security verification

`/health/live` does not query PostgreSQL. `/health/ready` runs `SELECT 1`.
Monitor process exits, latency, HTTP 5xx/429, connection pool pressure, database
locks, disk growth and decoder failures. Audit rows record successful content,
schema, menu and media mutations in the same database transaction. Rejected
writes do not masquerade as successful events.

Application errors are logged server-side without returning SQL or infrastructure
details to clients. Do not enable reverse-proxy query/header logging for OAuth
callbacks, cookies or bearer credentials. Provider client errors do not include
provider response bodies in client messages.

Run `npm audit` and a Rust advisory scanner in the release process. An audit
finding is not automatically proof of an exploitable production path, but must
be investigated rather than hidden. Commission a dedicated security review,
including proxy/header behavior, CSRF, token lifecycle, OAuth app settings and
untrusted image decoding. The existing tests are regression checks, not a
security certification.
