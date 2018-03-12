use std::result;
use std::path::PathBuf;

use glob;

pub fn glob_for_certificates(path: String) -> result::Result<Vec<PathBuf>, glob::PatternError> {
    glob::glob(&format!("{}/*.pem", path)).map(|v| v.filter_map(|v| v.ok()).collect())
}
