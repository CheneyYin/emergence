use super::SkillRegistry;
use std::path::PathBuf;

impl SkillRegistry {
    /// 创建默认 loader：扫描 ~/.emergence/skills/ 和 ./.emergence/skills/
    pub fn load_default() -> anyhow::Result<Self> {
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .ok();

        let user_skills_dir = home_dir.map(|h| h.join(".emergence").join("skills"));
        let project_skills_dir = std::env::current_dir()
            .ok()
            .map(|d| d.join(".emergence").join("skills"));

        Self::load(user_skills_dir, project_skills_dir)
    }
}
