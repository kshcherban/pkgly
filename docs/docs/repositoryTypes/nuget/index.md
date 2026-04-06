# NuGet Repository Quick Reference

Pkgly supports NuGet V3 hosted, proxy, and virtual repositories for standard `.NET` restore flows.

Supported today:
- Hosted publish via `dotnet nuget push`
- Hosted/proxy/virtual restore via `dotnet restore`
- V3 service index, flat-container package downloads, and registration documents
- Virtual repositories that merge NuGet members and forward pushes to a configured hosted member

Not implemented as a NuGet protocol feature:
- SearchQueryService
- Symbol packages / symbol server endpoints
- NuGet delete / unlist APIs

## Service Index URL

Use the repository V3 index URL as the source:

```text
https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/v3/index.json
```

## Common Commands

### Add a source

```bash
dotnet nuget add source \
  "https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/v3/index.json" \
  --name pkgly
```

### Restore from a source directly

```bash
dotnet restore --source "https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/v3/index.json"
```

### Push to a hosted repository

```bash
dotnet nuget push ./bin/Release/My.Package.1.0.0.nupkg \
  --source "https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/v3/index.json" \
  --api-key "$PKGLY_TOKEN"
```

`dotnet nuget push` sends the API key in `X-NuGet-ApiKey`; Pkgly accepts that header for NuGet publish requests.

### Push through a virtual repository

If the virtual repository has `publish_to` set to a hosted member, the same push command works against the virtual repository URL and Pkgly forwards the package to that hosted target.

## Repository Modes

### Hosted

- Stores `.nupkg` and extracted `.nuspec` files in flat-container layout
- Serves the NuGet V3 service index
- Builds registration documents from locally hosted package metadata
- Accepts multipart package publish requests at the NuGet publish endpoint

### Proxy

- Uses an upstream NuGet V3 service index
- Caches flat-container metadata, `.nupkg`, `.nuspec`, and registration JSON on demand
- Rewrites upstream registration and package URLs so clients continue talking to Pkgly

### Virtual

- Aggregates multiple NuGet repositories
- Merges flat-container version lists and registration leaves from enabled members
- Resolves exact package downloads through member repositories
- Can forward publish requests to a configured hosted member

## Important Paths

| Purpose | Path |
| --- | --- |
| Service index | `/v3/index.json` |
| Flat-container versions | `/v3/flatcontainer/<package-lower-id>/index.json` |
| Package download | `/v3/flatcontainer/<package-lower-id>/<version-lower>/<package-lower-id>.<version-lower>.nupkg` |
| Nuspec | `/v3/flatcontainer/<package-lower-id>/<version-lower>/<package-lower-id>.nuspec` |
| Registration index | `/v3/registration/<package-lower-id>/index.json` |
| Registration leaf | `/v3/registration/<package-lower-id>/<version-lower>.json` |
| Publish | `/api/v2/package` |

## Operational Notes

- Proxy repositories are cache-on-miss. Clear cached entries from the packages UI/API if you need a fresh upstream fetch.
- Virtual publish requires the caller to have write permission on the hosted `publish_to` repository.
- Package IDs are normalized to lower-case in NuGet storage paths, matching NuGet V3 client expectations.

See [NuGet Configs](./configs.md) for JSON configuration examples.
