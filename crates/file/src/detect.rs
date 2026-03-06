//! Binary file detection.
//!
//! Samples the first 4KB and checks for null bytes.

use std::path::Path;

/// Check if a file appears to be binary by sampling the first 4KB.
pub fn is_binary(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(data) => is_binary_content(&data),
        Err(_) => false,
    }
}

/// Check if content appears to be binary (contains null bytes in first 4KB).
pub fn is_binary_content(data: &[u8]) -> bool {
    let sample = &data[..data.len().min(4096)];
    sample.contains(&0)
}

/// Known binary file extensions.
pub fn is_binary_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "ico"
            | "webp"
            | "svg"
            | "pdf"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "o"
            | "a"
            | "wasm"
            | "class"
            | "pyc"
            | "pyo"
            | "mp3"
            | "mp4"
            | "avi"
            | "mov"
            | "wav"
            | "flac"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "eot"
            | "sqlite"
            | "db"
            | "sqlite3"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_not_binary() {
        assert!(!is_binary_content(b"Hello, world!\nThis is text."));
    }

    #[test]
    fn null_bytes_are_binary() {
        assert!(is_binary_content(b"Hello\x00world"));
    }

    #[test]
    fn empty_is_not_binary() {
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn binary_extensions() {
        assert!(is_binary_extension(Path::new("image.png")));
        assert!(is_binary_extension(Path::new("archive.zip")));
        assert!(is_binary_extension(Path::new("font.woff2")));
        assert!(!is_binary_extension(Path::new("code.rs")));
        assert!(!is_binary_extension(Path::new("readme.md")));
    }
}
