use std::collections::HashMap;
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
    pub source: SkillSource,
}

pub struct SkillRegistry {
    skills: HashMap<String, SkillMeta>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { skills: HashMap::new() }
    }

    pub fn list(&self) -> Vec<SkillMeta> {
        self.skills.values().cloned().collect()
    }

    pub fn load_full_content(&self, name: &str) -> anyhow::Result<String> {
        self.skills.get(name)
            .map(|_| String::new())
            .ok_or_else(|| anyhow::anyhow!("skill '{}' not found", name))
    }
}
