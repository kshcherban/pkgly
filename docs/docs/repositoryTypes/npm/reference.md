# NPM Registry Quick Reference

## Package metadata

Hosted package metadata returns the complete package name in both `_id` and
`name`. For a scoped package such as `@team/library`, both fields include the
scope; unscoped package names are returned as published. Metadata requests must
include credentials when repository authentication is enabled.

## Configuration Templates

### Hosted Repository
```json
{
  "type": "Hosted"
}
```

### Proxy Repository
```json
{
  "type": "Proxy",
  "config": {
    "routes": [
      {
        "url": "https://registry.npmjs.org",
        "name": "NPM Registry"
      }
    ]
  }
}
```

### Multi-Route Proxy Repository
```json
{
  "type": "Proxy",
  "config": {
    "routes": [
      {
        "url": "https://registry.npmjs.org",
        "name": "NPM Official"
      },
      {
        "url": "https://registry.yarnpkg.com",
        "name": "Yarn Registry"
      }
    ]
  }
}
```

## Essential Commands

### Configure NPM Registry
```bash
# Set the registry for your project
npm config set registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Set the registry globally
npm config set registry https://your-pkgly.example.com/repositories/storage/npm-repo/ --global

# Verify the registry configuration
npm config get registry
```

### Configure Authentication

Log in with an existing Pkgly account. This command does not register a new account.

```sh
npm login --auth-type=legacy --registry https://your-pkgly.example.com/repositories/storage/npm-repo/
```

### Configure in .npmrc
Add to project `.npmrc` or user `~/.npmrc`:
```
registry=https://your-pkgly.example.com/repositories/storage/npm-repo/
//your-pkgly.example.com/repositories/storage/npm-repo/:_authToken=${PKGLY_NPM_TOKEN}
//your-pkgly.example.com/repositories/storage/npm-repo/:always-auth=true
```

### Publish Package
```bash
# Prepare and publish
npm publish --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Publish with scoped package
npm publish --access public

# Publish with specific tag
npm publish --tag beta
```

### Install Package
```bash
# Install from the private registry
npm install my-package --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Install global package
npm install -g my-cli-tool --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Install with authentication token
npm install my-package
```

### Package Information
```bash
# View package info
npm view my-package --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# View specific version
npm view my-package@1.0.0 --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# List all versions
npm view my-package versions --registry https://your-pkgly.example.com/repositories/storage/npm-repo/
```

### Search Packages

Hosted registries do not currently implement `npm search`.
Use the Pkgly package list to find stored packages.

## Publishing Workflows

### Standard Package Publishing
```bash
# Initialize package
npm init

# Set registry in package.json
npm pkg set publishConfig.registry=https://your-pkgly.example.com/repositories/storage/npm-repo/

# Login
npm login --auth-type=legacy --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Publish
npm publish
```

### Scoped Package Publishing
```bash
# Initialize scoped package
npm init --scope=@mycompany

# Package.json will include the scope
{
  "name": "@mycompany/my-package",
  "version": "1.0.0",
  "publishConfig": {
    "registry": "https://your-pkgly.example.com/repositories/storage/npm-repo/"
  }
}

# Publish scoped package
npm publish --access public
```

### CI/CD Integration (GitHub Actions)
```yaml
name: Publish NPM Package

on:
  push:
    tags: ['v*']

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '18'
          registry-url: 'https://your-pkgly.example.com/repositories/storage/npm-repo/'

      - name: Install dependencies
        run: npm ci

      - name: Run tests
        run: npm test

      - name: Publish package
        run: npm publish
        env:
          NODE_AUTH_TOKEN: ${{ secrets.PKGLY_NPM_TOKEN }}
```

### Package.json Configuration
```json
{
  "name": "my-package",
  "version": "1.0.0",
  "description": "My awesome package",
  "main": "index.js",
  "scripts": {
    "test": "jest",
    "publish:pkgly": "npm publish --registry https://your-pkgly.example.com/repositories/storage/npm-repo/"
  },
  "publishConfig": {
    "registry": "https://your-pkgly.example.com/repositories/storage/npm-repo/"
  },
  "repository": {
    "type": "git",
    "url": "git+https://github.com/username/my-package.git"
  },
  "keywords": ["awesome", "package"],
  "author": "Your Name",
  "license": "MIT",
  "dependencies": {
    "lodash": "^4.17.21"
  },
  "devDependencies": {
    "jest": "^29.0.0"
  }
}
```

## Common Endpoints

Paths are relative to `/repositories/<storage>/<repository>/`.

| Description | Endpoint |
| --- | --- |
| Package metadata | `GET <name>` |
| Exact version or tag | `GET <name>/<version-or-tag>` |
| Package tarball | `GET <name>/-/<basename>-<version>.tgz` |
| Publish a version | `PUT <name>` with `Npm-Command: publish` |
| Deprecate versions | `PUT <name>` with `Npm-Command: deprecate` |
| List tags | `GET -/package/<name>/dist-tags` |
| Set a tag | `PUT -/package/<name>/dist-tags/<tag>` |
| Remove a tag | `DELETE -/package/<name>/dist-tags/<tag>` |
| Legacy login | `PUT -/user/org.couchdb.user:<username>` |

See the [HTTP route reference](routes.md) for request bodies, authentication, and unsupported operations.

## Yarn Integration

### Configure Yarn Registry
```bash
# Set registry
yarn config set registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Add authentication token
yarn config set 'https://your-pkgly.example.com/repositories/storage/npm-repo/:_authToken' "$PKGLY_NPM_TOKEN"

# Verify configuration
yarn config list
```

### Yarn .yarnrc.yml Configuration
```yaml
npmRegistryServer: "https://your-pkgly.example.com/repositories/storage/npm-repo/"
npmAuthToken: "${PKGLY_NPM_TOKEN}"
```

### Install and Publish with Yarn
```bash
# Install dependencies
yarn install

# Publish package
yarn npm publish

# Publish scoped package
yarn npm publish --access public
```

## Troubleshooting Commands

### Test Package Access

Use the name of a package that exists in the repository:

```sh
curl -H "Authorization: Bearer $PKGLY_NPM_TOKEN" \
     https://your-pkgly.example.com/repositories/storage/npm-repo/my-package
```

The root registry information document and `whoami` endpoint are not implemented by hosted repositories.

### Debug Publishing Issues
```bash
# Verbose publish output
npm publish --verbose

# Dry run publish (doesn't actually publish)
npm publish --dry-run

# Check package contents before publishing
npm pack --dry-run
```

### Common NPM Issues
```bash
# Clear npm cache
npm cache clean --force

# Verify npm configuration
npm config list

# Check package integrity
npm audit

# Fix permission issues
npm cache verify
```

## Configuration Options

| Setting | Default | Description |
|---------|---------|-------------|
| Proxy Routes | None | Upstream registries for proxy mode |
| Cache TTL | Default | Package metadata caching duration |
| Priority | 0 | Route priority in proxy mode |

## Performance Tips

### For Large Packages
- Use `.npmignore` to exclude unnecessary files
- Optimize `package.json` dependencies
- Use scoped packages for better organization

### For Proxy Mode
- Configure multiple upstream registries for reliability
- Set appropriate cache TTL based on update frequency
- Monitor cache hit rates

### For CI/CD
- Use authentication tokens instead of user credentials
- Cache node_modules between builds
- Use `npm ci` for faster, reproducible installs

## Security Checklist

- [ ] Use HTTPS for all registry communications
- [ ] Enable authentication for package publishing
- [ ] Use scoped packages for private packages
- [ ] Implement access control for package publishing
- [ ] Regularly audit published packages
- [ ] Use tokens for CI/CD automation
- [ ] Monitor registry access logs
- [ ] Enable package scanning for security vulnerabilities

## Version Management

### Semantic Versioning
```json
{
  "name": "my-package",
  "version": "1.2.3",
  "scripts": {
    "version:patch": "npm version patch",
    "version:minor": "npm version minor",
    "version:major": "npm version major",
    "publish:patch": "npm version patch && npm publish",
    "publish:minor": "npm version minor && npm publish",
    "publish:major": "npm version major && npm publish"
  }
}
```

### Publishing Different Tags
```bash
# Publish to latest tag (default)
npm publish

# Publish to beta tag
npm publish --tag beta

# Publish to next tag
npm publish --tag next

# Install specific tag
npm install my-package@beta
```

## Workspace Integration

### Lerna/Nx Workspaces
```json
// lerna.json
{
  "version": "independent",
  "npmClient": "npm",
  "registry": "https://your-pkgly.example.com/repositories/storage/npm-repo/",
  "command": {
    "publish": {
      "registry": "https://your-pkgly.example.com/repositories/storage/npm-repo/"
    }
  }
}
```

### Monorepo Publishing
```bash
# Publish all packages in workspace
lerna publish --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Publish packages with version changes only
lerna publish from-package --registry https://your-pkgly.example.com/repositories/storage/npm-repo/
```

## Storage Layout

```
@scope/
└── pkg/
    └── my-package/
        └── 1.0.0/
            ├── my-package-1.0.0.tgz
            └── my-package-1.0.0.json
my-package/
└── 1.0.0/
    ├── my-package-1.0.0.tgz
    └── my-package-1.0.0.json
```

---

*Quick reference for NPM registry configuration and usage. See [NPM Route Reference](routes.md) for detailed API documentation.*
