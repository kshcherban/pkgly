# MCP Server

`pkglyctl mcp serve` exposes all 38 CLI actions as individual Model Context Protocol tools over stdio. Your MCP client starts the process, discovers its tools, and invokes them using typed JSON arguments.

## Build and connect

Build the CLI from the repository root:

```bash
cargo build -p pkgly-cli
```

Configure a client that supports stdio MCP servers, using an absolute binary path:

```json
{
  "mcpServers": {
    "pkgly": {
      "command": "/absolute/path/to/pkgly/target/debug/pkglyctl",
      "args": ["mcp", "serve"],
      "env": {
        "PKGLY_URL": "https://packages.example.com",
        "PKGLY_TOKEN": "your-scoped-token"
      }
    }
  }
}
```

To use an existing CLI profile, set `args` to `["--profile", "local", "mcp", "serve"]`. `--config` selects a different configuration file. The Docker image also includes `/app/pkglyctl`.

Configuration follows the CLI's precedence: startup flags, environment variables, then profile values. Each call reloads the configuration file. Calls run sequentially so credential and profile mutations complete before subsequent calls execute. Startup overrides continue to take precedence: for example, `profile_use` does not override a profile explicitly selected at startup, and `auth_logout` does not remove a token supplied through the environment.

The process writes only MCP messages to stdout; diagnostics use stderr. Closing stdin shuts down the server. MCP output is always JSON, regardless of the CLI's `--output` flag.

## Available tools

Tool names follow CLI command paths, with underscores replacing spaces and hyphens. Each tool's input schema defines its parameters, required fields, and defaults.

| Group | Tools |
| --- | --- |
| Authentication | `auth_login`, `auth_set_token`, `auth_whoami`, `auth_logout` |
| Profiles | `profile_list`, `profile_show`, `profile_use`, `profile_remove` |
| Storage | `storage_list`, `storage_get`, `storage_create`, `storage_delete` |
| Repositories | `repo_list`, `repo_get`, `repo_id`, `repo_create`, `repo_delete`, `repo_config_list`, `repo_config_get`, `repo_config_set`, `repo_url` |
| Packages | `package_list`, `package_describe`, `package_search`, `package_browse`, `package_download`, `package_delete` |
| Uploads | `package_upload_maven`, `package_upload_python`, `package_upload_go`, `package_upload_php`, `package_upload_deb`, `package_upload_helm`, `package_upload_ruby`, `package_upload_nuget` |
| Native commands | `native_npm`, `native_cargo`, `native_docker` |

Repository references accept UUIDs or `storage/repository`, following the CLI. Native command tools require `storage/repository`; they return package-manager commands as text. `package_browse` returns a repository route URL.

The repository creation API returns HTTP 201 with `storage_name` alongside the repository fields, matching the association required by CLI and MCP clients.

For example, call `package_search` with:

```json
{"query": "type:maven package:example version:>1.0.0", "limit": 25}
```

Search uses the [existing query syntax](../knowledge/search#query-syntax), with a default limit of 25. Config parameters retain the CLI's JSON-string format:

```json
{"repository": "local/maven", "key": "auth", "value": "{\"enabled\":true}"}
```

Object results appear directly in `structuredContent`; lists appear as `{"items": [...]}` and CLI messages as `{"message": "..."}`. Each result also includes equivalent JSON text content. Tool execution failures set `isError: true` and return a message, including the HTTP status when available.

## Authentication and authorization

The server uses the configured API token as supplied. Discovery does not require credentials, and all tools remain discoverable even when the current identity cannot execute them. Credentials are never created automatically.

Repository authorization requires both the user's permissions and the token's global or repository-specific scopes:

| Global scope | Repository action |
| --- | --- |
| `ReadRepository` | Read protected repository details, package listings, downloads, and search results |
| `WriteRepository` | Upload package content |
| `EditRepository` | Edit repository configuration and delete package content; some configuration reads also require Edit |

Repository-specific token scopes use `Read`, `Write`, and `Edit` for individual repository UUIDs. Search filters protected repositories using the same checks as package listings. Existing anonymous access rules for public repositories still apply.

Artifact operations using a repository UUID first read repository metadata to resolve its storage and name. Use `storage/repository` references with write-only tokens to avoid requiring that metadata read.

Storage operations and role-only repository management operations, including repository creation and deletion, retain the backend's administrator/system-manager policy. Repository token scopes do not restrict those role-only operations. Use a non-administrator account when those management capabilities should be unavailable.

`auth_login` requires an explicit `password` argument; it cannot prompt on MCP stdin. It uses the CLI login flow to store a token with `ReadRepository`, `WriteRepository`, and `EditRepository`. For narrower access, create a scoped token separately and supply it through configuration or `auth_set_token`. Passwords are not persisted, and profile results mask stored tokens.

## Files and deletion

Upload arguments refer to existing files on the machine running `pkglyctl`. Downloads require an explicit `output_file` on that machine and return `{"path": "...", "bytes": 123}` after saving the file. A process running in Docker uses paths inside its container.

Repository and package deletion retain `yes` and `force` arguments. Set `yes: true` to confirm; `force: true` is also accepted with noninteractive stdin. Storage deletion requires `yes: true` and cascades to its repositories and contents. Tool annotations identify read-only and destructive actions for MCP clients.

## Validation

Protocol tests launch the actual CLI subprocess:

```bash
cargo test -q -p pkgly-cli
```

The Docker-backed suite uses real Pkgly APIs, PostgreSQL, package uploads/downloads, scoped search, and denied mutations. It requires Docker Compose and Python 3 on the host:

```bash
./tests/run_integration_tests.sh mcp
```
