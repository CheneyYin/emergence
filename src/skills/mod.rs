use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub mod loader;

/// Skill 来源
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SkillSource {
    User,
    Project,
}

/// Skill 元信息（轻量，注入 system prompt）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub source: SkillSource,
    /// content 未加载，保留路径用于按需加载
    pub file_path: PathBuf,
}

/// Skill 文件 frontmatter
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    #[serde(rename = "allowed-tools")]
    allowed_tools: Vec<String>,
}

/// Skill 注册表
pub struct SkillRegistry {
    skills: HashMap<String, SkillMeta>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// 扫描两级目录加载 skill meta
    pub fn load(user_dir: Option<PathBuf>, project_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let mut registry = Self::new();

        // 1. 先加载用户级
        if let Some(ref dir) = user_dir {
            registry.scan_dir(dir, SkillSource::User)?;
        }

        // 2. 再加载项目级（覆盖同名）
        if let Some(ref dir) = project_dir {
            registry.scan_dir(dir, SkillSource::Project)?;
        }

        Ok(registry)
    }

    fn scan_dir(&mut self, dir: &PathBuf, source: SkillSource) -> anyhow::Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                if let Ok(meta) = self.parse_frontmatter(&path, source.clone()) {
                    self.skills.insert(meta.name.clone(), meta);
                }
            }
        }

        Ok(())
    }

    /// 解析 YAML frontmatter
    fn parse_frontmatter(&self, path: &PathBuf, source: SkillSource) -> anyhow::Result<SkillMeta> {
        let content = std::fs::read_to_string(path)?;

        // 提取 frontmatter (--- ... ---)
        let fm = if content.starts_with("---") {
            let end = content[3..].find("---").map(|i| i + 3).unwrap_or(0);
            if end > 3 {
                Some(&content[3..end])
            } else {
                None
            }
        } else {
            None
        };

        match fm {
            Some(fm_str) => {
                let fm: SkillFrontmatter = serde_yaml::from_str(fm_str)?;
                Ok(SkillMeta {
                    name: fm.name,
                    description: fm.description,
                    allowed_tools: fm.allowed_tools,
                    source,
                    file_path: path.clone(),
                })
            }
            None => {
                // 无 frontmatter：用文件名作为 name
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                Ok(SkillMeta {
                    name,
                    description: String::new(),
                    allowed_tools: Vec::new(),
                    source,
                    file_path: path.clone(),
                })
            }
        }
    }

    /// 格式化为 <available_skills> 注入文本
    pub fn format_available_for_prompt(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let mut text = String::from("<available_skills>\n");
        for (_, meta) in &self.skills {
            text.push_str(&format!(
                "- skill: {} | desc: {}\n",
                meta.name, meta.description
            ));
        }
        text.push_str("</available_skills>");
        text
    }

    /// 按需加载完整 content（去掉 frontmatter）
    pub fn load_full_content(&self, name: &str) -> anyhow::Result<String> {
        let meta = self
            .skills
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("skill 不存在: {}", name))?;

        let content = std::fs::read_to_string(&meta.file_path)?;

        // 去掉 frontmatter
        let body = if content.starts_with("---") {
            if let Some(end) = content[3..].find("---") {
                content[3 + end + 3..].trim().to_string()
            } else {
                content
            }
        } else {
            content
        };

        Ok(body)
    }

    /// 模糊匹配 skill（简单前缀/包含匹配）
    pub fn fuzzy_match(&self, query: &str) -> Option<&SkillMeta> {
        let query = query.to_lowercase();
        // 精确匹配
        if let Some(meta) = self.skills.get(&query) {
            return Some(meta);
        }
        // 前缀匹配
        for (name, meta) in &self.skills {
            if name.to_lowercase().starts_with(&query) {
                return Some(meta);
            }
        }
        // 包含匹配
        for (name, meta) in &self.skills {
            if name.to_lowercase().contains(&query) {
                return Some(meta);
            }
        }
        None
    }

    pub fn list(&self) -> Vec<&SkillMeta> {
        let mut metas: Vec<&SkillMeta> = self.skills.values().collect();
        metas.sort_by_key(|m| &m.name);
        metas
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Verifies that parse_frontmatter extracts name, description, and allowed_tools from YAML frontmatter.
    #[test]
    fn test_parse_frontmatter() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "---\nname: rust-expert\ndescription: Rust systems expert\nallowed-tools: [read, write]\n---\n\n## Role\nYou are a Rust expert.\n").unwrap();

        let registry = SkillRegistry::new();
        let meta = registry
            .parse_frontmatter(&tmp.path().to_path_buf(), SkillSource::User)
            .unwrap();
        assert_eq!(meta.name, "rust-expert");
        assert_eq!(meta.description, "Rust systems expert");
        assert_eq!(meta.allowed_tools, vec!["read", "write"]);
    }

    /// Verifies that load_full_content returns body text with frontmatter stripped.
    #[test]
    fn test_load_full_content_strips_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("test-skill.md");
        std::fs::write(
            &skill_path,
            "---\nname: test-skill\ndescription: test\n---\n\nThis is the body.\n",
        )
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry
            .scan_dir(&dir.path().to_path_buf(), SkillSource::User)
            .unwrap();
        let content = registry.load_full_content("test-skill").unwrap();
        assert_eq!(content, "This is the body.");
    }

    /// Verifies that load_full_content returns error for a skill name not in the registry.
    #[test]
    fn test_load_full_content_missing_skill() {
        let registry = SkillRegistry::new();
        let result = registry.load_full_content("nonexistent");
        assert!(result.is_err());
    }

    /// Verifies that parse_frontmatter falls back to filename as name when no YAML frontmatter is present.
    #[test]
    fn test_parse_no_frontmatter_uses_filename() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("my-skill.md");
        std::fs::write(&skill_path, "Just some markdown, no frontmatter.\n").unwrap();

        let registry = SkillRegistry::new();
        let meta = registry
            .parse_frontmatter(&skill_path.to_path_buf(), SkillSource::User)
            .unwrap();
        assert_eq!(meta.name, "my-skill");
        assert!(meta.description.is_empty());
        assert!(meta.allowed_tools.is_empty());
    }

    /// Verifies that scan_dir ignores non-.md files.
    #[test]
    fn test_scan_dir_ignores_non_md_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a skill").unwrap();
        std::fs::write(
            dir.path().join("skill.md"),
            "---\nname: skill-1\ndescription: desc\n---\nbody",
        )
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry
            .scan_dir(&dir.path().to_path_buf(), SkillSource::User)
            .unwrap();
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].name, "skill-1");
    }

    /// Verifies that scan_dir returns Ok when the directory does not exist.
    #[test]
    fn test_scan_dir_nonexistent_dir() {
        let mut registry = SkillRegistry::new();
        let result = registry.scan_dir(&PathBuf::from("/nonexistent/dir"), SkillSource::User);
        assert!(result.is_ok());
        assert!(registry.list().is_empty());
    }

    /// Verifies that format_available_for_prompt returns empty string for empty registry.
    #[test]
    fn test_format_available_empty_registry() {
        let registry = SkillRegistry::new();
        assert_eq!(registry.format_available_for_prompt(), "");
    }

    /// Verifies that format_available_for_prompt wraps skills in available_skills tags.
    #[test]
    fn test_format_available_with_skills() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rust.md"),
            "---\nname: rust\ndescription: Rust expert\n---\nbody",
        )
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry
            .scan_dir(&dir.path().to_path_buf(), SkillSource::User)
            .unwrap();
        let text = registry.format_available_for_prompt();
        assert!(text.contains("<available_skills>"));
        assert!(text.contains("rust"));
        assert!(text.contains("Rust expert"));
        assert!(text.contains("</available_skills>"));
    }

    /// Verifies that fuzzy_match performs exact, prefix, and contains matching, and returns None for no match.
    #[test]
    fn test_fuzzy_match_variants() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("typescript.md"),
            "---\nname: typescript\ndescription: TS\n---\nbody",
        )
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry
            .scan_dir(&dir.path().to_path_buf(), SkillSource::User)
            .unwrap();

        // exact match
        assert!(registry.fuzzy_match("typescript").is_some());
        // prefix match
        assert!(registry.fuzzy_match("type").is_some());
        // contains match
        assert!(registry.fuzzy_match("script").is_some());
        // no match
        assert!(registry.fuzzy_match("rust").is_none());
    }

    /// Verifies that load() scans both user and project directories with project overriding user for same-named skills.
    #[test]
    fn test_load_two_levels_project_overrides() {
        let user_dir = tempfile::tempdir().unwrap();
        let project_dir = tempfile::tempdir().unwrap();

        std::fs::write(
            user_dir.path().join("shared.md"),
            "---\nname: shared\ndescription: user version\n---\nuser body",
        )
        .unwrap();
        std::fs::write(
            user_dir.path().join("user-only.md"),
            "---\nname: user-only\ndescription: only user\n---\nbody",
        )
        .unwrap();
        std::fs::write(
            project_dir.path().join("shared.md"),
            "---\nname: shared\ndescription: project version\n---\nproject body",
        )
        .unwrap();

        let registry = SkillRegistry::load(
            Some(user_dir.path().to_path_buf()),
            Some(project_dir.path().to_path_buf()),
        )
        .unwrap();

        let metas = registry.list();
        assert_eq!(metas.len(), 2); // shared (project override) + user-only
        let shared = metas.iter().find(|m| m.name == "shared").unwrap();
        assert_eq!(shared.description, "project version");
        assert_eq!(shared.source, SkillSource::Project);
    }
}
