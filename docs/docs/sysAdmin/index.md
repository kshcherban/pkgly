# How to setup Pkgly
## Pre Install Tasks
1. Install MySQL. For more information click [here](https://docs.pkgly.dev/knowledge/InternalWorkings.html#users).
2. Create a database. For pkgly to use
## Getting your build
Please use one of the following options for your build
1. Latest [Release](https://github.com/kshcherban/pkgly/releases) on Github
2. Latest [Build](https://github.com/kshcherban/pkgly/actions/workflows/push.yml) on Github
3. Build yourself. Instructions are [here](https://docs.pkgly.dev/compiling.html).  
   **Linux build prerequisites:** install `pkg-config` and the OpenSSL development headers (`libssl-dev` on Debian/Ubuntu, `openssl-devel` on Fedora/RHEL) before running `cargo build`.

## Setup
1. Decompress the build inside your install directory. I use `/opt/pkgly`. Using the command `tar -xf pkgly.tar.gz` Note: You might have to decompress the zip for Github Latest Builds
2. Run `./pkgly --install` Follow the CLI for installation. 
3. After completing the installation go ahead and run ./pkgly again. To ensure proper setup. Connect to it over the browser. Using your host and port set
4. Edit other/pkgly.service to use the appropriate location of your installation. Then copy the pkgly.service to the service directory Command: `cp other/pkgly.service /etc/systemd/system/pkgly.service`
5. Run `systemctl daemon-reload` and `systemctl start pkgly.service`
### SSL
After installation you can add SSL

Edit cfg/pkgly.toml

Under the application section

Add

```toml
ssl_private_key=
ssl_cert_key=
```

Make sure to specify values

#### For Lets Encrypt 

```toml
ssl_private_key='/etc/letsencrypt/live/{domain}/privkey.pem'
ssl_cert_key='/etc/letsencrypt/live/{domain}/cert.pem'
```
### 

Finally Restart Pkgly

## Storage Backends

- [Configuring S3 Storage](./s3.md) — steps for attaching Pkgly to an S3 or S3-compatible bucket.
- [Package Webhooks](./webhooks.md) — configure outbound publish/delete notifications and delivery retries.

## Enabling SSO Login
Pkgly can delegate authentication to an upstream SSO provider (Cloudflare Access, Okta, Auth0, etc.) that issues signed JWT/ID tokens. Configure the security section in `cfg/pkgly.toml` to enable the feature:

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
