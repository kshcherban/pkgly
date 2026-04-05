# NPM Virtual Repositories

Virtual NPM repositories let you present several hosted or proxy NPM repositories as a single endpoint. Clients point to the virtual repo URL and Pkgly resolves packages across the configured members.

## When to use
- Provide a unified feed that fronts multiple internal hosted registries plus selected public proxies.
- Offer a stable URL for CI while rotating upstreams.
- Add a write target so `npm publish` always lands in a specific hosted repo.

## Resolution behavior
- Members are queried in **ascending priority** (then by repository name for ties).
- Only NPM repositories can be members; a virtual repo cannot include itself and nested virtuals are ignored to avoid recursion.
- Successful hits are cached by package/version key for `cache_ttl_seconds` (default 60s). Expired or missing cache entries trigger a fresh resolution.
- Auth:
  - Reads: require read access to the virtual repository; member permissions are not re-checked.
  - Publish: requires `Write` on the configured publish target repository.

## Configuration fields
- **member_repositories**: array of members with:
  - `repository_id` (UUID)
  - `repository_name` (informational; filled automatically)
  - `priority` (u32, lower is tried first)
  - `enabled` (bool)
- **resolution_order**: currently only `Priority`.
- **cache_ttl_seconds**: positive integer TTL for resolution cache (min 1, default 60).
- **publish_to**: optional member repository UUID; must reference an enabled hosted member. If omitted Pkgly auto-selects the first enabled hosted member (if any), otherwise publishing is rejected.

## Managing a virtual repo

### UI
1) Create an NPM repository and choose **Virtual** as the type.  
2) Add members, set priorities, and (optionally) choose a publish target.  
3) Adjust cache TTL and resolution order if needed.  
4) Save.

### API
- **List config & members**  
  `GET /api/repository/{id}/virtual/members`
- **Replace members / update settings**  
  `POST /api/repository/{id}/virtual/members` with body:
  ```json
  {
    "members": [
      { "repository_id": "<uuid>", "repository_name": "npm-hosted", "priority": 10, "enabled": true }
    ],
    "resolution_order": "Priority",
    "cache_ttl_seconds": 120,
    "publish_to": "<uuid of hosted member>"
  }
  ```
- **Update only resolution settings**  
  `PUT /api/repository/{id}/virtual/resolution-order` with:
  ```json
  {
    "resolution_order": "Priority",
    "cache_ttl_seconds": 300,
    "publish_to": "<uuid>"
  }
  ```

## Client usage
Point npm/yarn/pnpm to the virtual repository URL:
```ini
//<host>/repositories/<storage>/<virtual-repo>/:_authToken=<token>
registry=https://<host>/repositories/<storage>/<virtual-repo>/
always-auth=true
```
Publishing uses the same URL; Pkgly forwards to the configured hosted member.
