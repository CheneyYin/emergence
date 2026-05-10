use std::path::Path;

/// 从项目目录加载 AGENTS.md
pub fn load_agents_md(project_dir: &Path) -> Option<String> {
    let path = project_dir.join(".emergence").join("AGENTS.md");
    if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    }
}

/// 从用户目录加载 AGENTS.md
pub fn load_user_agents_md(home_dir: &Path) -> Option<String> {
    let path = home_dir.join(".emergence").join("AGENTS.md");
    if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Verifies that load_agents_md returns Some(content) when AGENTS.md exists.
    #[test]
    fn test_load_agents_md_exists() {
        let dir = TempDir::new().unwrap();
        let emergence = dir.path().join(".emergence");
        fs::create_dir_all(&emergence).unwrap();
        fs::write(emergence.join("AGENTS.md"), "# Project Rules\n- Use tabs").unwrap();

        let content = load_agents_md(dir.path());
        assert!(content.is_some());
        assert!(content.unwrap().contains("Use tabs"));
    }

    /// Verifies that load_agents_md returns None when no AGENTS.md file exists.
    #[test]
    fn test_load_agents_md_missing() {
        let dir = TempDir::new().unwrap();
        let content = load_agents_md(dir.path());
        assert!(content.is_none());
    }

    /// Verifies that load_user_agents_md returns Some when the file exists.
    #[test]
    fn test_load_user_agents_md_exists() {
        let dir = TempDir::new().unwrap();
        let emergence = dir.path().join(".emergence");
        fs::create_dir_all(&emergence).unwrap();
        fs::write(emergence.join("AGENTS.md"), "user instructions").unwrap();

        let content = load_user_agents_md(dir.path());
        assert!(content.is_some());
        assert_eq!(content.unwrap(), "user instructions");
    }

    /// Verifies that load_user_agents_md returns None when no file exists.
    #[test]
    fn test_load_user_agents_md_missing() {
        let dir = TempDir::new().unwrap();
        let content = load_user_agents_md(dir.path());
        assert!(content.is_none());
    }
}
