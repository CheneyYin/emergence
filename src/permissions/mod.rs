use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum RiskLevel {
    ReadOnly,
    Write,
    System,
}

/// 用户对权限弹窗的选择
#[derive(Debug, Clone)]
pub enum UserChoice {
    ApproveOnce,
    ApproveAlways,
    Deny,
}

/// 会话级权限白名单
#[derive(Debug, Default)]
pub struct PermissionStore {
    /// (tool_name, RiskLevel) 的永久批准集合
    always_allow: HashSet<(String, RiskLevel)>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self {
            always_allow: HashSet::new(),
        }
    }

    /// 检查工具是否已批准
    pub fn is_allowed(&self, tool_name: &str, risk: RiskLevel) -> bool {
        self.always_allow.contains(&(tool_name.to_string(), risk))
    }

    /// 添加永久批准
    pub fn approve_always(&mut self, tool_name: &str, risk: RiskLevel) {
        self.always_allow.insert((tool_name.to_string(), risk));
    }

    /// 清空白名单（会话关闭时调用）
    pub fn clear(&mut self) {
        self.always_allow.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that is_allowed() correctly returns true only after approve_always() is called for the matching tool and risk level.
    #[test]
    fn test_is_allowed() {
        let mut store = PermissionStore::new();
        assert!(!store.is_allowed("bash", RiskLevel::Write));

        store.approve_always("bash", RiskLevel::Write);
        assert!(store.is_allowed("bash", RiskLevel::Write));
        assert!(!store.is_allowed("bash", RiskLevel::System));
    }

    /// Verifies that clear() removes all previously granted permanent approvals.
    #[test]
    fn test_clear() {
        let mut store = PermissionStore::new();
        store.approve_always("bash", RiskLevel::Write);
        assert!(store.is_allowed("bash", RiskLevel::Write));
        store.clear();
        assert!(!store.is_allowed("bash", RiskLevel::Write));
    }

    /// Verifies that RiskLevel variants follow the expected ordering: ReadOnly < Write < System.
    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::ReadOnly < RiskLevel::Write);
        assert!(RiskLevel::Write < RiskLevel::System);
    }

    /// Verifies that permissions for different tools and risk levels are independently tracked.
    #[test]
    fn test_multiple_tools_independent() {
        let mut store = PermissionStore::new();
        store.approve_always("read", RiskLevel::ReadOnly);
        store.approve_always("bash", RiskLevel::Write);

        assert!(store.is_allowed("read", RiskLevel::ReadOnly));
        assert!(store.is_allowed("bash", RiskLevel::Write));
        assert!(!store.is_allowed("read", RiskLevel::Write));
        assert!(!store.is_allowed("bash", RiskLevel::ReadOnly));
    }

    /// Verifies that a single tool can be approved at multiple distinct risk levels.
    #[test]
    fn test_same_tool_multiple_risk_levels() {
        let mut store = PermissionStore::new();
        store.approve_always("bash", RiskLevel::Write);
        store.approve_always("bash", RiskLevel::System);

        assert!(store.is_allowed("bash", RiskLevel::Write));
        assert!(store.is_allowed("bash", RiskLevel::System));
        // ReadOnly 未被批准
        assert!(!store.is_allowed("bash", RiskLevel::ReadOnly));
    }

    /// Verifies that a default PermissionStore grants no permissions for any tool.
    #[test]
    fn test_default_is_empty() {
        let store = PermissionStore::default();
        assert!(!store.is_allowed("any", RiskLevel::ReadOnly));
    }
}
