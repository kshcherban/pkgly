# Maven Repository

Maven Repositories have two modes.

- **Hosted** - The repository is hosted on the server and is used to store artifacts. This is used to create a regular Maven Repository.
- **Proxy** - The repository is a proxy to another repository. This is used to cache artifacts from another repository.

## Package Listing

The admin **Packages** tab lists Maven package/version rows from the package catalog. For Maven rows, the displayed **Size** is refreshed from storage when possible: file paths use the stored file size, and hosted version-directory paths sum the regular files directly under that version directory.
