#![allow(clippy::expect_used, clippy::panic, clippy::todo, clippy::unwrap_used)]
use super::*;

#[test]
fn parse_control_file_handles_multiline_description() {
    let control = "Package: sample\nVersion: 1.0\nDescription: summary line\n more details\n .\n final line\n";
    let parsed = ControlFile::parse(control).expect("control to parse");
    let description = parsed.get("Description").expect("description field");
    assert_eq!(
        description, "summary line\nmore details\n\nfinal line",
        "description should preserve folded lines"
    );
}

#[test]
fn parse_control_rejects_invalid_lines() {
    let control = "Package sample";
    let err = ControlFile::parse(control).expect_err("validation error");
    assert!(matches!(err, ControlParseError::MissingSeparator(_)));
}

#[test]
fn packages_entry_contains_required_fields() {
    let record = PackagesRecord {
        package: "sample".into(),
        version: "1.2.3".into(),
        architecture: "amd64".into(),
        section: Some("utils".into()),
        priority: Some("optional".into()),
        maintainer: Some("Pkgly <dev@pkgly>".into()),
        installed_size: Some(2048),
        depends: Some("libc6 (>= 2.31)".into()),
        description: "summary\ndetails line".into(),
        homepage: Some("https://pkgly".into()),
        filename: "pool/main/s/sample/sample_1.2.3_amd64.deb".into(),
        size: 42,
        md5: "md5".into(),
        sha1: "sha1".into(),
        sha256: "sha256".into(),
    };
    let entry = format_packages_entry(&record);
    assert!(entry.contains("Package: sample"));
    assert!(entry.contains("Depends: libc6"));
    assert!(entry.contains("Filename: pool/main/s/sample/sample_1.2.3_amd64.deb"));
    assert!(entry.contains(" details line"));
}

#[test]
fn packages_entry_format_matches_expected_output() {
    let record = PackagesRecord {
        package: "sample".into(),
        version: "1.2.3".into(),
        architecture: "amd64".into(),
        section: Some("utils".into()),
        priority: Some("optional".into()),
        maintainer: Some("Pkgly <dev@pkgly>".into()),
        installed_size: Some(2048),
        depends: Some("libc6 (>= 2.31)".into()),
        description: "summary\ndetails line".into(),
        homepage: Some("https://pkgly".into()),
        filename: "pool/main/s/sample/sample_1.2.3_amd64.deb".into(),
        size: 42,
        md5: "md5".into(),
        sha1: "sha1".into(),
        sha256: "sha256".into(),
    };

    let entry = format_packages_entry(&record);
    let expected = "\
Package: sample
Version: 1.2.3
Architecture: amd64
Section: utils
Priority: optional
Maintainer: Pkgly <dev@pkgly>
Depends: libc6 (>= 2.31)
Installed-Size: 2048
Homepage: https://pkgly
Filename: pool/main/s/sample/sample_1.2.3_amd64.deb
Size: 42
MD5sum: md5
SHA1: sha1
SHA256: sha256
Description: summary
 details line

";
    assert_eq!(entry, expected);
}

#[test]
fn release_file_lists_all_hashes() {
    let entries = [ReleaseEntry {
        path: "dists/stable/main/binary-amd64/Packages".into(),
        size: 1024,
        md5: "aaa".into(),
        sha1: "bbb".into(),
        sha256: "ccc".into(),
    }];
    let release = build_release_file("stable", &["main".into()], &["amd64".into()], &entries);
    assert!(release.contains("Suite: stable"));
    assert!(release.contains("Components: main"));
    assert!(release.contains("aaa"));
    assert!(release.contains("SHA256:"));
    assert!(release.contains("dists/stable/main/binary-amd64/Packages"));
}

#[test]
fn release_file_aligns_hash_section_entries() {
    let entries = [ReleaseEntry {
        path: "dists/stable/main/binary-amd64/Packages".into(),
        size: 1024,
        md5: "aaa".into(),
        sha1: "bbb".into(),
        sha256: "ccc".into(),
    }];
    let release = build_release_file("stable", &["main".into()], &["amd64".into()], &entries);
    let lines: Vec<_> = release.lines().collect();

    // MD5 section header + entry directly after the empty spacer.
    let md5_section = lines
        .iter()
        .position(|line| *line == "MD5Sum:")
        .expect("MD5 section present");
    let expected_md5_entry = format!(
        " {} {:>16} {}",
        "aaa", 1024, "dists/stable/main/binary-amd64/Packages"
    );
    assert_eq!(lines[md5_section + 1], expected_md5_entry);

    let sha1_section = lines
        .iter()
        .position(|line| *line == "SHA1:")
        .expect("SHA1 section present");
    let expected_sha1_entry = format!(
        " {} {:>16} {}",
        "bbb", 1024, "dists/stable/main/binary-amd64/Packages"
    );
    assert_eq!(lines[sha1_section + 1], expected_sha1_entry);

    let sha256_section = lines
        .iter()
        .position(|line| *line == "SHA256:")
        .expect("SHA256 section present");
    let expected_sha256_entry = format!(
        " {} {:>16} {}",
        "ccc", 1024, "dists/stable/main/binary-amd64/Packages"
    );
    assert_eq!(lines[sha256_section + 1], expected_sha256_entry);
}
