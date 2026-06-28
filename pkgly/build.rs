// ABOUTME: Prepares compile-time assets and build metadata for the pkgly binary.
// ABOUTME: Embeds the frontend bundle and optional source commit identifier.
#![allow(dead_code)]
use std::{
    env,
    fs::File,
    io::{Seek, Write, prelude::*},
    iter::Iterator,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use walkdir::{DirEntry, WalkDir};
use zip::{ZipWriter, write::SimpleFileOptions};

fn main() -> anyhow::Result<()> {
    expose_commit_id();
    #[cfg(feature = "frontend")]
    build_frontend()?;
    Ok(())
}

fn expose_commit_id() {
    println!("cargo::rerun-if-env-changed=PKGLY_COMMIT_ID");
    rerun_if_git_head_changes();
    if let Some(commit_id) = env::var("PKGLY_COMMIT_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(git_commit_id)
    {
        println!("cargo::rustc-env=PKGLY_COMMIT_ID={commit_id}");
    }
}

fn rerun_if_git_head_changes() {
    let Some(workspace_dir) = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let head_path = workspace_dir.join(".git").join("HEAD");
    println!("cargo::rerun-if-changed={}", head_path.display());
    let Ok(head) = std::fs::read_to_string(&head_path) else {
        return;
    };
    let Some(ref_path) = head.trim().strip_prefix("ref: ") else {
        return;
    };
    println!(
        "cargo::rerun-if-changed={}",
        workspace_dir.join(".git").join(ref_path).display()
    );
}

fn git_commit_id() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit_id = String::from_utf8(output.stdout).ok()?;
    let commit_id = commit_id.trim();
    if commit_id.is_empty() {
        return None;
    }
    Some(commit_id.to_string())
}
fn build_frontend() -> anyhow::Result<()> {
    let ignore_dir_not_found = env::var_os("IGNORE_DIR_NOT_FOUND").is_some();
    let frontend_dist = if let Some(frontend_dist) = env::var_os("FRONTEND_DIST").map(PathBuf::from)
    {
        if !frontend_dist.exists() {
            if ignore_dir_not_found {
                println!("cargo::warning=site build directory not found - creating empty zip");
                return empty_zip();
            }
            return Err(anyhow::anyhow!(
                "site build directory which was specified by the env var FRONTEND_DIST not found"
            ));
        }
        frontend_dist
    } else {
        let frontend_dist = get_site_dist_dir()?;
        if !frontend_dist.exists() {
            if ignore_dir_not_found {
                println!("cargo::warning=site build directory not found - creating empty zip");
                return empty_zip();
            }
            return Err(anyhow::anyhow!("{} not found", frontend_dist.display()));
        }
        frontend_dist
    };
    rerun_if_changed(&frontend_dist);
    zip_site(frontend_dist)?;
    Ok(())
}

fn get_site_dist_dir() -> anyhow::Result<PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .with_context(|| "CARGO_MANIFEST_DIR not set")?
        .parent()
        .context("Invalid CARGO_MANIFEST_DIR. (Could not get parent)")?
        .to_path_buf();
    let frontend_src = manifest_dir.join("site");
    if !frontend_src.exists() {
        return Err(anyhow::anyhow!("site directory not found"));
    }
    Ok(frontend_src.join("dist"))
}

fn rerun_if_changed(path: &Path) {
    println!("cargo::rerun-if-changed={}", path.display());
}
/// Bundling files seem to be broken with Android. So as a work around. I will zip the files and include them in the binary.
fn zip_site(frontend_dist: impl AsRef<Path>) -> anyhow::Result<()> {
    let out_dir = env::var("OUT_DIR").with_context(|| "OUT_DIR not set")?;
    let frontend_src = frontend_dist.as_ref();
    if !frontend_src.exists() {
        return Err(anyhow::anyhow!("site build directory not found"));
    }
    let dst = PathBuf::from(out_dir).join("frontend.zip");
    if dst.exists() {
        std::fs::remove_file(&dst)?;
    }
    let file = File::create(&dst)?;

    let walkdir = WalkDir::new(frontend_src);
    let it = walkdir.into_iter();

    internal_zip_dir(
        &mut it.filter_map(|e| e.ok()),
        frontend_src,
        file,
        zip::CompressionMethod::Stored,
    )?;
    println!("cargo:rustc-env=FRONTEND_ZIP={}", dst.display());
    println!("cargo:rustc-env=FRONTEND_SRC={}", frontend_src.display());

    Ok(())
}
fn internal_zip_dir<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    prefix: &Path,
    writer: T,
    method: zip::CompressionMethod,
) -> anyhow::Result<()>
where
    T: Write + Seek,
{
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    let mut buffer = Vec::with_capacity(1024);
    for entry in it {
        let absolute_path = entry.path();
        let stripped_path = entry.path().strip_prefix(prefix)?;
        let name = camino::Utf8Path::from_path(stripped_path)
            .with_context(|| format!("{stripped_path:?} Could not be converted to UTF-8"))?;

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if absolute_path.is_file() {
            zip.start_file(name.as_str(), options)?;
            let mut f = File::open(absolute_path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_str().is_empty() {
            zip.add_directory(name.to_string(), options)?;
        }
    }
    zip.finish()?;
    Result::Ok(())
}

fn empty_zip() -> anyhow::Result<()> {
    let out_dir = env::var("OUT_DIR").with_context(|| "OUT_DIR not set")?;
    let dst = PathBuf::from(out_dir).join("frontend.zip");
    if dst.exists() {
        std::fs::remove_file(&dst)?;
    }
    let file = File::create(&dst)?;

    let zip = ZipWriter::new(file);
    zip.finish()?;
    println!("cargo:rustc-env=FRONTEND_ZIP={}", dst.display());
    Ok(())
}
