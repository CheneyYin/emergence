use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

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
        Self { skills: HashMap::new() }
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
                let name = path.file_stem()
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
            text.push_str(&format!("- skill: {} | desc: {}\n", meta.name, meta.description));
        }
        text.push_str("</available_skills>");
        text
    }

    /// 按需加载完整 content（去掉 frontmatter）
    pub fn load_full_content(&self, name: &str) -> anyhow::Result<String> {
        let meta = self.skills.get(name)
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
        let meta = registry.parse_frontmatter(&tmp.path().to_path_buf(), SkillSource::User).unwrap();
        assert_eq!(meta.name, "rust-expert");
        assert_eq!(meta.description, "Rust systems expert");
        assert_eq!(meta.allowed_tools, vec!["read", "write"]);
    }

    /// Verifies that load_full_content returns body text with frontmatter stripped.
    #[test]
    fn test_load_full_content_strips_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("test-skill.md");
        std::fs::write(&skill_path, "---\nname: test-skill\ndescription: test\n---\n\nThis is the body.\n").unwrap();

        let mut registry = SkillRegistry::new();
        registry.scan_dir(&dir.path().to_path_buf(), SkillSource::User).unwrap();
        let content = registry.load_full_content("test-skill").unwrap();
        assert_eq!(content, "This is the body.");
    }
}
