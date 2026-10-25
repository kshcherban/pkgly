# ABOUTME: Exercises MCP stdio and scoped package operations against the real Docker stack.
# ABOUTME: Uses Python's standard library and disposable repositories without API doubles.
import contextlib
import io
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid
import zipfile


HTTP_CLIENT = """
import json, os, sys, urllib.request, urllib.error
spec = json.load(sys.stdin)
headers = {'User-Agent': 'pkgly-mcp-e2e', 'Content-Type': 'application/json'}
headers.update(spec.get('headers', {}))
body = spec.get('body')
request = urllib.request.Request(os.environ.get('PKGLY_URL', 'http://pkgly:8888') + spec['route'],
    data=None if body is None else json.dumps(body).encode(), headers=headers, method=spec['method'])
try:
    response = urllib.request.urlopen(request, timeout=30)
except urllib.error.HTTPError as error:
    response = error
with response:
    print(json.dumps({'status': response.code, 'body': response.read().decode(),
        'cookie': response.headers.get('Set-Cookie')}))
"""


class Stack:
    def __init__(self, compose):
        self.compose = compose
        self.root = f"/tmp/pkgly-mcp-{uuid.uuid4().hex}"
        self.token = os.environ.get("TEST_TOKEN", "NPDxeLFM8ehXKteIHW7DFy1chf2QaYdf")

    def command(self, *arguments, **kwargs):
        return subprocess.run(self.compose + list(arguments), check=True, capture_output=True, **kwargs)

    def http(self, method, route, body=None, cookie=None):
        headers = {"Cookie": cookie} if cookie else {"Authorization": f"Bearer {self.token}"}
        result = self.command("exec", "-T", "test-runner", "python3", "-c", HTTP_CLIENT,
            input=json.dumps({"method": method, "route": route, "body": body, "headers": headers}).encode())
        response = json.loads(result.stdout)
        assert response["status"] < 400, f"HTTP {method} {route}: {response['status']}"
        return response


class Client:
    def __init__(self, stack, token=None):
        self.sequence = 0
        self.buffer = b""
        self.errors = tempfile.TemporaryFile()
        self.process = subprocess.Popen(stack.compose + ["exec", "-T", "pkgly", "/app/pkglyctl",
            "--config", f"{stack.root}/config.toml", "--base-url", "http://127.0.0.1:8888",
            "--token", token or stack.token, "mcp", "serve"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.errors, bufsize=0)
        response = self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "pkgly-e2e", "version": "1"}})
        assert "tools" in response["result"]["capabilities"], response
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, message):
        self.process.stdin.write(json.dumps(message).encode() + b"\n")
        self.process.stdin.flush()

    def read(self):
        deadline = time.monotonic() + 60
        while b"\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            assert remaining > 0, "MCP response timed out"
            ready, _, _ = select.select([self.process.stdout], [], [], remaining)
            assert ready, "MCP response timed out"
            chunk = os.read(self.process.stdout.fileno(), 65536)
            if not chunk:
                self.errors.seek(0)
                raise AssertionError("MCP stdout closed: " + self.errors.read().decode())
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b"\n", 1)
        try:
            message = json.loads(line)
        except json.JSONDecodeError as error:
            raise AssertionError(f"MCP stdout is not JSON-RPC: {line[:200]!r}") from error
        assert message.get("jsonrpc") == "2.0", "stdout must contain only JSON-RPC"
        return message

    def request(self, method, params):
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        while True:
            response = self.read()
            if response.get("id") == self.sequence:
                return response

    def tool(self, name, arguments=None, error=False):
        response = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        assert "error" not in response, f"{name}: {response}"
        result = response["result"]
        assert bool(result.get("isError")) == error, f"{name}: {response}"
        assert json.loads(result["content"][0]["text"]) == result["structuredContent"], response
        return result["structuredContent"]

    def close(self):
        self.process.stdin.close()
        try:
            assert self.process.wait(timeout=15) == 0, "MCP server exited unsuccessfully"
            self.errors.seek(0)
            diagnostics = self.errors.read().decode()
            assert not diagnostics, f"MCP diagnostics must be pristine: {diagnostics}"
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()
            self.process.stdout.close()
            self.errors.close()


@contextlib.contextmanager
def client(stack, token=None):
    connection = Client(stack, token)
    try:
        yield connection
    finally:
        connection.close()


def create_repositories(admin, storage):
    repositories = []
    for name in ["mcp-thunder-a", "mcp-thunder-b"]:
        repo = admin.tool("repo_create", {"repository_type": "maven", "name": name,
            "storage": storage, "config": json.dumps({"maven": {"type": "Hosted"}})})
        admin.tool("repo_config_set", {"repository": repo["id"], "key": "auth", "value": '{"enabled":true}'})
        repositories.append(repo)
    return repositories


def check_local_and_management_actions(admin, storage, repositories):
    listing = admin.request("tools/list", {})["result"]["tools"]
    assert len(listing) == 38, len(listing)
    assert admin.tool("auth_whoami")["username"] == "admin"
    assert admin.tool("storage_get", {"id": storage["id"]})["name"] == storage["name"]
    assert any(item["id"] == storage["id"] for item in admin.tool("storage_list")["items"])
    repo = repositories[0]
    reference = f"{storage['name']}/{repo['name']}"
    assert admin.tool("repo_id", {"repository": reference})["message"] == repo["id"]
    assert admin.tool("repo_get", {"repository": reference})["id"] == repo["id"]
    assert any(item["id"] == repo["id"] for item in admin.tool("repo_list")["items"])
    assert "maven" in admin.tool("repo_config_list", {"repository": reference})["items"]
    assert admin.tool("repo_config_get", {"repository": reference, "key": "auth"})["enabled"] is True
    route = admin.tool("repo_url", {"repository": reference, "path": "example.jar"})
    assert route == admin.tool("package_browse", {"repository": reference, "path": "example.jar"})
    for ecosystem in ["npm", "cargo", "docker"]:
        assert ecosystem in admin.tool(f"native_{ecosystem}", {"repository": reference})["message"]


def check_profiles(admin):
    admin.tool("auth_login", {"username": "admin", "password": "TestAdmin", "token_name": "mcp-e2e"})
    saved = admin.tool("profile_show")
    assert saved["token"] and saved["token"] != "TestAdmin"
    admin.tool("auth_set_token", {"token": "disposable-profile-token", "profile": "mcp-profile"})
    admin.tool("profile_use", {"profile": "mcp-profile"})
    assert "disposable-profile-token" not in json.dumps(admin.tool("profile_show"))
    assert len(admin.tool("profile_list")["items"]) == 2
    admin.tool("auth_logout")
    assert admin.tool("profile_show")["token"] is None
    admin.tool("profile_remove", {"profile": "mcp-profile"})
    admin.tool("profile_use", {"profile": "local"})


def check_packages(stack, admin, repositories, temporary):
    artifact = Path(temporary) / "mcp.jar"
    with zipfile.ZipFile(artifact, "w") as archive:
        archive.writestr("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n")
    stack.command("cp", str(artifact), f"pkgly:{stack.root}/upload.jar")
    pom = Path(temporary) / "mcp.pom"
    pom.write_text('<project><modelVersion>4.0.0</modelVersion><groupId>com.example</groupId>'
        '<artifactId>mcp-widget</artifactId><version>1.0.0</version></project>')
    stack.command("cp", str(pom), f"pkgly:{stack.root}/upload.pom")
    path = "com/example/mcp-widget/1.0.0/mcp-widget-1.0.0.jar"
    for repo in repositories:
        admin.tool("package_upload_maven", {"repository": repo["id"], "path": path, "file": f"{stack.root}/upload.jar"})
        admin.tool("package_upload_maven", {"repository": repo["id"], "path": path.replace(".jar", ".pom"), "file": f"{stack.root}/upload.pom"})
    items = admin.tool("package_list", {"repository": repositories[0]["id"]})["items"]
    assert items, "uploaded package missing from listing"
    package = items[0]
    detail = admin.tool("package_describe", {"repository": repositories[0]["id"],
        "package": package["package"], "version": package["version"]})
    assert path.startswith(detail["cache_path"]), detail
    results = admin.tool("package_search", {"query": "mcp-widget", "limit": 25})["items"]
    assert {repo["id"] for repo in repositories}.issubset({row["repository_id"] for row in results}), results
    saved = admin.tool("package_download", {"repository": repositories[0]["id"], "path": path,
        "output_file": f"{stack.root}/download.jar"})
    assert saved["bytes"] == artifact.stat().st_size
    downloaded = Path(temporary) / "download.jar"
    stack.command("cp", f"pkgly:{saved['path']}", str(downloaded))
    assert downloaded.read_bytes() == artifact.read_bytes(), "download changed package bytes"
    return path


def upload_go_module(stack, admin, repository, temporary, module, version):
    gomod = Path(temporary) / "go.mod"
    gomod.write_text(f"module {module}\n\ngo 1.21\n// {version}\n")
    archive = Path(temporary) / "module.zip"
    with zipfile.ZipFile(archive, "w") as contents:
        contents.writestr(f"{module}@{version}/go.mod", gomod.read_bytes())
        contents.writestr(f"{module}@{version}/dummy.go", "package dummy\n")
    info = Path(temporary) / "module.info"
    info.write_text(json.dumps({"Version": version, "Time": "2026-10-24T12:00:00Z"}))
    for source, name in [(gomod, "go.mod"), (archive, "module.zip"), (info, "module.info")]:
        stack.command("cp", str(source), f"pkgly:{stack.root}/{name}")
    admin.tool("package_upload_go", {"repository": repository, "module_name": module,
        "version": version, "module": f"{stack.root}/module.zip",
        "info": f"{stack.root}/module.info", "gomod": f"{stack.root}/go.mod"})


def check_go_deletion(stack, admin, storage, temporary):
    repo = admin.tool("repo_create", {"repository_type": "go", "name": "mcp-go-dummy",
        "storage": storage["name"], "config": '{"go":{"type":"Hosted"}}'})
    module = "example.com/mcp-go-dummy"
    for version in ["v0.1.0", "v0.2.0"]:
        upload_go_module(stack, admin, repo["id"], temporary, module, version)
    assert len(admin.tool("package_list", {"repository": repo["id"]})["items"]) == 2
    assert len(admin.tool("package_search", {"query": "mcp-go-dummy"})["items"]) == 2
    deleted = admin.tool("package_delete", {"repository": repo["id"],
        "paths": [f"{module}/@v/v0.2.0.mod"], "yes": True})
    assert deleted["deleted"] == 3, deleted
    listing = admin.tool("package_list", {"repository": repo["id"]})["items"]
    assert len(listing) == 1 and listing[0]["version"] == "v0.1.0.zip", listing
    search = admin.tool("package_search", {"query": "mcp-go-dummy"})["items"]
    assert len(search) == 1 and search[0]["cache_path"] == f"{module}/@v/v0.1.0.zip", search
    versions = stack.http("GET", f"/repositories/{storage['name']}/{repo['name']}/{module}/@v/list")
    assert versions["body"] == "v0.1.0\n", versions
    latest = stack.http("GET", f"/repositories/{storage['name']}/{repo['name']}/{module}/@latest")
    assert json.loads(latest["body"])["Version"] == "v0.1.0", latest
    gomod = stack.http("GET", f"/repositories/{storage['name']}/{repo['name']}/{module}/go.mod")
    assert "// v0.1.0" in gomod["body"] and "// v0.2.0" not in gomod["body"], gomod
    admin.tool("package_delete", {"repository": repo["id"], "paths": [module], "yes": True})
    assert not admin.tool("package_list", {"repository": repo["id"]})["items"]
    assert not admin.tool("package_search", {"query": "mcp-go-dummy"})["items"]
    print("MCP Go deletion clears package listings, search, and version lists", flush=True)


def tar_gzip(entries):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for path, contents in entries.items():
            contents = contents.encode()
            info = tarfile.TarInfo(path)
            info.size = len(contents)
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(contents))
    return output.getvalue()


def build_debian_artifact(path, name):
    control = (f"Package: {name}\nVersion: 0.1.0\nArchitecture: all\n"
        "Maintainer: Pkgly Test <test@example.invalid>\nDescription: MCP test package\n")
    output = bytearray(b"!<arch>\n")
    members = [("debian-binary", b"2.0\n"),
        ("control.tar.gz", tar_gzip({"./control": control})),
        ("data.tar.gz", tar_gzip({f"./usr/share/{name}/fixture.txt": name}))]
    for name, contents in members:
        header = f'{name + "/":<16}{0:<12}{0:<6}{0:<6}{"100644":<8}{len(contents):<10}`\n'
        output.extend(header.encode())
        output.extend(contents)
        if len(contents) % 2:
            output.extend(b"\n")
    path.write_bytes(output)


def check_debian_upload(stack, admin, storage, temporary):
    repo = admin.tool("repo_create", {"repository_type": "deb", "name": "mcp-deb",
        "storage": storage["name"]})
    name = "mcp-deb-fixture"
    filename = f"{name}_0.1.0_all.deb"
    artifact = Path(temporary) / filename
    build_debian_artifact(artifact, name)
    stack.command("cp", str(artifact), f"pkgly:{stack.root}/{filename}")
    path = f"pool/main/m/{name}/{filename}"
    admin.tool("package_upload_deb", {"repository": repo["id"], "path": path,
        "file": f"{stack.root}/{filename}"})
    listing = admin.tool("package_list", {"repository": repo["id"]})["items"]
    assert len(listing) == 1 and listing[0]["package"] == name, listing
    saved = admin.tool("package_download", {"repository": repo["id"], "path": path,
        "output_file": f"{stack.root}/download.deb"})
    downloaded = Path(temporary) / "download.deb"
    stack.command("cp", f"pkgly:{saved['path']}", str(downloaded))
    assert downloaded.read_bytes() == artifact.read_bytes()
    deleted = admin.tool("package_delete", {"repository": repo["id"], "paths": [path], "yes": True})
    assert deleted == {"deleted": 1, "missing": [], "rejected": []}, deleted
    assert not admin.tool("package_list", {"repository": repo["id"]})["items"]
    print("MCP Debian multipart upload, exact download, and deletion passed", flush=True)


def check_scopes(stack, repositories, path):
    login = stack.http("POST", "/api/user/login", {"email_or_username": "admin", "password": "TestAdmin"})
    cookie = login["cookie"].split(";", 1)[0]
    scoped = stack.http("POST", "/api/user/token/create", {"name": "mcp-scoped", "scopes": [],
        "repository_scopes": [{"repository_id": repositories[0]["id"], "scopes": ["Read"]}]}, cookie)
    token = json.loads(scoped["body"])["token"]
    with client(stack, token) as reader:
        allowed = repositories[0]["id"]
        assert reader.tool("package_list", {"repository": allowed})["items"]
        results = reader.tool("package_search", {"query": "mcp-widget"})["items"]
        assert {row["repository_id"] for row in results} == {allowed}, results
        assert "403" in reader.tool("repo_get", {"repository": repositories[1]["id"]}, error=True)["message"]
        reference = f"{repositories[1]['storage_name']}/{repositories[1]['name']}"
        denied = reader.tool("package_download", {"repository": reference, "path": path,
            "output_file": f"{stack.root}/denied.jar"}, error=True)
        assert "403" in denied["message"], denied
        for name, arguments, status in [
            ("package_delete", {"repository": allowed, "paths": [path], "yes": True}, "403"),
            ("repo_config_set", {"repository": allowed, "key": "auth", "value": '{"enabled":false}'}, "403"),
            ("package_upload_maven", {"repository": allowed, "path": path, "file": f"{stack.root}/upload.jar"}, "401"),
        ]:
            denied = reader.tool(name, arguments, error=True)
            assert status in denied["message"], denied
            assert token not in json.dumps(denied)
    scoped = stack.http("POST", "/api/user/token/create", {"name": "mcp-global-read",
        "scopes": ["ReadRepository"], "repository_scopes": []}, cookie)
    with client(stack, json.loads(scoped["body"])["token"]) as reader:
        results = reader.tool("package_search", {"query": "mcp-widget"})["items"]
        assert {repo["id"] for repo in repositories}.issubset({row["repository_id"] for row in results})
        saved = reader.tool("package_download", {"repository": reference, "path": path,
            "output_file": f"{stack.root}/allowed-read.jar"})
        assert saved["bytes"] > 0
    with client(stack, "invalid-mcp-token") as invalid:
        assert "401" in invalid.tool("auth_whoami", error=True)["message"]


def run(stack):
    stack.command("exec", "-T", "pkgly", "mkdir", "-p", stack.root)
    with tempfile.TemporaryDirectory(prefix="pkgly-mcp-") as temporary, client(stack) as admin:
        storage = admin.tool("storage_create", {"name": f"mcp-grave-digger-{uuid.uuid4().hex[:8]}", "path": f"{stack.root}/storage"})
        try:
            repositories = create_repositories(admin, storage["name"])
            check_local_and_management_actions(admin, storage, repositories)
            check_profiles(admin)
            print("MCP management and profile tools passed", flush=True)
            path = check_packages(stack, admin, repositories, temporary)
            print("MCP upload, search, describe, and download passed", flush=True)
            check_go_deletion(stack, admin, storage, temporary)
            check_debian_upload(stack, admin, storage, temporary)
            check_scopes(stack, repositories, path)
            denied = admin.tool("package_delete", {"repository": repositories[0]["id"], "paths": [path]}, error=True)
            assert "requires --yes" in denied["message"]
            deleted = admin.tool("package_delete", {"repository": repositories[0]["id"], "paths": [path], "yes": True})
            assert deleted["deleted"] > 0, deleted
            admin.tool("repo_delete", {"repository": repositories[1]["id"], "yes": True})
        finally:
            admin.tool("storage_delete", {"id": storage["id"], "yes": True})
    print("MCP E2E passed: stdio, profiles, repositories, packages, scoped search, denied mutations, shutdown")


if __name__ == "__main__":
    run(Stack(sys.argv[1:] or ["docker", "compose", "-f", "tests/docker/docker-compose.test.yml"]))
