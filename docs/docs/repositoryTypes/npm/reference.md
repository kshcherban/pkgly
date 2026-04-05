# NPM Registry Quick Reference

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
  "proxy": {
    "routes": [
      {
        "url": "https://registry.npmjs.org",
        "name": "NPM Registry",
        "priority": 1
      }
    ]
  }
}
```

### Multi-Route Proxy Repository
```json
{
  "type": "Proxy",
  "proxy": {
    "routes": [
      {
        "url": "https://registry.npmjs.org",
        "name": "NPM Official",
        "priority": 10
      },
      {
        "url": "https://registry.yarnpkg.com",
        "name": "Yarn Registry",
        "priority": 5
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

### Add User/Configure Authentication
```bash
# Add user to the registry
npm adduser --registry https://your-pkgly.example.com/repositories/storage/npm-repo/
# Follow prompts to enter username, password, and email

# Or login with existing credentials
npm login --registry https://your-pkgly.example.com/repositories/storage/npm-repo/
# Enter username, password, and email when prompted
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
```bash
# Search packages
npm search my-search-term --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

# Search with JSON output
npm search my-search-term --json --registry https://your-pkgly.example.com/repositories/storage/npm-repo/
```

## Publishing Workflows

### Standard Package Publishing
```bash
# Initialize package
npm init

# Set registry in package.json
npm pkg set publishConfig.registry=https://your-pkgly.example.com/repositories/storage/npm-repo/

# Login
npm login --registry https://your-pkgly.example.com/repositories/storage/npm-repo/

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

| Description | Endpoint | Example |
|-------------|----------|---------|
| Root Registry | `GET /` | Registry information |
| Package Info | `GET /package/{name}` | Package metadata and versions |
| Package Version | `GET /package/{name}/{version}` | Specific version info |
| Download Tarball | `GET /package/{name}/{version}/{file}` | Download package tarball |
| Publish Package | `PUT /` | Upload new package version |
| User Info | `GET /-/whoami` | Get current user info |
| User Auth | `GET /-/user/org.couchdb.user:{username}` | User authentication |

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

### Test Registry Connectivity
```bash
# Check if registry is accessible
curl -I https://your-pkgly.example.com/repositories/storage/npm-repo/

# Get registry info
curl https://your-pkgly.example.com/repositories/storage/npm-repo/
```

### Test Authentication
```bash
# Test with token authentication
curl -H "Authorization: Bearer $PKGLY_NPM_TOKEN" \
     https://your-pkgly.example.com/repositories/storage/npm-repo/-/whoami

# Test package access
curl -H "Authorization: Bearer $PKGLY_NPM_TOKEN" \
     https://your-pkgly.example.com/repositories/storage/npm-repo/package/my-package
```

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