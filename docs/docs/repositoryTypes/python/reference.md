# Python Package Repository Quick Reference

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
        "url": "https://pypi.org",
        "name": "PyPI",
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
        "url": "https://pypi.org",
        "name": "PyPI Official",
        "priority": 10
      },
      {
        "url": "https://pypi.python.org/simple",
        "name": "PyPI Simple",
        "priority": 5
      }
    ]
  }
}
```

## Essential Commands

### Configure pip Repository
```bash
# Create pip configuration directory
mkdir -p ~/.pip

# Add repository to pip.conf
cat >> ~/.pip/pip.conf << EOF
[global]
index-url = https://your-pkgly.example.com/repositories/storage/python-repo/simple
extra-index-url = https://pypi.org/simple
EOF

# Or configure for project
cat >> pip.conf << EOF
[global]
index-url = https://your-pkgly.example.com/repositories/storage/python-repo/simple
extra-index-url = https://pypi.org/simple
EOF
```

### Configure with Environment Variables
```bash
export PIP_INDEX_URL=https://your-pkgly.example.com/repositories/storage/python-repo/simple
export PIP_EXTRA_INDEX_URL=https://pypi.org/simple

# Add to shell profile
echo 'export PIP_INDEX_URL=https://your-pkgly.example.com/repositories/storage/python-repo/simple' >> ~/.bashrc
echo 'export PIP_EXTRA_INDEX_URL=https://pypi.org/simple' >> ~/.bashrc
```

### Configure in requirements.txt
```txt
--index-url https://your-pkgly.example.com/repositories/storage/python-repo/simple
--extra-index-url https://pypi.org/simple

my-package==1.0.0
other-package>=2.0.0
```

### Install Package
```bash
# Install from private repository
pip install my-package

# Install specific version
pip install my-package==1.0.0

# Install with requirements file
pip install -r requirements.txt

# Install with additional options
pip install --trusted-host your-pkgly.example.com my-package
```

### Upload Package
```bash
# Install build tools
pip install build twine

# Build package
python -m build

# Upload to repository
twine upload --repository pkgly dist/*
```

### Configure .pypirc
Add to `~/.pypirc`:
```ini
[pkgly]
repository = https://your-pkgly.example.com/repositories/storage/python-repo
username = your-username
password = your-password

[distutils]
index-servers =
    pkgly
    pypi
```

### Upload with Specific Repository
```bash
# Upload to Pkgly
twine upload --repository pkgly dist/*

# Upload with environment variables
REPOSITORY_URL=https://your-pkgly.example.com/repositories/storage/python-repo \
USERNAME=your-username \
PASSWORD=your-password \
twine upload --repository-url $REPOSITORY_URL dist/*
```

## Publishing Workflows

### Standard Python Package
```bash
# Project structure
my-package/
├── pyproject.toml
├── setup.py (optional)
├── src/
│   └── my_package/
│       ├── __init__.py
│       └── module.py
└── tests/
    └── test_module.py

# Build package
python -m build

# Upload to Pkgly
twine upload --repository pkgly dist/*
```

### pyproject.toml Configuration
```toml
[build-system]
requires = ["setuptools>=61.0"]
build-backend = "setuptools.build_meta"

[project]
name = "my-package"
version = "1.0.0"
authors = [
  { name="Your Name", email="your.email@example.com" },
]
description = "My awesome Python package"
readme = "README.md"
requires-python = ">=3.8"
classifiers = [
    "Programming Language :: Python :: 3",
    "License :: OSI Approved :: MIT License",
    "Operating System :: OS Independent",
]
dependencies = [
    "requests>=2.25.0",
    "numpy>=1.20.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=6.0",
    "black>=21.0",
    "flake8>=3.8",
]

[project.urls]
Homepage = "https://github.com/username/my-package"
"Bug Tracker" = "https://github.com/username/my-package/issues"
```

### CI/CD Integration (GitHub Actions)
```yaml
name: Build and Publish Python Package

on:
  push:
    tags: ['v*']
  workflow_dispatch:

jobs:
  build-and-publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Set up Python
        uses: actions/setup-python@v4
        with:
          python-version: '3.9'

      - name: Install build dependencies
        run: |
          python -m pip install --upgrade pip
          pip install build twine

      - name: Build package
        run: python -m build

      - name: Publish to Pkgly
        env:
          TWINE_USERNAME: ${{ secrets.PKGLY_USERNAME }}
          TWINE_PASSWORD: ${{ secrets.PKGLY_PASSWORD }}
          TWINE_REPOSITORY_URL: https://your-pkgly.example.com/repositories/storage/python-repo
        run: |
          twine upload --repository-url $TWINE_REPOSITORY_URL \
                      --username $TWINE_USERNAME \
                      --password $TWINE_PASSWORD \
                      dist/*
```

## Common Endpoints

| Description | Endpoint | Example |
|-------------|----------|---------|
| Simple Package Index | `GET /simple/{package}/` | Package versions list |
| Package Metadata | `GET /pypi/{package}/{version}/json` | Package information |
| Upload Package | `POST /pypi/{package}/upload/` | Upload new version |
| Download Package | `GET /pypi/{package}/{version}/` | Download specific version |
| Package Index Data | `GET /pypi-data/{package}/json` | Package index information |

## Testing Repository Access

### pip Configuration Test
```bash
# Test pip configuration
pip config list

# Install test package
pip install --dry-run my-package

# Debug pip install
pip install --verbose my-package
```

### Upload Test
```bash
# Check authentication
twine check dist/*

# Test upload without actually uploading (dry run)
twine upload --repository pkgly --dry-run dist/*

# Upload with verbose output
twine upload --repository pkgly --verbose dist/*
```

### Manual Endpoint Testing
```bash
# Test simple index
curl https://your-pkgly.example.com/repositories/storage/python-repo/simple/

# Test package page
curl https://your-pkgly.example.com/repositories/storage/python-repo/simple/my-package/

# Test package metadata
curl https://your-pkgly.example.com/repositories/storage/python-repo/pypi/my-package/1.0.0/json
```

## Advanced Configuration

### Multiple Repository Configuration
```ini
# ~/.pypirc
[pkgly]
repository = https://your-pkgly.example.com/repositories/storage/python-repo
username = your-username
password = your-password

[other-repo]
repository = https://other-repo.example.com/simple
username = other-username
password = other-password

[distutils]
index-servers =
    pkgly
    other-repo
```

### pip Configuration with Authentication
```ini
# ~/.pip/pip.conf
[global]
index-url = https://username:password@your-pkgly.example.com/repositories/storage/python-repo/simple
extra-index-url = https://pypi.org/simple

[pkgly]
index-url = https://your-pkgly.example.com/repositories/storage/python-repo/simple
```

### conda Integration
```bash
# Create conda channel configuration
mkdir -p ~/.conda/channels

# Add Pkgly as channel
conda config --add channels https://your-pkgly.example.com/repositories/storage/python-repo/conda

# Set channel priority
conda config --set channel_priority strict
```

## Troubleshooting Commands

### Debug pip Install Issues
```bash
# Verbose install
pip install --verbose my-package

# Force reinstall
pip install --force-reinstall --no-cache-dir my-package

# Check package location
pip show my-package

# List installed packages
pip list | grep my-package
```

### Debug Upload Issues
```bash
# Check package files
twine check dist/*

# Test repository connection
twine upload --repository pkgly --skip-existing dist/*

# Verbose upload
twine upload --repository pkgly --verbose dist/*
```

### Common Issues Solutions
```bash
# SSL certificate issues
pip install --trusted-host your-pkgly.example.com my-package

# Authentication issues
pip install --index-url https://username:password@your-pkgly.example.com/repositories/storage/python-repo/simple my-package

# Cache issues
pip cache purge
```

## Configuration Options

| Setting | Default | Description |
|---------|---------|-------------|
| Proxy Routes | None | Upstream PyPI repositories |
| Cache TTL | Default | Package metadata caching duration |
| Authentication | Optional | Basic auth for uploads |

## Package Formats Supported

### Source Distribution (sdist)
```bash
# Create source distribution
python setup.py sdist

# Upload source distribution
twine upload --repository pkgly dist/*.tar.gz
```

### Wheel Distribution
```bash
# Create wheel distribution
python setup.py bdist_wheel

# Build both source and wheel
python -m build

# Upload wheel
twine upload --repository pkgly dist/*.whl
```

## Development Workflow

### Local Development
```bash
# Install in development mode
pip install -e .

# Install with development dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Lint code
flake8 src/
black src/
```

### Version Management
```bash
# Bump version with bumpversion
pip install bumpversion
bumpversion patch  # 1.0.0 -> 1.0.1
bumpversion minor  # 1.0.0 -> 1.1.0
bumpversion major  # 1.0.0 -> 2.0.0
```

### Dependency Management
```txt
# requirements.txt
my-package==1.0.0
requests>=2.25.0

# requirements-dev.txt
-r requirements.txt
pytest>=6.0.0
black>=21.0.0
```

## Security Checklist

- [ ] Use HTTPS for all repository communications
- [ ] Enable authentication for package uploads
- [ ] Scan packages for security vulnerabilities
- [ ] Use signed packages when possible
- [ ] Monitor package access logs
- [ ] Implement token-based authentication for CI/CD
- [ ] Regular dependency audits
- [ ] Use separate repositories for development and production

## Performance Tips

### For Large Packages
- Use wheel distributions for faster installation
- Exclude unnecessary files in `MANIFEST.in`
- Optimize package dependencies

### For Proxy Mode
- Configure multiple PyPI mirrors for reliability
- Set appropriate cache TTL for package metadata
- Monitor cache hit rates

### For Development
- Use local development installations (`pip install -e .`)
- Cache dependencies between CI/CD runs
- Use parallel testing for faster builds

---

*Quick reference for Python package repository configuration and usage. See [Python Route Reference](routes.md) for detailed API documentation.*