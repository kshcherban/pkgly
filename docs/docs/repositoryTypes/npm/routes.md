# NPM Registry HTTP Routes

Pkgly implements a subset of the npm registry API.
All paths below are relative to `/repositories/<storage>/<repository>/`.
For scoped package metadata, npm encodes the slash, for example `@scope%2Fmy-package`.
Tarball paths can contain the unencoded scope and package name.

## Package reads

Hosted repositories support these routes. HEAD returns the same status and headers without a response body:

| Method | Path | Result |
| --- | --- | --- |
| GET | `<name>` | Package metadata, versions, README, and distribution tags |
| GET | `<name>/<version>` | Metadata for one exact version |
| GET | `<name>/<tag>` | Metadata for the version selected by a tag |
| GET | `<name>/-/<basename>-<version>.tgz` | Package archive |

For `@scope/my-package`, the basename is `my-package`.
Prerelease identifiers and build metadata remain part of the exact version.
Use authentication headers when the repository requires authentication.

```sh
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/@scope%2Fmy-package"
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/@scope%2Fmy-package/beta"
curl -H "Authorization: Bearer <token>" \
     -O "https://<host>/repositories/<storage>/<repository>/@scope/my-package/-/my-package-1.2.0-beta.1.tgz"
```

An exact-version response includes the manifest and download information. For example:

```json
{
  "_id": "@scope/my-package@1.2.0-beta.1",
  "name": "@scope/my-package",
  "version": "1.2.0-beta.1",
  "dist": {
    "integrity": "sha512-...",
    "shasum": "...",
    "tarball": "https://<host>/repositories/<storage>/<repository>/@scope/my-package/-/my-package-1.2.0-beta.1.tgz"
  }
}
```

Proxy repositories cache upstream metadata and tarballs.
Virtual repositories resolve reads through their configured members.
They forward publication, deprecation, and tag changes to their configured hosted target.

## Publish a new version

Use `npm publish` with the repository URL as the registry:

```sh
npm publish --registry="https://<host>/repositories/<storage>/<repository>/" --tag beta
```

The client sends `PUT <name>` with `Npm-Command: publish`.
Its JSON body contains the package `name`, one entry in `versions`, and base64 tarball data in `_attachments`.
Each version includes its manifest, `dist` hashes and tarball URL, `_id`, `_nodeVersion`, and `_npmVersion`.
The tarball URL must point into the target storage and repository.
Metadata and archive upload form one publish request. Pkgly does not support a separate archive upload endpoint.

A publication must contain exactly one SemVer version and one attachment.
The package name in the route must match the publish body and manifest.
The attachment and advertised download URL must use the tarball filename for that name and version.
Pkgly decodes the attachment before creating catalog entries.
Invalid attachment data, empty attachments, mismatched versions, and invalid distribution tags return `400 Bad Request`.

The `dist-tags` object controls which tags change.
Missing or empty tags default to `latest`, including for prereleases.
An explicit `beta` tag leaves `latest` unchanged. An explicit `latest` can select a prerelease.
See [Publishing](standard.md#publishing) for client behavior and examples.

### Publish with HTTP

Use this request shape for custom clients. Replace the hash, attachment data, and length placeholders with values from the tarball. The attachment length is the number of bytes before base64 encoding.

```sh
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -H "Npm-Command: publish" \
     --data-binary @publish.json \
     "https://<host>/repositories/<storage>/<repository>/my-package"
```

```json
{
  "name": "my-package",
  "dist-tags": {"beta": "1.2.0-beta.1"},
  "versions": {
    "1.2.0-beta.1": {
      "_id": "my-package@1.2.0-beta.1",
      "name": "my-package",
      "version": "1.2.0-beta.1",
      "_nodeVersion": "22.0.0",
      "_npmVersion": "11.0.0",
      "dist": {
        "integrity": "sha512-<base64-sha512>",
        "shasum": "<hex-sha1>",
        "tarball": "https://<host>/repositories/<storage>/<repository>/my-package/-/my-package-1.2.0-beta.1.tgz"
      }
    }
  },
  "_attachments": {
    "my-package-1.2.0-beta.1.tgz": {
      "content_type": "application/octet-stream",
      "data": "<base64-tarball>",
      "length": 123
    }
  }
}
```

## Update package metadata

For README or ordinary manifest changes, edit the package files and publish a new version.
The README normally comes from `README.md` beside `package.json`.
Pkgly stores README content supplied in the version metadata.
If that content is absent, it accepts `readme` and `readmeFilename` from the root of the publish body.

The package response exposes the README and public metadata from the version selected by `latest`.
If there is no `latest` tag, it uses the most recent published version.
Public fields include description, author, contributors, license, keywords, homepage, bugs, repository, and maintainers.
Older versions keep their own metadata.

```sh
npm version patch
npm publish --registry="https://<host>/repositories/<storage>/<repository>/"
npm view my-package readme --registry="https://<host>/repositories/<storage>/<repository>/"
```

Pkgly does not offer arbitrary edits to an existing version's manifest or a standalone package README update.
The separate operations below change tags and deprecation messages without a new publication.

## Manage distribution tags

| Method | Path | Request body |
| --- | --- | --- |
| GET | `-/package/<name>/dist-tags` | None |
| PUT | `-/package/<name>/dist-tags/<tag>` | A JSON string with an existing version |
| DELETE | `-/package/<name>/dist-tags/<tag>` | None |

For scoped packages, `<name>` includes the scope, such as `@scope/my-package`.
Tag reads from hosted repositories require Read permission when repository authentication is enabled or visibility is Private or Hidden.
Tokens must also grant Read access to that repository.
Public proxy and virtual repositories continue to allow anonymous tag reads, even when repository authentication is enabled.
Private and Hidden proxy and virtual repositories require Read permission and Read access in the token for tag reads.

Tag changes require Write permission and Write access in the token. They do not require Read permission.
These checks apply to the distribution-tag endpoints. Existing package metadata, version, and tarball read rules stay unchanged.
Tag names preserve case and cannot be semantic-version ranges.
They can contain ASCII letters, digits, and `- _ . ! ~ * ' ( )`.
Unicode, spaces, percent signs, and path separators are invalid tag characters.
A tag such as `release*candidate` is valid, while standalone `*` is a version range and is invalid.
Request paths are percent-decoded once before tag validation.
To change `latest`, assign it to another version. Direct removal of `latest` returns `400 Bad Request`.

```sh
npm dist-tag ls my-package
npm dist-tag add my-package@1.2.0-beta.1 beta
npm dist-tag rm my-package beta
```

The equivalent HTTP request to set a tag is:

```sh
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '"1.2.0-beta.1"' \
     "https://<host>/repositories/<storage>/<repository>/-/package/my-package/dist-tags/beta"
```

## Deprecate and undeprecate versions

Use the npm CLI to select a version or version range:

```sh
npm deprecate 'my-package@1.x' 'Use version 2 instead'
npm deprecate 'my-package@1.x' ''
```

The CLI first reads the package document with `GET <name>?write=true`.
It sets `deprecated` on the matching versions, then sends the document to `PUT <name>` with `Npm-Command: deprecate`.
Pkgly also accepts `Npm-Command: undeprecate` for clients with a separate undeprecate command.
An empty message clears the warning.

Pkgly changes only the supplied versions' `deprecated` fields.
It preserves their other metadata, tarballs, and distribution tags.
The entire update fails if a target version is missing or a deprecation message is not a string.
These changes require repository write permission.

## Repository management API

These Pkgly management endpoints are separate from the npm registry protocol:

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/repository/<repository-id>/packages?page=1&per_page=50` | List stored packages |
| DELETE | `/api/repository/<repository-id>/packages` | Delete paths returned by the package list |
| GET | `/api/repository/<repository-id>/config/npm` | Read npm repository configuration |
| PUT | `/api/repository/<repository-id>/config/npm` | Change npm repository configuration |

Deletion requires repository edit permission and a JSON body such as `{"paths":["<path-from-package-list>"]}`.
It is not an implementation of `npm unpublish`.
When Pkgly deletes a version through this API or package retention, it clears any tags that point to it.
If that includes `latest`, Pkgly does not promote another version. Assign a new `latest` if needed.

```sh
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/packages?page=1&per_page=50"
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/npm"
```

To delete a listed path or update the npm configuration:

```sh
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     --data '{"paths":["<path-from-package-list>"]}' \
     "https://<host>/api/repository/<repository-id>/packages"
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     --data-binary @config.json \
     "https://<host>/api/repository/<repository-id>/config/npm"
```

A proxy configuration uses the following shape:

```json
{
  "type": "Proxy",
  "config": {
    "routes": [
      {"url": "https://registry.npmjs.org", "name": "NPM Official"}
    ]
  }
}
```

## Authentication and unsupported operations

Use a Pkgly repository token in `.npmrc`, or log in with an existing Pkgly account:

```sh
npm login --auth-type=legacy --registry="https://<host>/repositories/<storage>/<repository>/"
```

For direct package reads, use a repository token or Basic authentication with an existing Pkgly account:

```sh
curl -H "Authorization: Bearer <token>" \
     "https://<host>/repositories/<storage>/<repository>/my-package"
curl -u "<username>" \
     "https://<host>/repositories/<storage>/<repository>/my-package"
```

With only the username supplied, curl prompts for the password.

Legacy login uses `PUT -/user/org.couchdb.user:<username>` and returns a repository token.
It does not register a new Pkgly account. Web login is not implemented.

Hosted repositories do not currently implement npm unpublish, registry search, registry statistics, `whoami`, or CouchDB session endpoints.
A root registry information document and an attachments listing endpoint are also not implemented.
These limits do not prevent package publication, installation, tag management, or deprecation through npm.
