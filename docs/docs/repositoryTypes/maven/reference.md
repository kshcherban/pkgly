# Maven Repository Quick Reference

## Configuration Templates

### Hosted Repository
```json
{
  "type": "Hosted"
}
```

### Hosted Repository with Push Rules
```json
{
  "type": "Hosted",
  "push_rules": {
    "push_policy": "RELEASES",
    "yanking_allowed": true,
    "allow_overwrite": false,
    "must_be_project_member": false,
    "require_pkgly_deploy": false,
    "must_use_auth_token_for_push": false
  }
}
```

### Proxy Repository
```json
{
  "type": "Proxy",
  "proxy": {
    "routes": [
      {
        "url": "https://repo.maven.apache.org/maven2",
        "name": "Maven Central",
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
        "url": "https://repo.maven.apache.org/maven2",
        "name": "Maven Central",
        "priority": 10
      },
      {
        "url": "https://repo1.maven.org/maven2",
        "name": "Maven Central Mirror",
        "priority": 5
      }
    ]
  }
}
```

## Essential Commands

### Configure Maven Settings
Add the repository to your `~/.m2/settings.xml`:

```xml
<settings>
  <servers>
    <server>
      <id>pkgly</id>
      <username>your-username</username>
      <password>your-password</password>
    </server>
  </servers>
  <profiles>
    <profile>
      <id>pkgly</id>
      <repositories>
        <repository>
          <id>pkgly</id>
          <url>https://your-pkgly.example.com/repositories/storage/maven-repo</url>
        </repository>
      </repositories>
    </profile>
  </profiles>
  <activeProfiles>
    <activeProfile>pkgly</activeProfile>
  </activeProfiles>
</settings>
```

### Configure in pom.xml
```xml
<project>
  <repositories>
    <repository>
      <id>pkgly</id>
      <url>https://your-pkgly.example.com/repositories/storage/maven-repo</url>
    </repository>
  </repositories>
  <distributionManagement>
    <repository>
      <id>pkgly</id>
      <url>https://your-pkgly.example.com/repositories/storage/maven-repo</url>
    </repository>
    <snapshotRepository>
      <id>pkgly</id>
      <url>https://your-pkgly.example.com/repositories/storage/maven-repo</url>
    </snapshotRepository>
  </distributionManagement>
</project>
```

### Deploy Artifact
```bash
# Deploy with settings.xml configuration
mvn deploy

# Deploy with explicit credentials
mvn deploy \
  -DrepositoryId=pkgly \
  -Dusername=your-username \
  -Dpassword=your-password
```

### Deploy Specific Files
```bash
mvn deploy:deploy-file \
  -Durl=https://your-pkgly.example.com/repositories/storage/maven-repo \
  -DrepositoryId=pkgly \
  -Dfile=target/myapp-1.0.0.jar \
  -DpomFile=pom.xml \
  -DgeneratePom=false
```

### Download Dependencies
```bash
# Download all dependencies
mvn dependency:copy-dependencies

# Download specific artifact
mvn dependency:get \
  -Dartifact=com.example:myapp:1.0.0 \
  -DremoteRepositories=https://your-pkgly.example.com/repositories/storage/maven-repo
```

### Test Repository Access
```bash
# Test connectivity
curl -I "https://your-pkgly.example.com/repositories/storage/maven-repo"

# Download artifact metadata
curl -u "username:password" \
     "https://your-pkgly.example.com/repositories/storage/maven-repo/com/example/myapp/maven-metadata.xml"

# Download specific JAR
curl -u "username:password" -O \
     "https://your-pkgly.example.com/repositories/storage/maven-repo/com/example/myapp/1.0.0/myapp-1.0.0.jar"
```

## Publishing Workflows

### Standard Maven Release
```bash
# Set version
mvn versions:set -DnewVersion=1.0.0

# Deploy to repository
mvn clean deploy

# Commit version change
git commit -am "Release version 1.0.0"
git tag v1.0.0
```

### Release with Maven Release Plugin
```xml
<!-- pom.xml -->
<build>
  <plugins>
    <plugin>
      <groupId>org.apache.maven.plugins</groupId>
      <artifactId>maven-release-plugin</artifactId>
      <version>3.0.0</version>
      <configuration>
        <tagNameFormat>v@{project.version}</tagNameFormat>
      </configuration>
    </plugin>
  </plugins>
</build>
```

```bash
# Prepare release
mvn release:prepare

# Perform release
mvn release:perform
```

### CI/CD Integration (GitHub Actions)
```yaml
name: Build and Deploy Maven

on:
  push:
    branches: [main, develop]

jobs:
  build-and-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Set up JDK 17
        uses: actions/setup-java@v3
        with:
          java-version: '17'
          distribution: 'temurin'

      - name: Deploy to Maven Repository
        run: mvn deploy
        env:
          MAVEN_USERNAME: ${{ secrets.PKGLY_USERNAME }}
          MAVEN_PASSWORD: ${{ secrets.PKGLY_PASSWORD }}
```

## Common Endpoints

| Description | Endpoint | Example |
|-------------|----------|---------|
| Download JAR | `GET /{group}/{artifact}/{version}/{artifact}-{version}.jar` | `/com/example/myapp/1.0.0/myapp-1.0.0.jar` |
| Download Sources | `GET /{group}/{artifact}/{version}/{artifact}-{version}-sources.jar` | `/com/example/myapp/1.0.0/myapp-1.0.0-sources.jar` |
| Download Javadoc | `GET /{group}/{artifact}/{version}/{artifact}-{version}-javadoc.jar` | `/com/example/myapp/1.0.0/myapp-1.0.0-javadoc.jar` |
| Get Metadata | `GET /{group}/{artifact}/{version}/maven-metadata.xml` | `/com/example/myapp/1.0.0/maven-metadata.xml` |
| Upload JAR | `PUT /{group}/{artifact}/{version}/{artifact}-{version}.jar` | Upload with `curl -T` |
| Upload Metadata | `PUT /{group}/{artifact}/{version}/maven-metadata.xml` | Upload with `curl -T` |

## Maven Push Policies

### RELEASES
Only allows non-snapshot versions (no `-SNAPSHOT` suffix):
```bash
# Allowed
mvn versions:set -DnewVersion=1.0.0
mvn deploy

# Rejected
mvn versions:set -DnewVersion=1.0.0-SNAPSHOT
mvn deploy  # Will fail
```

### SNAPSHOTS
Only allows snapshot versions (must end with `-SNAPSHOT`):
```bash
# Allowed
mvn versions:set -DnewVersion=1.0.0-SNAPSHOT
mvn deploy

# Rejected
mvn versions:set -DnewVersion=1.0.0
mvn deploy  # Will fail
```

### STAGES
Allows both snapshots and releases:
```bash
# Both allowed
mvn versions:set -DnewVersion=1.0.0-SNAPSHOT
mvn deploy

mvn versions:set -DnewVersion=1.0.0
mvn deploy
```

## Troubleshooting Commands

### Check Repository Metadata
```bash
# Check root metadata
curl -u "username:password" \
     "https://your-pkgly.example.com/repositories/storage/maven-repo/maven-metadata.xml"

# Check artifact metadata
curl -u "username:password" \
     "https://your-pkgly.example.com/repositories/storage/maven-repo/com/example/myapp/maven-metadata.xml"
```

### Test Authentication
```bash
# Test basic authentication
curl -u "username:password" -I \
     "https://your-pkgly.example.com/repositories/storage/maven-repo/"

# Test upload permissions
echo "test" | curl -u "username:password" -T - \
     "https://your-pkgly.example.com/repositories/storage/maven-repo/test/test.txt"
```

### Debug Maven Deployment
```bash
# Enable debug logging
mvn deploy -X

# Skip tests for faster deployment
mvn deploy -DskipTests

# Deploy with explicit repository URL
mvn deploy \
  -DaltDeploymentRepository=pkgly::default::https://your-pkgly.example.com/repositories/storage/maven-repo
```

## Configuration Options

| Setting | Default | Recommended Range |
|---------|---------|-------------------|
| Push Policy | STAGES | RELEASES for production, SNAPSHOTS for development |
| Yanking Allowed | true | false for immutable repositories |
| Allow Overwrite | true | false for production, true for development |
| Project Member Only | false | true for team access control |
| Require Pkgly Deploy | false | true for automated deployments |

## Performance Tips

### For Large Artifacts
- Enable incremental builds to avoid redeploying unchanged dependencies
- Use Maven's parallel execution for faster builds: `mvn -T 4 clean deploy`
- Consider using artifact signatures for integrity verification

### For Proxy Repositories
- Set appropriate cache TTL based on update frequency
- Configure multiple proxy routes with different priorities for reliability
- Monitor proxy cache hit rates to optimize configuration

## Security Checklist

- [ ] Use HTTPS for all repository communications
- [ ] Enable authentication for write operations
- [ ] Implement project-based access control
- [ ] Use separate repositories for snapshots and releases
- [ ] Enable artifact signing for critical dependencies
- [ ] Regular audit of published artifacts
- [ ] Implement CI/CD token management
- [ ] Monitor deployment logs for unauthorized access

## Storage Layout

```
com/
├── example/
│   └── myapp/
│       ├── 1.0.0/
│       │   ├── myapp-1.0.0.jar
│       │   ├── myapp-1.0.0-sources.jar
│       │   ├── myapp-1.0.0-javadoc.jar
│       │   └── maven-metadata.xml
│       └── maven-metadata.xml
└── company/
    └── library/
        ├── 1.0.0/
        └── maven-metadata.xml
```

## Maven Repository Types

### Release Repository
- Configuration: `"push_policy": "RELEASES"`
- Purpose: Stable, non-changing versions
- Recommended for production dependencies
- No overwrite allowed in production settings

### Snapshot Repository
- Configuration: `"push_policy": "SNAPSHOTS"`
- Purpose: Development and testing versions
- Supports timestamped snapshots
- Overwrite typically allowed

### Staging Repository
- Configuration: `"push_policy": "STAGES"`
- Purpose: Mixed development and releases
- Supports both snapshots and releases
- Flexible configuration for mixed use cases

---

*Quick reference for Maven repository configuration and usage. See [Maven Route Reference](routes.md) for detailed API documentation.*