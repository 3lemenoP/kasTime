//! Shared test utilities and fixtures for ktcs-cli integration tests

use std::io::Write;
use tempfile::NamedTempFile;

/// Create a temporary file with the given content
pub fn temp_file_with_content(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    write!(file, "{}", content).expect("Failed to write temp file");
    file.flush().expect("Failed to flush temp file");
    file
}

/// Known SHA256 hash of "test content\n"
pub const TEST_CONTENT: &str = "test content\n";
pub const TEST_CONTENT_HASH: &str = "a1fff0ffefb9eace7230c24e50731f0a91c62f9cefdfe77121c2f607125dffae";
