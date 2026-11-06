## Publishing

Use `npm publish --tag beta` to publish a prerelease without changing `latest`:

```sh
npm publish --tag beta
npm install my-package@beta
npm install my-package@1.2.0-beta.1
```

Pkgly stores the `dist-tags` from each publish request. A publish changes only the supplied tags.
If `dist-tags` is missing or an empty object, Pkgly defaults to `latest`, including for prerelease versions.
An explicit `beta` tag changes only `beta`. An explicit `latest` tag can point to a prerelease.
Pkgly does not infer a tag from a prerelease suffix.

The [npm 11+ CLI](https://github.com/npm/cli/blob/latest/lib/commands/publish.js) normally rejects prerelease publication without an explicit tag.
[Bun](https://github.com/oven-sh/bun/blob/main/src/runtime/cli/publish_command.rs) defaults to `latest` and sends that tag in the publish request.
The server cannot distinguish that default from an intentional `--tag latest`.

Use npm to manage tags after publication:

```sh
npm dist-tag ls my-package
npm dist-tag add my-package@1.2.0-beta.1 beta
npm dist-tag add my-package@1.2.0 latest
npm dist-tag rm my-package beta
```

Tag operations use repository permissions and token scopes.
See [distribution-tag authorization](routes.md#manage-distribution-tags) for the rules.

Scoped packages use the same commands with names such as `@scope/my-package`.
Exact version downloads preserve prerelease identifiers and build metadata.

Existing packages without stored tags use their most recent stable upload for `latest`.
Packages with only prereleases have no inferred `latest` tag.
Old publish tags cannot be recovered because earlier Pkgly versions discarded them.
Use `npm dist-tag add` to restore those tags.

## README and package metadata

Keep `README.md` in the package root, beside `package.json`.
Publish a new version to update the README, description, or other manifest fields.
Pkgly returns public package metadata from `latest`, or from the most recent publication when that tag is absent.
Each published version keeps its own metadata.

Use `npm deprecate` to change deprecation messages without a new publication:

```sh
npm deprecate 'my-package@1.x' 'Use version 2 instead'
npm deprecate 'my-package@1.x' ''
```

An empty message removes the warning.
See the [HTTP routes](routes.md) for supported operations and limits.

## Logging in

Use an existing Pkgly account with legacy npm login:

```sh
npm login --auth-type=legacy --registry="https://<host>/repositories/<storage>/<repository>/"
```

The client sends credentials to `PUT -/user/org.couchdb.user:<username>` and saves the returned repository token.
This endpoint authenticates an existing account. It does not create an account.
Web login is not implemented.

## Compatibility differences

Pkgly uses case-insensitive catalog paths across repository types, including npm.
Versions whose paths differ only by letter case cannot coexist in the same package.
For example, do not publish both `1.0.0-RC.1` and `1.0.0-rc.1`.
Use distinct version identifiers instead, such as `1.0.0-rc.1` and `1.0.0-rc.2`.
This differs from npmjs, which treats these version identifiers as distinct.
Pkgly preserves the version spelling in package metadata. This catalog rule does not make every npm endpoint case-insensitive.
