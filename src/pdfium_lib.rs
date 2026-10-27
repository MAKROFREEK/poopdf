// poopdf - a local-first PDF viewer that reads documents aloud.
// Copyright (C) 2026  poopdf contributors <https://github.com/MAKROFREEK/poopdf>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! First-launch pdfium library provisioning (ADR-0001's startup contract):
//! download the pinned [bblanchon/pdfium-binaries][pin] release for the
//! running platform into the app cache, and hand out the shared library path
//! for [`pdfium_render::Pdfium::bind_to_library`].
//!
//! [pin]: https://github.com/bblanchon/pdfium-binaries/releases

use std::fmt;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The pinned pdfium binaries release (chromium build 8125, PDFium 158.0).
pub const PINNED_TAG: &str = "chromium/8125";

/// Base download URL, with the tag path-encoded as GitHub requires.
pub fn download_url(tag: &str, asset_name: &str) -> String {
    format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{}/{}",
        tag.replace('/', "%2F"),
        asset_name
    )
}

/// Which release asset matches the given OS/arch pair (None: unsupported).
pub fn asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("pdfium-linux-x64.tgz"),
        ("linux", "aarch64") => Some("pdfium-linux-arm64.tgz"),
        ("macos", "x86_64") => Some("pdfium-mac-x64.tgz"),
        ("macos", "aarch64") => Some("pdfium-mac-arm64.tgz"),
        ("windows", "x86_64") => Some("pdfium-win-x64.tgz"),
        ("windows", "aarch64") => Some("pdfium-win-arm64.tgz"),
        _ => None,
    }
}

/// The shared library file name on the given OS.
pub fn library_file_name(os: &str) -> &'static str {
    match os {
        "macos" => "libpdfium.dylib",
        "windows" => "pdfium.dll",
        _ => "libpdfium.so",
    }
}

/// Anything that can download a URL's body. Injected so tests never touch
/// the network.
pub trait Fetcher {
    fn fetch(&self, url: &str) -> std::io::Result<Vec<u8>>;
}

/// The production fetcher.
pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> std::io::Result<Vec<u8>> {
        let response = ureq::get(url).call().map_err(std::io::Error::other)?;
        let mut bytes = Vec::new();
        response.into_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// Full path of the shared library inside `dir`, and the pin marker's path.
fn provision_paths(dir: &Path, os: &str) -> (PathBuf, PathBuf) {
    (dir.join(library_file_name(os)), dir.join("pin.txt"))
}

/// The library counts as provisioned only when it exists and the pin marker
/// names the current tag.
fn is_provisioned(dir: &Path, tag: &str, os: &str) -> bool {
    let (lib_path, marker_path) = provision_paths(dir, os);
    let pinned = std::fs::read_to_string(marker_path)
        .map(|contents| contents.trim() == tag)
        .unwrap_or(false);
    pinned && lib_path.is_file()
}

/// Extract the shared library out of a gzip+tar archive into `dir`.
/// Returns the size of the written file.
fn extract_library(archive: &[u8], dir: &Path, os: &str) -> Result<(), PdfiumEnsureError> {
    let wanted = library_file_name(os);
    let decoder = flate2::read::GzDecoder::new(archive);
    let mut tarball = tar::Archive::new(decoder);
    let mut found = false;
    let mut files = tarball
        .entries()
        .map_err(|e| PdfiumEnsureError::Unpack(e.to_string()))?;
    loop {
        match files.next() {
            Some(Ok(mut entry)) => {
                let path = entry
                    .path()
                    .map_err(|e| PdfiumEnsureError::Unpack(e.to_string()))?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name == wanted {
                    let mut buffer = Vec::new();
                    entry
                        .read_to_end(&mut buffer)
                        .map_err(|e| PdfiumEnsureError::Unpack(e.to_string()))?;
                    std::fs::write(dir.join(wanted), buffer).map_err(PdfiumEnsureError::Io)?;
                    found = true;
                    break;
                }
            }
            Some(Err(e)) => return Err(PdfiumEnsureError::Unpack(e.to_string())),
            None => break,
        }
    }
    if found {
        Ok(())
    } else {
        Err(PdfiumEnsureError::Unpack(format!(
            "no {wanted} in the pinned archive"
        )))
    }
}

/// Make sure the pinned pdfium shared library for the given platform is
/// present in `dir`, downloading it from the pinned release when needed.
/// Returns the library's path, ready for `Pdfium::bind_to_library`.
pub fn ensure_dynamic_library(
    os: &str,
    arch: &str,
    dir: &Path,
    fetcher: &dyn Fetcher,
) -> Result<PathBuf, PdfiumEnsureError> {
    let asset = asset_name(os, arch).ok_or(PdfiumEnsureError::UnsupportedPlatform)?;
    if is_provisioned(dir, PINNED_TAG, os) {
        return Ok(provision_paths(dir, os).0);
    }
    std::fs::create_dir_all(dir).map_err(PdfiumEnsureError::Io)?;
    let bytes = fetcher
        .fetch(&download_url(PINNED_TAG, asset))
        .map_err(PdfiumEnsureError::Io)?;
    extract_library(&bytes, dir, os)?;
    std::fs::write(dir.join("pin.txt"), PINNED_TAG).map_err(PdfiumEnsureError::Io)?;
    Ok(dir.join(library_file_name(os)))
}

/// Provision the pinned pdfium library into the platform cache dir, for the
/// platform we're running on, using the network.
pub fn ensure_pinned_pdfium() -> Result<PathBuf, PdfiumEnsureError> {
    let os = std::env::consts::OS;
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("poopdf")
        .join("pdfium");
    ensure_dynamic_library(os, std::env::consts::ARCH, &dir, &HttpFetcher)
}

use pdfium_render::prelude::Pdfium;

/// Download (first launch) or locate the pinned pdfium library, bind it, and
/// leak the binding for the process lifetime. Process-wide: call once.
pub fn bind_pinned_pdfium() -> Result<&'static Pdfium, BindError> {
    let path = ensure_pinned_pdfium().map_err(BindError::Ensure)?;
    let bindings = Pdfium::bind_to_library(&path).map_err(BindError::Bind)?;
    Ok(Box::leak(Box::new(Pdfium::new(bindings))))
}

#[derive(Debug)]
pub enum BindError {
    Bind(pdfium_render::prelude::PdfiumError),
    Ensure(PdfiumEnsureError),
}

impl fmt::Display for BindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind(e) => write!(f, "could not load {PINNED_TAG} pdfium library: {e}"),
            Self::Ensure(e) => write!(f, "could not provision pdfium ({PINNED_TAG}): {e}"),
        }
    }
}

impl std::error::Error for BindError {}
#[derive(Debug)]
pub enum PdfiumEnsureError {
    UnsupportedPlatform,
    Unpack(String),
    Io(std::io::Error),
}

impl fmt::Display for PdfiumEnsureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(f, "no pinned pdfium asset for this platform"),
            Self::Unpack(what) => write!(f, "could not unpack pdfium archive: {what}"),
            Self::Io(e) => write!(f, "pdfium download failed: {e}"),
        }
    }
}

impl std::error::Error for PdfiumEnsureError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::Write as _;

    /// A fake network: record requested URLs, serve canned bytes.
    struct FakeNet {
        urls: RefCell<Vec<String>>,
        body: Result<Vec<u8>, String>,
    }

    impl FakeNet {
        fn ok(body: Vec<u8>) -> Self {
            Self {
                urls: RefCell::new(Vec::new()),
                body: Ok(body),
            }
        }

        fn failing() -> Self {
            Self {
                urls: RefCell::new(Vec::new()),
                body: Err("no network".to_string()),
            }
        }
    }

    impl Fetcher for FakeNet {
        fn fetch(&self, url: &str) -> std::io::Result<Vec<u8>> {
            self.urls.borrow_mut().push(url.to_string());
            match &self.body {
                Ok(bytes) => Ok(bytes.clone()),
                Err(message) => Err(std::io::Error::other(message.clone())),
            }
        }
    }

    /// Build an in-memory tar.gz containing one file at `path`.
    fn gzip_tar(path: &str, contents: &[u8]) -> Vec<u8> {
        let mut tarball = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_path(path).unwrap();
        header.set_mode(0o755);
        header.set_cksum();
        tarball.append_data(&mut header, path, contents).unwrap();
        let raw = tarball.into_inner().unwrap();

        let mut encoded = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoded.write_all(&raw).unwrap();
        encoded.finish().unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("poopdf-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn url_encodes_the_slash_in_the_tag() {
        assert_eq!(
            download_url("chromium/8125", "pdfium-linux-x64.tgz"),
            "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8125/pdfium-linux-x64.tgz"
        );
    }

    #[test]
    fn asset_names_cover_the_supported_matrix() {
        assert_eq!(asset_name("linux", "x86_64"), Some("pdfium-linux-x64.tgz"));
        assert_eq!(
            asset_name("linux", "aarch64"),
            Some("pdfium-linux-arm64.tgz")
        );
        assert_eq!(asset_name("macos", "aarch64"), Some("pdfium-mac-arm64.tgz"));
        assert_eq!(asset_name("macos", "x86_64"), Some("pdfium-mac-x64.tgz"));
        assert_eq!(asset_name("windows", "x86_64"), Some("pdfium-win-x64.tgz"));
        assert_eq!(
            asset_name("windows", "aarch64"),
            Some("pdfium-win-arm64.tgz")
        );
        assert_eq!(asset_name("freebsd", "x86_64"), None);
    }

    #[test]
    fn library_names_per_os() {
        assert_eq!(library_file_name("linux"), "libpdfium.so");
        assert_eq!(library_file_name("macos"), "libpdfium.dylib");
        assert_eq!(library_file_name("windows"), "pdfium.dll");
    }

    #[test]
    fn download_fetches_the_pinned_asset_url() {
        let dir = temp_dir("download-url");
        let net = FakeNet::ok(gzip_tar("lib/libpdfium.so", b"library"));

        ensure_dynamic_library("linux", "x86_64", &dir, &net).unwrap();

        let urls = net.urls.borrow();
        assert_eq!(urls.len(), 1);
        assert!(urls[0].ends_with("pdfium-linux-x64.tgz"), "{}", urls[0]);
        assert!(urls[0].contains(PINNED_TAG.replace('/', "%2F").as_str()));
    }

    #[test]
    fn download_writes_the_library_and_the_pin_marker() {
        let dir = temp_dir("download-writes");
        let archive = gzip_tar("lib/libpdfium.so", b"library-bytes");

        let path = ensure_dynamic_library("linux", "x86_64", &dir, &FakeNet::ok(archive)).unwrap();
        assert_eq!(path, dir.join("libpdfium.so"));
        assert_eq!(std::fs::read(&path).unwrap(), b"library-bytes");
        assert_eq!(
            std::fs::read(dir.join("pin.txt")).unwrap(),
            PINNED_TAG.as_bytes()
        );
    }

    #[test]
    fn already_provisioned_and_correctly_pinned_skips_the_download() {
        let dir = temp_dir("skip-download");
        std::fs::write(dir.join("libpdfium.so"), b"already-here").unwrap();
        std::fs::write(dir.join("pin.txt"), PINNED_TAG).unwrap();
        let net = FakeNet::failing(); // any fetch is a test failure via noisy None

        let path = ensure_dynamic_library("linux", "x86_64", &dir, &net).unwrap();
        assert_eq!(path, dir.join("libpdfium.so"));
        assert!(net.urls.borrow().is_empty());
    }

    #[test]
    fn wrong_pin_marker_triggers_redownload() {
        let dir = temp_dir("wrong-pin");
        std::fs::write(dir.join("libpdfium.so"), b"stale").unwrap();
        std::fs::write(dir.join("pin.txt"), "chromium/1").unwrap();

        let archive = gzip_tar("lib/libpdfium.so", b"fresh");
        let path = ensure_dynamic_library("linux", "x86_64", &dir, &FakeNet::ok(archive)).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"fresh");
        assert_eq!(
            std::fs::read(dir.join("pin.txt")).unwrap(),
            PINNED_TAG.as_bytes()
        );
    }

    #[test]
    fn network_failure_is_reported_and_caches_nothing() {
        let dir = temp_dir("net-fail");
        let err = ensure_dynamic_library("linux", "x86_64", &dir, &FakeNet::failing()).unwrap_err();
        assert!(matches!(err, PdfiumEnsureError::Io(_)));
        assert!(!dir.join("libpdfium.so").exists());
        assert!(!dir.join("pin.txt").exists());
    }

    #[test]
    fn missing_library_in_archive_is_reported() {
        let dir = temp_dir("no-lib");
        let archive = gzip_tar("README.txt", b"no library here");
        let err =
            ensure_dynamic_library("linux", "x86_64", &dir, &FakeNet::ok(archive)).unwrap_err();
        assert!(matches!(err, PdfiumEnsureError::Unpack(_)));
    }

    #[test]
    fn unsupported_platform_is_reported_without_fetching() {
        let dir = temp_dir("unsupported");
        let err = ensure_dynamic_library("freebsd", "x86_64", &dir, &FakeNet::ok(Vec::new()))
            .unwrap_err();
        assert!(matches!(err, PdfiumEnsureError::UnsupportedPlatform));
    }

    #[test]
    fn finds_the_library_whatever_directory_it_sits_in() {
        // Linux archives keep it at lib/, Windows at bin/: match on file name.
        let dir = temp_dir("lib-in-bin");
        let archive = gzip_tar("bin/pdfium.dll", b"win-lib");
        let path =
            ensure_dynamic_library("windows", "x86_64", &dir, &FakeNet::ok(archive)).unwrap();
        assert_eq!(path, dir.join("pdfium.dll"));
        assert_eq!(std::fs::read(path).unwrap(), b"win-lib");
    }
}
