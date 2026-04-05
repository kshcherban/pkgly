# Go Proxy Repository Setup and Usage Guide

This guide covers how to configure and use Pkgly as a Go module proxy for caching and serving Go modules.

## Overview

Pkgly supports two modes for Go module repositories:

- **Hosted Mode**: Store and serve Go modules directly from Pkgly storage
- **Proxy Mode**: Cache and proxy Go modules from upstream Go module proxies (like proxy.golang.org)

## Quick Start

### 1. Create a Go Repository

1. Navigate to the Pkgly web interface
2. Click "Create Repository"
3. Select "Go" as the repository type
4. Choose between "Hosted" or "Proxy" mode
5. Configure your repository settings
6. Click "Create Repository"

### 2. Configure Go to Use Pkgly

Set the `GOPROXY` environment variable to point to your Pkgly instance:

```bash
export GOPROXY=https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>,https://proxy.golang.org,direct
```

Or configure it in your shell profile (`~/.bashrc`, `~/.zshrc`, etc.):

```bash
echo 'export GOPROXY=https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>,https://proxy.golang.org,direct' >> ~/.bashrc
source ~/.bashrc
```

## Configuration Options

### Hosted Repository

A hosted repository stores Go modules directly in Pkgly's storage. This is ideal for:

- Private Go modules within your organization
- Custom Go packages not available publicly
- Full control over module versions and access

**Configuration:**
- Repository Type: **Hosted**
- No additional configuration required

### Proxy Repository

A proxy repository caches modules from upstream Go proxies. This is ideal for:

- Improving download speeds for commonly used modules
- Reducing external dependency on public proxies
- Providing fallback proxy services
- Offline access to cached modules

**Configuration:**
- Repository Type: **Proxy**
- **Cache TTL (seconds)**: How long to cache module responses (default: 3600)
- **Upstream Routes**: List of proxy servers to fetch modules from, in priority order

#### Proxy Route Configuration

Each upstream route requires:

- **URL**: The upstream proxy server URL
- **Display Name**: Optional label for the route
- **Priority**: Higher numbers = higher priority (tried first)

**Example Configuration:**

```json
{
  "type": "Proxy",
  "config": {
    "go_module_cache_ttl": 7200,
    "routes": [
      {
        "url": "https://proxy.golang.org",
        "name": "Go Official Proxy",
        "priority": 10
      },
      {
        "url": "https://goproxy.cn",
        "name": "China Proxy",
        "priority": 5
      },
      {
        "url": "https://goproxy.io",
        "name": "Alternative Proxy",
        "priority": 1
      }
    ]
  }
}
```

## Go Module Protocol Support

Pkgly supports the standard Go module proxy protocol endpoints:

### Version List
- **Endpoint**: `GET /{module}/@v/list`
- **Description**: Returns all available versions of a module
- **Example**: `https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/github.com/user/repo/@v/list`

### Version Info
- **Endpoint**: `GET /{module}/@v/{version}.info`
- **Description**: Returns metadata about a specific version
- **Example**: `https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/github.com/user/repo/@v/v1.2.3.info`

### Go Module File
- **Endpoint**: `GET /{module}/@v/{version}.mod`
- **Description**: Returns the go.mod file for a specific version
- **Example**: `https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/github.com/user/repo/@v/v1.2.3.mod`

### Module Zip
- **Endpoint**: `GET /{module}/@v/{version}.zip`
- **Description**: Returns the complete module source as a zip file
- **Example**: `https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/github.com/user/repo/@v/v1.2.3.zip`

### Latest Version
- **Endpoint**: `GET /{module}/@latest`
- **Description**: Returns info about the latest version
- **Example**: `https://your-pkgly.example.com/repositories/<storage-name>/<repository-name>/github.com/user/repo/@latest`

## Usage Examples

### Basic Go Module Usage

Once your Go repository is configured and `GOPROXY` is set, use Go commands as normal:

```bash
# Initialize a new module
go mod init myproject

# Add a dependency
go get github.com/gin-gonic/gin@v1.9.1

# Download dependencies
go mod download

# Build your project
go build

# Run tests
go test
```

### Private Go Modules

For private Go modules in hosted repositories:

1. **Configure Authentication** (if required):
   ```bash
   git config --global url."https://user:token@your-pkgly.example.com".insteadOf "https://your-pkgly.example.com"
   ```

2. **Use the module**:
   ```bash
   go get your-pkgly.example.com/repositories/<storage-name>/<repository-name>/private/module@v1.0.0
   ```

### Offline Development

With proxy mode and sufficient cache time, you can work offline:

```bash
# Ensure modules are cached
go mod download

# Work offline (modules served from cache)
go build
```

## Authentication and Access Control

### Repository Permissions

Pkgly integrates with your existing authentication system:

- **Public repositories**: Accessible to all users
- **Private repositories**: Require authentication
- **Custom permissions**: Control who can read/publish modules

### Configuring Authentication

1. Enable authentication in repository settings
2. Configure user permissions
3. Set up API tokens for automated access
4. Configure Git credentials for private modules

## Monitoring and Troubleshooting

### Viewing Repository Statistics

In the Pkgly interface:

1. Navigate to your Go repository
2. View download statistics, cache hit rates, and error rates
3. Monitor proxy route performance
4. Check storage usage

### Common Issues

#### GOPROXY Not Working

**Symptom**: `go get` fails with module not found errors

**Solutions**:
1. Verify your `GOPROXY` environment variable is set correctly
2. Check that your Pkgly instance is accessible
3. Verify repository permissions and authentication
4. Check Pkgly logs for errors

#### Slow Module Downloads

**Symptom**: Modules download slowly despite using a proxy

**Solutions**:
1. Check upstream proxy route priorities
2. Verify network connectivity to upstream proxies
3. Consider increasing cache TTL for frequently used modules
4. Monitor proxy route performance in the dashboard

#### Module Not Cached

**Symptom**: Modules are always fetched from upstream, not cache

**Solutions**:
1. Check cache TTL settings
2. Verify storage permissions and available space
3. Check for cache invalidation settings
4. Monitor cache hit rates in the dashboard

#### Authentication Issues

**Symptom**: 401 Unauthorized errors when accessing private modules

**Solutions**:
1. Verify user permissions in Pkgly
2. Check API token configuration
3. Ensure proper Git credential setup
4. Verify repository authentication settings

### Debug Mode

Enable debug logging for detailed troubleshooting:

```bash
# Set log level
export PKGLY_REPO_LOG_LEVEL=debug

# Restart Pkgly service
```

## Performance Optimization

### Cache Tuning

- **Short TTL (300-1800 seconds)**: For fast-changing modules
- **Medium TTL (3600-7200 seconds)**: Default for most use cases
- **Long TTL (86400+ seconds)**: For stable dependencies and offline access

### Proxy Route Configuration

- **Primary proxy**: High priority (8-10), high availability
- **Secondary proxies**: Medium priority (3-7), for redundancy
- **Fallback proxies**: Low priority (1-2), for reliability

### Storage Optimization

- Monitor storage usage regularly
- Set up storage quotas if needed
- Consider cache cleanup policies for old modules
- Use SSD storage for better performance

## Security Considerations

### Network Security

- Use HTTPS for all proxy communications
- Configure firewall rules for upstream access
- Monitor for unusual download patterns
- Validate module checksums

### Access Control

- Implement proper authentication for private repositories
- Use API tokens instead of passwords for automation
- Regularly audit user permissions
- Consider IP whitelisting for sensitive repositories

### Module Security

- Only proxy from trusted upstream servers
- Validate module signatures when available
- Monitor for malicious or compromised modules
- Keep proxy software updated

## Advanced Configuration

### Custom Upstream Proxies

You can configure any Go module proxy as an upstream route:

```json
{
  "url": "https://your-custom-proxy.com",
  "name": "Custom Proxy",
  "priority": 10
}
```

### Load Balancing

Configure multiple upstream proxies with the same priority for load balancing:

```json
{
  "routes": [
    {
      "url": "https://proxy1.example.com",
      "name": "Proxy 1",
      "priority": 10
    },
    {
      "url": "https://proxy2.example.com",
      "name": "Proxy 2",
      "priority": 10
    }
  ]
}
```

### Geographic Distribution

Set up multiple Pkgly instances in different regions with appropriate `GOPROXY` configuration:

```bash
export GOPROXY=https://us-pkgly.example.com,https://eu-pkgly.example.com,https://asia-pkgly.example.com,direct
```

## API Reference

### Configuration API

Get repository configuration:
```bash
curl -H "Authorization: Bearer $TOKEN" \
     https://your-pkgly.example.com/api/repository/{repo-id}/config/go
```

Update repository configuration:
```bash
curl -X PUT -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d @config.json \
     https://your-pkgly.example.com/api/repository/{repo-id}/config/go
```

### Statistics API

Get repository statistics:
```bash
curl -H "Authorization: Bearer $TOKEN" \
     https://your-pkgly.example.com/api/repository/{repo-id}/stats
```

## Migration Guide

### From Other Go Proxies

To migrate from another Go proxy solution:

1. **Export existing configurations** if possible
2. **Create equivalent repositories** in Pkgly
3. **Update GOPROXY settings** gradually
4. **Monitor cache warming** as modules are accessed
5. **Retire old proxy** after confirming everything works

### Database Migration

If migrating from a different system:

1. **Export module data** from the old system
2. **Import into Pkgly storage** using the API
3. **Update module metadata** as needed
4. **Verify module integrity** after import
5. **Update client configurations**

## Support and Resources

### Documentation

- [Pkgly Official Documentation](https://pkgly.kingtux.dev)
- [Go Module Proxy Protocol](https://golang.org/cmd/go/#hdr-Module_download_protocol)
- [Go Modules Reference](https://golang.org/ref/mod)

### Community

- [GitHub Issues](https://github.com/pkgly/pkgly/issues)
- [Discord Community](https://discord.gg/pkgly)
- [Stack Overflow](https://stackoverflow.com/questions/tagged/pkgly)

### Getting Help

If you encounter issues:

1. Check this documentation for common solutions
2. Search existing GitHub issues
3. Create a new issue with detailed information
4. Include configuration details and error logs
5. Provide steps to reproduce the problem

## Best Practices

### Repository Management

- Use descriptive repository names
- Organize repositories by team or project
- Tag repositories with appropriate metadata
- Regular review of repository permissions

### Cache Management

- Monitor cache hit rates and storage usage
- Set appropriate TTL values based on update frequency
- Consider cache warmup strategies for critical modules
- Plan storage growth for large module collections
- Use the **Admin → Repository → Packages** tab to inspect and delete cached entries under `go-proxy-cache/` when you need to force-refresh specific versions

### Performance

- Monitor proxy route performance regularly
- Use geographic distribution for global teams
- Optimize network connectivity to upstream proxies
- Consider CDN integration for high-traffic scenarios

### Security

- Regular security audits of repository access
- Keep Pkgly updated with latest security patches
- Monitor for unusual access patterns
- Implement proper backup and recovery procedures

---

*This documentation covers Pkgly Go proxy setup and usage. For additional information, refer to the Pkgly official documentation or community resources.*
