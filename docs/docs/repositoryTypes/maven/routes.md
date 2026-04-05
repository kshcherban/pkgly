# Maven Repository HTTP Routes

Pkgly implements Maven repository protocol for both hosted and proxy repositories. This reference details every HTTP route, its purpose, and example usage. Replace placeholders with your own hostname, storage name, repository name, Maven coordinates, and authentication credentials.

- `<host>` – Pkgly base URL (e.g. `pkgly.example.com`)
- `<storage>` – Storage identifier that backs the repository
- `<repository>` – Repository name
- `<groupId>` – Maven group ID (dots become path separators: `com.example` → `com/example`)
- `<artifactId>` – Maven artifact ID
- `<version>` – Maven version (e.g. `1.0.0`, `1.0.0-SNAPSHOT`)
- `<classifier>` – Optional classifier (e.g. `sources`, `javadoc`)
- `<username>` – Username for basic authentication
- `<password>` – Password for basic authentication

## Artifact Download Operations

### Download Main Artifact
Download the primary JAR file:
```bash
curl -u "<username>:<password>" -O \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>.jar"
```

### Download Sources
Download source JAR:
```bash
curl -u "<username>:<password>" -O \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>-sources.jar"
```

### Download Javadoc
Download Javadoc JAR:
```bash
curl -u "<username>:<password>" -O \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>-javadoc.jar"
```

### Download Classified Artifact
Download artifact with custom classifier:
```bash
curl -u "<username>:<password>" -O \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>-<classifier>.jar"
```

### Download POM File
Download project metadata:
```bash
curl -u "<username>:<password>" -O \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>.pom"
```

## Metadata Operations

### Get Version Metadata
Retrieve metadata for a specific version:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/maven-metadata.xml"
```
**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<metadata modelVersion="1.1.0">
  <groupId>com.example</groupId>
  <artifactId>myapp</artifactId>
  <version>1.0.0</version>
  <versioning>
    <snapshot>
      <timestamp>20231215.143022</timestamp>
      <buildNumber>1</buildNumber>
    </snapshot>
    <lastUpdated>20231215143022</lastUpdated>
    <snapshotVersions>
      <snapshotVersion>
        <extension>pom</extension>
        <value>1.0.0-SNAPSHOT</value>
        <updated>20231215143022</updated>
      </snapshotVersion>
    </snapshotVersions>
  </versioning>
</metadata>
```

### Get Artifact Metadata
Retrieve metadata for all versions of an artifact:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/maven-metadata.xml"
```
**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<metadata modelVersion="1.1.0">
  <groupId>com.example</groupId>
  <artifactId>myapp</artifactId>
  <versioning>
    <latest>1.2.0</latest>
    <release>1.2.0</release>
    <versions>
      <version>1.0.0</version>
      <version>1.1.0</version>
      <version>1.2.0</version>
      <version>1.1.0-SNAPSHOT</version>
    </versions>
    <lastUpdated>20231215143022</lastUpdated>
  </versioning>
</metadata>
```

### Get Group Metadata
List all artifacts in a group:
```bash
curl -u "<username>:<password>" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/maven-metadata.xml"
```

## Upload Operations

### Upload Main Artifact
Upload the primary JAR file:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -T "myapp-1.0.0.jar" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>.jar"
```

### Upload Sources
Upload source JAR:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -T "myapp-1.0.0-sources.jar" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>-sources.jar"
```

### Upload Javadoc
Upload Javadoc JAR:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -T "myapp-1.0.0-javadoc.jar" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>-javadoc.jar"
```

### Upload Classified Artifact
Upload artifact with custom classifier:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -T "myapp-1.0.0-linux.jar" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>-linux.jar"
```

### Upload POM File
Upload project metadata:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -T "pom.xml" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>.pom"
```

### Upload Version Metadata
Upload version-specific metadata:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -H "Content-Type: application/xml" \
     -d @version-metadata.xml \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/maven-metadata.xml"
```

### Upload Artifact Metadata
Upload artifact-level metadata:
```bash
curl -X PUT \
     -u "<username>:<password>" \
     -H "Content-Type: application/xml" \
     -d @artifact-metadata.xml \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/maven-metadata.xml"
```

## Complete Upload Workflow

### Single Artifact Upload Sequence
```bash
#!/bin/bash
HOST="https://pkgly.example.com"
STORAGE="storage"
REPO="maven-repo"
GROUP="com/example"
ARTIFACT="myapp"
VERSION="1.0.0"
USER="username"
PASS="password"

# Upload main JAR
curl -X PUT \
     -u "${USER}:${PASS}" \
     -T "target/${ARTIFACT}-${VERSION}.jar" \
     "${HOST}/repositories/${STORAGE}/${REPO}/${GROUP}/${ARTIFACT}/${VERSION}/${ARTIFACT}-${VERSION}.jar"

# Upload POM
curl -X PUT \
     -u "${USER}:${PASS}" \
     -T "pom.xml" \
     "${HOST}/repositories/${STORAGE}/${REPO}/${GROUP}/${ARTIFACT}/${VERSION}/${ARTIFACT}-${VERSION}.pom"

# Upload sources (optional)
curl -X PUT \
     -u "${USER}:${PASS}" \
     -T "target/${ARTIFACT}-${VERSION}-sources.jar" \
     "${HOST}/repositories/${STORAGE}/${REPO}/${GROUP}/${ARTIFACT}/${VERSION}/${ARTIFACT}-${VERSION}-sources.jar"

# Update artifact metadata
curl -X PUT \
     -u "${USER}:${PASS}" \
     -H "Content-Type: application/xml" \
     -d @maven-metadata.xml \
     "${HOST}/repositories/${STORAGE}/${REPO}/${GROUP}/${ARTIFACT}/maven-metadata.xml"
```

### Maven Deploy Plugin Integration
The Maven deploy plugin automatically handles the upload sequence:
```bash
mvn deploy \
  -DrepositoryId=pkgly \
  -Durl=https://pkgly.example.com/repositories/storage/maven-repo \
  -Dusername=your-username \
  -Dpassword=your-password
```

## Proxy Repository Operations

### Proxy Route Configuration
Configure upstream repositories for proxy mode:
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

### Proxy Cache Bypass
Force refresh from upstream repositories:
```bash
curl -u "<username>:<password>" \
     -H "Cache-Control: no-cache" \
     "https://<host>/repositories/<storage>/<repository>/<groupId>/<artifactId>/<version>/<artifactId>-<version>.jar"
```

## Repository Management API

### List Cached Packages
List cached Maven artifacts:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/packages?page=1&per_page=50"
```

### Delete Cached Artifacts
Remove cached artifacts (requires repository edit permission):
```bash
curl -X DELETE \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d '{"paths": ["com/example/myapp/1.0.0/myapp-1.0.0.jar"]}' \
     "https://<host>/api/repository/<repository-id>/packages"
```

### Repository Configuration
Get Maven repository configuration:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/config/maven"
```

Update repository configuration:
```bash
curl -X PUT \
     -H "Authorization: Bearer <token>" \
     -H "Content-Type: application/json" \
     -d @config.json \
     "https://<host>/api/repository/<repository-id>/config/maven"
```

### Repository Statistics
Get repository usage statistics:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/stats"
```
**Response:**
```json
{
  "total_artifacts": 234,
  "total_versions": 567,
  "storage_used": "3.2GB",
  "last_activity": "2023-12-15T15:30:00Z",
  "top_artifacts": [
    {"name": "com.example:myapp", "version_count": 12},
    {"name": "com.company:library", "version_count": 8}
  ],
  "download_stats": {
    "total_downloads": 15234,
    "unique_artifacts": 89
  }
}
```

## Authentication and Security

### Basic Authentication
All write operations require authentication:
```bash
curl -u "username:password" \
     "https://<host>/repositories/<storage>/<repository>/com/example/myapp/1.0.0/myapp-1.0.0.jar"
```

### Project-Based Access Control
If `must_be_project_member` is enabled, users must be project members:
```bash
# Check project membership (for debugging)
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/project/myproject/members"
```

### Token-Based Authentication
API endpoints use Bearer token authentication:
```bash
curl -H "Authorization: Bearer <token>" \
     "https://<host>/api/repository/<repository-id>/stats"
```

## Error Responses

### Common Error Codes
- `401 Unauthorized` - Authentication required or invalid credentials
- `403 Forbidden` - Insufficient permissions or not a project member
- `404 Not Found` - Artifact, metadata, or repository does not exist
- `409 Conflict` - Version already exists and overwrite not allowed
- `422 Unprocessable Entity` - Invalid POM or metadata format
- `500 Internal Server Error` - Server-side error during upload

### Error Response Format
```json
{
  "error": "Authentication failed",
  "message": "Invalid username or password provided",
  "details": {
    "repository": "maven-repo",
    "path": "com/example/myapp/1.0.0/myapp-1.0.0.jar"
  }
}
```

## Maven Integration Examples

### Gradle Integration
```groovy
repositories {
    maven {
        url 'https://pkgly.example.com/repositories/storage/maven-repo'
        credentials {
            username = System.getenv('PKGLY_USERNAME')
            password = System.getenv('PKGLY_PASSWORD')
        }
    }
}

publishing {
    publications {
        mavenJava(MavenPublication) {
            from components.java
        }
    }
    repositories {
        maven {
            url 'https://pkgly.example.com/repositories/storage/maven-repo'
            credentials {
                username = System.getenv('PKGLY_USERNAME')
                password = System.getenv('PKGLY_PASSWORD')
            }
        }
    }
}
```

### SBT Integration
```scala
// build.sbt
publishTo := Some("Pkgly" at "https://pkgly.example.com/repositories/storage/maven-repo")

credentials += Credentials(
  "Pkgly",
  "pkgly.example.com",
  System.getenv("PKGLY_USERNAME"),
  System.getenv("PKGLY_PASSWORD")
)

resolvers += "Pkgly" at "https://pkgly.example.com/repositories/storage/maven-repo"
```

### Leiningen (Clojure) Integration
```clojure
;; project.clj
:repositories [["pkgly" {:url "https://pkgly.example.com/repositories/storage/maven-repo"
                              :username [:env/pkgly-username]
                              :password [:env/pkgly-password]}]]

:deploy-repositories [["releases" {:url "https://pkgly.example.com/repositories/storage/maven-repo"
                                   :username [:env/pkgly-username]
                                   :password [:env/pkgly-password]}]]
```

Use these endpoints as a foundation for Maven client integration, build tool configuration, or custom tooling when working with Pkgly Maven repositories.

---

*Complete reference for Maven repository HTTP routes. See [Maven Quick Reference](reference.md) for usage examples and configuration.*