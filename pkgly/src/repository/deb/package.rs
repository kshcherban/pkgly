use std::io::{Cursor, Read};

use ar::Archive as ArArchive;
use bytes::Bytes;
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use tar::Archive as TarArchive;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder as ZstdDecoder;

use super::metadata::ControlFile;

#[derive(Debug, Clone)]
pub struct ParsedDeb {
    pub control: ControlFile,
    pub file_size: u64,
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
}

pub fn parse_deb_package(bytes: Bytes) -> Result<ParsedDeb, DebPackageError> {
    let mut cursor = Cursor::new(bytes.clone());
    let mut archive = ArArchive::new(&mut cursor);
    let mut control_bytes: Option<Vec<u8>> = None;

    while let Some(entry_result) = archive.next_entry() {
        let mut entry = entry_result.map_err(|err| DebPackageError::Archive(err.to_string()))?;
        let mut name = entry.header().identifier().to_vec();
        while name.ends_with(&[0]) {
            name.pop();
        }
        let id = String::from_utf8_lossy(&name);
        if id.starts_with("control.tar") {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|err| DebPackageError::Archive(err.to_string()))?;
            control_bytes = Some(decompress_control(&id, data)?);
            break;
        }
    }

    let control_data = control_bytes.ok_or(DebPackageError::MissingControl)?;
    let mut tar = TarArchive::new(Cursor::new(control_data));
    let mut control_file = None;
    for entry_result in tar
        .entries()
        .map_err(|err| DebPackageError::Archive(err.to_string()))?
    {
        let mut entry = entry_result.map_err(|err| DebPackageError::Archive(err.to_string()))?;
        if let Ok(path) = entry.path() {
            if path.ends_with("control") {
                let mut buf = String::new();
                entry
                    .read_to_string(&mut buf)
                    .map_err(|err| DebPackageError::Archive(err.to_string()))?;
                control_file = Some(ControlFile::parse(&buf).map_err(DebPackageError::Control)?);
                break;
            }
        }
    }

    let control = control_file.ok_or(DebPackageError::MissingControl)?;
    let file_size = bytes.len() as u64;
    let md5 = format_md5(&bytes);
    let mut sha1_hasher = Sha1::new();
    sha1_hasher.update(&bytes);
    let sha1 = format!("{:x}", sha1_hasher.finalize());
    let mut sha256_hasher = Sha256::new();
    sha256_hasher.update(&bytes);
    let sha256 = format!("{:x}", sha256_hasher.finalize());

    Ok(ParsedDeb {
        control,
        file_size,
        md5,
        sha1,
        sha256,
    })
}

fn format_md5(bytes: &Bytes) -> String {
    use md5::Md5;
    let mut hasher = Md5::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn decompress_control(identifier: &str, bytes: Vec<u8>) -> Result<Vec<u8>, DebPackageError> {
    if identifier.ends_with(".gz") {
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|err| DebPackageError::Archive(err.to_string()))?;
        return Ok(output);
    }
    if identifier.ends_with(".xz") {
        let mut decoder = XzDecoder::new(&bytes[..]);
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|err| DebPackageError::Archive(err.to_string()))?;
        return Ok(output);
    }
    if identifier.ends_with(".bz2") {
        let mut decoder = BzDecoder::new(&bytes[..]);
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|err| DebPackageError::Archive(err.to_string()))?;
        return Ok(output);
    }
    if identifier.ends_with(".zst") {
        let mut decoder = ZstdDecoder::new(&bytes[..])
            .map_err(|err| DebPackageError::Archive(err.to_string()))?;
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|err| DebPackageError::Archive(err.to_string()))?;
        return Ok(output);
    }
    if identifier.ends_with(".tar") {
        return Ok(bytes);
    }
    Err(DebPackageError::UnsupportedCompression(
        identifier.to_string(),
    ))
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DebPackageError {
    #[error("failed to parse deb archive: {0}")]
    Archive(String),
    #[error("unsupported control compression type: {0}")]
    UnsupportedCompression(String),
    #[error("control file missing in deb archive")]
    MissingControl,
    #[error(transparent)]
    Control(super::metadata::ControlParseError),
}

#[cfg(test)]
mod tests;
