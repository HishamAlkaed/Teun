pub mod rag;
pub mod stream;
pub mod types;

/// Walk up from CWD to find the workspace root (directory containing
/// resources/acceptatie). Relocated from the deleted `claude.rs`
/// (Plan 01-04 T4) — used for skill-path resolution.
pub fn find_project_root() -> Option<String> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..5 {
        if dir.join("resources/acceptatie").is_dir() {
            return Some(dir.to_string_lossy().to_string());
        }
        if !dir.pop() {
            break;
        }
    }
    None
}
