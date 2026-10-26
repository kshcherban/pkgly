# Documentation deployment

The VitePress site deploys as the `pkgly-docs` static-assets Worker, following
the landing page's Cloudflare setup in `pkgly.dev`.

Connect this repository to the Worker using Cloudflare Workers Builds and set:

| Setting | Value |
| --- | --- |
| Worker name | `pkgly-docs` |
| Root directory | `docs` |
| Build command | `npm run docs:build` |
| Deploy command | `npx wrangler deploy` |

Select the production branch to deploy automatically when changes are pushed.
Cloudflare installs dependencies from `package-lock.json` before building.
Wrangler serves `docs/.vitepress/dist` relative to this directory, including
VitePress's generated `404.html` for missing pages.

Attach `docs.pkgly.dev` under the Worker's **Settings → Domains & Routes → Add →
Custom Domain**. The domain stays in the dashboard, matching the landing page
setup, so the build token does not need DNS permissions.

See [Workers Builds configuration](https://developers.cloudflare.com/workers/ci-cd/builds/configuration/) and [custom domains](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/).
