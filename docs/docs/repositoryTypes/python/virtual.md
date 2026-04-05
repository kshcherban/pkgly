# Python Virtual Repositories

Virtual Python repositories let you present several hosted or proxy Python repositories as a single endpoint. Clients point to the virtual repo URL and Pkgly resolves packages across the configured members.

## When to use
- Provide a unified `/simple/` index that fronts multiple internal hosted repositories plus selected proxies.
- Offer a stable URL for CI while rotating upstreams.
- Add a write target so publishes always land in a specific hosted repo.

## Resolution behavior
- Members are queried in **ascending priority** (then by repository name for ties).
- Only Python repositories can be members; a virtual repo cannot include itself and nested virtuals are ignored to avoid recursion.
- Successful hits are cached by path key for `cache_ttl_seconds` (default 60s). Expired or missing cache entries trigger a fresh resolution.
- Auth:
  - Reads: require read access to the virtual repository; member permissions are not re-checked.
  - Publish: requires `Write` on the configured publish target repository.

## Simple index behavior
- `GET /simple/`: lists packages from **hosted members only** (union).
- `GET /simple/<package>/`: unions the link sets from all enabled members; duplicates are removed deterministically (higher priority wins).

## Configuration fields
- **member_repositories**: array of members with:
  - `repository_id` (UUID)
  - `repository_name` (informational; filled automatically)
  - `priority` (u32, lower is tried first)
  - `enabled` (bool)
- **resolution_order**: currently only `Priority`.
- **cache_ttl_seconds**: positive integer TTL for virtual resolution cache (min 1, default 60).
- **publish_to**: optional member repository UUID; must reference an enabled hosted member. If omitted Pkgly auto-selects the first enabled hosted member (if any), otherwise publishing is rejected.

## Managing a virtual repo

### UI
1) Create a Python repository and choose **Virtual** as the type.  
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
      { "repository_id": "<uuid>", "repository_name": "python-hosted", "priority": 10, "enabled": true }
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

### pip
Point `pip` at the virtual `/simple/` index:
```ini
[global]
index-url = https://<host>/repositories/<storage>/<virtual-repo>/simple/
```

### uv
Install using the virtual repo:
```bash
uv pip install --index-url https://<host>/repositories/<storage>/<virtual-repo>/simple <package>
```

### Publishing (twine / uv publish)
Publish to the virtual repository root URL; Pkgly forwards to the configured hosted member:
```ini
[pkgly-virtual]
repository = https://<host>/repositories/<storage>/<virtual-repo>/
username = <user>
password = <password>
```

