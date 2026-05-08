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
