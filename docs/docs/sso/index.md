# Single Sign-On (SSO)

Pkgly validates signed JWT/ID tokens from upstream SSO providers (Cloudflare Access, Okta, Auth0, Azure B2C, etc.) using JWKS. Tokens are verified per-provider, mapped to a Pkgly user, optional Casbin roles are applied, and a Pkgly session cookie is issued. Auto-creation requires that the identity provider asserts `email_verified: true` in the token.

## Configuration

Configure the `security.sso` block inside `cfg/pkgly.toml` (or via Admin → System → Single Sign-On):

```toml
[security.sso]
enabled = true
login_path = "/api/user/sso/login"
login_button_text = "Sign in with SSO"
provider_login_url = "https://example.com/login"   # optional
provider_redirect_param = "redirect"               # optional
auto_create_users = true
role_claims = ["roles", "role", "groups"]        # optional global role claim keys

  [[security.sso.providers]]
  name = "cloudflare"
  issuer = "https://<team>.cloudflareaccess.com"
  audience = "<your-aud-tag>"
  jwks_url = "https://<team>.cloudflareaccess.com/cdn-cgi/access/certs"
  token_source = { kind = "header", name = "Cf-Access-Jwt-Assertion" }
  subject_claim = "sub"                # optional, defaults to preferred_username/sub
  email_claim = "email"                # optional
  display_name_claim = "name"          # optional
  role_claims = ["roles", "role"]     # optional per-provider overrides
```

Key points:
- Define one or more providers. Each token is verified against the provider’s JWKS using the `kid` in the header and checked for matching `iss`/`aud`.
- Tokens can be sourced from headers or cookies; optional prefixes (e.g., `Bearer `) are stripped automatically.
- `role_claims` (global or per-provider) pull role values from string or string-array claims and apply them to Casbin before redirecting the user back to the UI.
- Users are auto-provisioned when `auto_create_users` is true **and** the identity provider asserts `email_verified: true` in the JWT/ID token. If the claim is missing or `false`, auto-creation is rejected even when `auto_create_users` is enabled.

## OIDC / JWT Providers (JWKS)

Header-based SSO still works, but Pkgly can now validate signed ID tokens directly against a provider's JWKS endpoint. Add one or more providers under `security.sso.providers`:

```toml
[security.sso]
enabled = true
login_path = "/api/user/sso/login"
login_button_text = "Sign in with SSO"

[[security.sso.providers]]
name = "cloudflare"
issuer = "https://<team>.cloudflareaccess.com"
audience = "<your-aud-tag>"
jwks_url = "https://<team>.cloudflareaccess.com/cdn-cgi/access/certs"
token_source = { kind = "header", name = "Cf-Access-Jwt-Assertion" }
# Optional claim overrides
subject_claim = "sub"
email_claim = "email"
display_name_claim = "name"
```

Each provider entry:

- `name`: Friendly identifier for logs.
- `issuer`: Expected `iss` claim.
- `audience`: Expected `aud` claim (often the OAuth/OIDC client ID).
- `jwks_url`: JWKS endpoint. If omitted Pkgly will try `/.well-known/openid-configuration` discovery on the issuer.
- `token_source`: Where to read the token (`header` or `cookie`). For headers you can supply an optional `prefix` (default `"Bearer "`).
- `subject_claim` / `email_claim` / `display_name_claim`: Optional claim keys when a provider deviates from `preferred_username`, `email`, or `name`.

JWKS keys are cached for one hour and refreshed automatically when they expire or a new `kid` shows up. Pkgly supports **RSA**, **EC** (P-256/P-384/P-521), and **EdDSA** (Ed25519/Ed448) key types. If no provider yields a valid token, Pkgly falls back to the legacy header-based extraction above.

### Runtime Configuration

Administrators can update these settings without restarting the server under **Admin → System → Single Sign-On**. Changes persist in the database and are visible immediately on the login screen.

## Frontend Experience

When SSO is enabled, the login page shows a "Sign in with SSO" button above the traditional username/password form. Users bypass the password flow entirely once the SSO proxy authenticates them. The frontend app automatically handles redirect targets (e.g., deep links to browse or project views) when returning from `/api/user/sso/login`.

## Reverse Proxy Checklist

- Ensure the upstream SSO proxy terminates TLS and authenticates users **before** forwarding to Pkgly.
- Strip or overwrite the identity headers so untrusted clients cannot spoof them.
- Allow access to `/api/user/sso/login`, `/api/user/logout`, and the SPA assets.

With these pieces in place, Pkgly delegates authentication to your enterprise IdP while retaining its existing session and authorization model.

## OAuth2 / OIDC Login (Google & Microsoft)

In addition to trusting upstream SSO headers, Pkgly can act as an OAuth2 client and talk directly to Google or Microsoft Entra ID (Azure AD). The backend handles the full authorization code flow with PKCE, validates ID tokens, and optionally maps IdP groups/roles into Casbin RBAC policies.

### Enabling OAuth2 in the Admin UI

1. Sign in as an administrator and open **Admin → System**. The page now has a dedicated
   *OAuth2 Providers* card beneath the legacy header-based SSO settings.
2. Populate the provider credentials (client ID/secret) and set the callback URL to `https://<your-domain>/api/user/oauth2/callback`.
3. (Optional) Provide paths to a Casbin `model.conf` and `policy.csv`. Pkgly reloads these files whenever you save, allowing you to map IdP roles to application permissions.
4. Click **Save**. The server validates the configuration, writes it to the database, and updates the in-memory OAuth2 client. No restart is required.

### Google Cloud Console Setup

1. Create an OAuth client inside **APIs & Services → Credentials**.
2. Choose **Web application** and add the Pkgly callback URL (`https://<your-domain>/api/user/oauth2/callback`) under **Authorized redirect URIs**.
3. Copy the **Client ID** and **Client Secret** into the Pkgly OAuth2 settings.
4. Enable the scopes you want to request. Pkgly defaults to `openid profile email`, which is sufficient to retrieve the user’s identity.
5. (Optional) If you need Google Workspace group claims, enable the *Admin SDK* API and configure domain-wide delegation or use the Groups API in a webhook that populates Casbin policies.

### Microsoft Entra ID Setup

1. Open the Azure Portal and register an application under **Entra ID → App registrations**.
2. Record the **Application (client) ID** and create a **Client secret**.
3. Set a **Redirect URI** of type Web: `https://<your-domain>/api/user/oauth2/callback`.
4. Decide which tenant scope to use. Pkgly accepts a specific tenant ID or `common` for multi-tenant sign-in.
5. Grant the `openid`, `profile`, and `email` API permissions for the Microsoft Graph. Add the `GroupMember.Read.All` permission if you want group claims in the ID token.
6. Paste the client information into the Pkgly OAuth2 configuration and, if desired, provide the tenant ID and additional scopes.

### Mapping Provider Roles to Casbin Policies

- The OAuth2 callback inspects the `roles` and `groups` claims in the ID token.
- Microsoft Entra can emit security groups or application roles. Google typically requires an external process to supply group membership; Pkgly falls back to `group:<email>` when none are present.
- Any roles discovered are synchronized with the configured Casbin model/policy. Pkgly assigns the authenticated email address as the Casbin subject and rewrites its group membership before each session.
- In the UI you can add *Group to role mappings* that translate provider group IDs into Pkgly roles (for example mapping `Employee.ReadWrite.All` to the `read/write` Casbin role). Mappings are applied in addition to the raw roles in the token, so you can keep both coarse and fine-grained assignments.
- Pkgly exposes every role it finds in the current Casbin policy as auto-complete suggestions when you edit the mapping list. That list is generated directly from the policy stored in the database.

### Casbin Model & Policy Editor

OAuth2 RBAC definitions now live alongside other application settings in PostgreSQL. The Admin → System screen exposes a pair of editors where you can update the Casbin model (INI syntax) and policy (CSV syntax). Saving the form writes the values back to the database, reloads the in-memory Enforcer, and updates the auto-complete suggestions immediately—no more copying files into containers.

If you prefer to start from the built-in defaults, the editor pre-populates them using the same templates that previously shipped as `resources/rbac/model.conf` and `resources/rbac/policy.csv`. Leave either textarea blank to revert to those defaults.
- In the UI you can add *Group to role mappings* that translate provider group IDs into Pkgly roles (for example mapping `Employee.ReadWrite.All` to the `read/write` Casbin role). Mappings are applied in addition to the raw roles in the token, so you can keep both coarse and fine-grained assignments.

### Frontend Experience

Once configured, the login page displays provider-specific buttons that hit:

- `GET /api/user/oauth2/login/google`
- `GET /api/user/oauth2/login/microsoft`

Pkgly redirects the user to the provider’s consent screen, exchanges the authorization code on callback, issues a Pkgly session cookie, and finally forwards the user to the original `redirect` query parameter.

## Example: Cloudflare One (One-Time PIN)

When Pkgly runs behind a Cloudflare Access application, Cloudflare authenticates users and adds identity headers to the proxied request. For the One-Time PIN IdP:

- `CF-Access-Authenticated-User-Email` contains the user’s verified email address.
- `CF-Access-Authenticated-User-Name` (if present) contains a friendly display name.
- Every authenticated request also carries a signed `Cf-Access-Jwt-Assertion` header that Cloudflare validates before forwarding to your origin. Pkgly now re-validates that signature against Cloudflare’s JWKS so the token cannot be spoofed.

Configure Pkgly to validate the Cloudflare Access token via JWKS:

```toml
[security]
allow_basic_without_tokens = false

  [security.sso]
  enabled = true
  login_path = "/api/user/sso/login"
  login_button_text = "Sign in with Cloudflare"
  provider_login_url = "https://app.pkgly.dev/cdn-cgi/access/login"
  provider_redirect_param = "redirect_url"
  auto_create_users = true

  [[security.sso.providers]]
  name = "cloudflare"
  issuer = "https://<team>.cloudflareaccess.com"
  audience = "<your-aud-tag>"
  jwks_url = "https://<team>.cloudflareaccess.com/cdn-cgi/access/certs"
  token_source = { kind = "header", name = "Cf-Access-Jwt-Assertion" }
  email_claim = "email"
  display_name_claim = "name"
  role_claims = ["roles"]
```

Cloudflare automatically blocks unauthenticated traffic, so Pkgly only receives requests that satisfy your Access policies. The JWKS-backed provider above re-validates the `Cf-Access-Jwt-Assertion` signature; no trust in plain headers is required.

## Example: Google Workspace via OAuth2 Proxy

When fronting Pkgly with an OAuth2 reverse proxy (such as [oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/)) that uses Google Workspace as the identity provider, configure the proxy to include headers containing the user’s email and name:

```yaml
# oauth2-proxy excerpt
upstreams:
  - https://pkgly:6742

providers:
  - id: google-workspace
    provider: google
    clientID: ${GOOGLE_CLIENT_ID}
    clientSecret: ${GOOGLE_CLIENT_SECRET}
    scope: "openid email profile"
    google:
      adminEmail: admin@example.com
      group: pkgly-users@example.com

setAuthorizationHeader: true
passAccessToken: true
passAuthorization: true
passUserHeaders: true
setXAuthRequest: true
```

### Docker Compose Example

Below is a minimal docker-compose setup that runs Pkgly behind oauth2-proxy and routes traffic through a single Nginx reverse proxy. OAuth2 proxy handles Google Workspace authentication and forwards identity headers to Pkgly.

```yaml
services:
  pkgly:
    image: pkgly-pkgly:latest
    env_file: .env
    volumes:
      - pkgly_data:/data
    networks: [pkgly_net]

  oauth2_proxy:
    image: quay.io/oauth2-proxy/oauth2-proxy:v7.7.1
    command:
      - --http-address=0.0.0.0:4180
      - --upstream=http://pkgly:6742
      - --cookie-secret=${OAUTH2_PROXY_COOKIE_SECRET}
      - --cookie-secure=true
      - --cookie-domain=example.com
      - --cookie-refresh=24h
      - --cookie-expire=168h
      - --reverse-proxy=true
      - --pass-access-token=true
      - --pass-authorization-header=true
      - --set-authorization-header=true
      - --set-xauthrequest=true
      - --skip-provider-button=true
      - --provider=google
      - --client-id=${GOOGLE_CLIENT_ID}
      - --client-secret=${GOOGLE_CLIENT_SECRET}
      - --redirect-url=https://login.example.com/oauth2/callback
      - --email-domain=example.com
      - --scope=openid email profile
      - --oidc-extra-audience=https://www.googleapis.com/auth/userinfo.email
    environment:
      OAUTH2_PROXY_COOKIE_SECRET: ${OAUTH2_PROXY_COOKIE_SECRET}
    networks: [pkgly_net]

  nginx:
    image: nginx:stable
    depends_on:
      - oauth2_proxy
    volumes:
      - ./nginx.conf:/etc/nginx/conf.d/default.conf:ro
    ports:
      - 80:80
      - 443:443
    networks: [pkgly_net]

networks:
  pkgly_net:
    driver: bridge

volumes:
  pkgly_data:
```

Example `nginx.conf` forwarding public traffic through oauth2-proxy:

```nginx
server {
  listen 80;
  server_name login.example.com;

  location / {
    proxy_pass http://oauth2_proxy:4180;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
  }
}

server {
  listen 80;
  server_name pkgly.dev;

  location / {
    proxy_pass http://oauth2_proxy:4180;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
  }
}
```

