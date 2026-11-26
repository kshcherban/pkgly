# Operating Pkgly

## Installation

Start with the [Quickstart](../quickstart.md). Pkgly requires **PostgreSQL**, and the installer provisions it alongside the application using Docker Compose. Create the first administrator through `/admin/install` in the web UI.

For a manual binary installation, download `pkgly-linux-amd64` or `pkgly-linux-arm64` and `checksums.txt` from [GitHub Releases](https://github.com/kshcherban/pkgly/releases). These are executable files, not archives. Verify the checksum, mark the binary executable, provision PostgreSQL, and use the [example configuration](https://github.com/kshcherban/pkgly/blob/main/examples/config.toml):

```bash
chmod +x pkgly-linux-amd64
./pkgly-linux-amd64 start --config /absolute/path/to/config.toml
```

The server listens on port `6742` by default; the generated Compose deployment maps it to `127.0.0.1:8000`. The binary does not install PostgreSQL or a systemd unit. For source builds, see [Development](../development.md). Linux builds require `pkg-config` and OpenSSL development headers (`libssl-dev` on Debian/Ubuntu, `openssl-devel` on Fedora/RHEL).

## HTTPS

For remote access, place Pkgly behind an HTTPS reverse proxy and set `[site].app_url` to the public URL and `is_https = true`. Keep PostgreSQL private. If you terminate TLS directly in Pkgly, configure:

```toml
[web_server.tls]
private_key = "/path/to/key.pem"
certificate_chain = "/path/to/fullchain.pem"
```

Restart the application after editing its configuration. See [Maintenance](./maintenance.md) for operational tasks.

## Storage Backends

- [Configuring S3 Storage](./s3.md) — steps for attaching Pkgly to an S3 or S3-compatible bucket.
- [Package Webhooks](./webhooks.md) — configure outbound publish/delete notifications and delivery retries.
- [Package Retention](./retention.md) — configure per-repository cleanup for old package files.

## Security-sensitive configuration

Set `[site].app_url` to the canonical HTTPS URL of the installation. Password-reset links use this configured value and never use request `Origin` or `Host` headers. Session cookies are `HttpOnly`, `Secure` when HTTPS is enabled, and always `SameSite=Lax`; the API is same-origin only.

Outbound proxy, webhook, OIDC/JWKS, and custom S3 requests are restricted to globally routable addresses. Private destinations require explicit exact-host or CIDR exceptions:

```toml
[security.egress]
allowed_hosts = ["s3.internal.example"]
allowed_cidrs = ["10.20.0.0/16"]
```

The policy rejects loopback, private, link-local, multicast, and metadata addresses by default.

Blocked destinations are enforced for the initial request as well as every redirect hop. Webhook deliveries that are blocked at runtime are marked as failed without retries.

## Email

Outbound email (e.g. password reset) uses the `[email]` section:

```toml
[email]
username = ""
password = ""
host = "smtp.example.com"
encryption = "TLS" # NONE, StartTLS, or TLS
from = "admin@pkgly.dev"
```

When `port` is omitted, the transport default is selected from `encryption`: `25` for `NONE`,
`587` for `StartTLS`, and `465` for `TLS`. Set `port` only for a non-standard SMTP port.
`NONE` uses plaintext SMTP and should only be used on a trusted network.

## Enabling SSO Login
Pkgly can delegate authentication to an upstream SSO provider (Cloudflare Access, Okta, Auth0, etc.) that issues signed JWT/ID tokens. Configure the security section in `config.toml` to enable the feature:

```toml
[security.sso]
enabled = true
login_path = "/api/user/sso/login"
login_button_text = "Sign in with SSO"
provider_login_url = "https://example.com/login"
provider_redirect_param = "redirect"
auto_create_users = true

  [[security.sso.providers]]
  name = "example"
  issuer = "https://issuer.example.com"
  audience = "my-client-id"
  jwks_url = "https://issuer.example.com/.well-known/jwks.json"
  token_source = { kind = "header", name = "Authorization", prefix = "Bearer " }
  role_claims = ["roles", "groups"]
```
- `login_path` is where the UI redirects users when clicking the SSO button.
- `provider_login_url` can point to the IdP login endpoint; Pkgly appends its own SSO callback URL using `provider_redirect_param` (defaults to `redirect`).
- Define one or more providers; each token is verified against JWKS with matching `iss` and `aud` claims. `role_claims` pull roles from claims and apply them to Casbin before redirecting the user.

You can also manage these settings under **Admin → System → Single Sign-On** without editing configuration files or restarting the service.

Requests that reach `/api/user/sso/login` must already be authenticated by the upstream provider; Pkgly verifies the JWT signature and claims, issues its own session cookie, and redirects back to the UI.

## Password Rules

Pkgly enforces configurable password strength requirements for all user passwords (login, registration, and password changes). The defaults require at least 8 characters with uppercase, lowercase, a number, and a special character.

### Configuring via config.toml

Add a `[security.password_rules]` section:

```toml
[security.password_rules]
min_length = 12
require_uppercase = false
require_lowercase = true
require_number = false
require_symbol = false
```

All fields are optional. Omitted fields fall back to their defaults (`min_length: 8`, all `require_*` set to `true`).

### Managing via the Admin UI

Password rules can also be configured at runtime under **Admin → System → Password Rules** without editing configuration files or restarting the service. The UI provides:

- An **enable/disable** toggle — when disabled, no password strength checks are applied.
- A **minimum length** field and four toggles for uppercase, lowercase, number, and symbol requirements.
- An inline warning when no constraints are active (at least one must be set before saving).

Changes made through the UI are persisted to the database and **override the `config.toml` value** after the next restart. If you later disable the rules through the UI (which deletes the database entry), the `config.toml` value takes effect again on the next restart.
