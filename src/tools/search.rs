use super::*;
use crate::permissions::RiskLevel;
use std::process::Command;

pub struct GrepTool;

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str { "grep" }
    fn description(&self) -> &str { "在文件中搜索文本模式" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "搜索模式（支持正则）" },
                "path": { "type": "string", "description": "搜索路径，默认当前目录", "default": "." },
                "include": { "type": "string", "description": "文件过滤 glob，如 '*.rs'" },
            },
            "required": ["pattern"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::ReadOnly }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 pattern 参数"))?;
        let path = params["path"].as_str().unwrap_or(".");
        let include = params["include"].as_str();

        let mut cmd = Command::new("grep");
        cmd.arg("-rn").arg("-I").arg("--color=never");
        cmd.arg(pattern).arg(path);

        // 排除隐藏目录
        cmd.arg("--exclude-dir=.git");
        cmd.arg("--exclude-dir=.emergence");
        cmd.arg("--exclude-dir=target");
        cmd.arg("--exclude-dir=node_modules");

        if let Some(glob) = include {
            cmd.arg("--include").arg(glob);
        }

        let output = cmd.output().map_err(|e| anyhow::anyhow!("grep 执行失败: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let result = if stdout.trim().is_empty() {
            "未找到匹配结果".to_string()
        } else if stdout.lines().count() > 500 {
            let limited: Vec<&str> = stdout.lines().take(500).collect();
            format!("{}(... 截断，共 {} 行，显示前 500 行)", limited.join("\n"), stdout.lines().count())
        } else {
            stdout.to_string()
        };

        Ok(ToolOutput {
            content: result,
            metadata: Some(serde_json::json!({"match_count": stdout.lines().count()})),
        })
    }
}

pub struct GlobTool;

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str { "glob" }
    fn description(&self) -> &str { "按文件模式匹配查找文件" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "文件 glob 模式，如 'src/**/*.rs'" },
                "path": { "type": "string", "description": "搜索路径，默认当前目录", "default": "." }
            },
            "required": ["pattern"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::ReadOnly }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 pattern 参数"))?;
        let path = params["path"].as_str().unwrap_or(".");

        let output = Command::new("find")
            .arg(path)
            .arg("-path")
            .arg(pattern)
            .arg("-not")
            .arg("-path")
            .arg("*/.*")
            .arg("-not")
            .arg("-path")
            .arg("*/target/*")
            .arg("-not")
            .arg("-path")
            .arg("*/node_modules/*")
            .output()
            .map_err(|e| anyhow::anyhow!("find 执行失败: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let result = if stdout.trim().is_empty() {
            "未找到匹配文件".to_string()
        } else {
            stdout.to_string()
        };

        Ok(ToolOutput {
            content: result,
            metadata: Some(serde_json::json!({"file_count": stdout.lines().count()})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::RiskLevel;

    /// Verifies that GrepTool declares "pattern" as a required parameter.
    #[test]
    fn test_grep_parameters() {
        let tool = GrepTool;
        let params = tool.parameters();
        assert!(params["required"].as_array().unwrap().contains(&serde_json::json!("pattern")));
    }

    /// Verifies that GrepTool reports ReadOnly risk level.
    #[test]
    fn test_grep_risk_level() {
        assert_eq!(GrepTool.risk_level(&serde_json::json!({})), RiskLevel::ReadOnly);
    }

    /// Verifies that GlobTool reports ReadOnly risk level.
    #[test]
    fn test_glob_risk_level() {
        assert_eq!(GlobTool.risk_level(&serde_json::json!({})), RiskLevel::ReadOnly);
    }

    /// Verifies that GlobTool finds files matching the given pattern in the specified path.
    #[tokio::test]
    async fn test_glob_finds_files() {
        let tool = GlobTool;
        let params = serde_json::json!({"pattern": "*main.rs", "path": "."});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("main.rs"));
    }

    /// Verifies that GrepTool finds text matching the given pattern in Rust source files.
    #[tokio::test]
    async fn test_grep_finds_pattern() {
        let tool = GrepTool;
        let params = serde_json::json!({"pattern": "fn main", "path": "src", "include": "*.rs"});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("main"));
    }

    /// Verifies that GrepTool returns the correct name and a non-empty description.
    #[test]
    fn test_grep_name_and_description() {
        let tool = GrepTool;
        assert_eq!(tool.name(), "grep");
        assert!(tool.description().contains("搜索"));
    }

    /// Verifies that GlobTool returns the correct name and a non-empty description.
    #[test]
    fn test_glob_name_and_description() {
        let tool = GlobTool;
        assert_eq!(tool.name(), "glob");
        assert!(tool.description().contains("文件模式"));
    }

    /// Verifies that GlobTool declares "pattern" as a required parameter.
    #[test]
    fn test_glob_parameters() {
        let tool = GlobTool;
        let params = tool.parameters();
        assert!(params["required"].as_array().unwrap().contains(&serde_json::json!("pattern")));
    }

    /// Verifies that GlobTool returns a "no matches" message when no files match the pattern.
    #[tokio::test]
    async fn test_glob_empty_result() {
        let tool = GlobTool;
        let params = serde_json::json!({"pattern": "nonexistent_*.xyz", "path": "."});
        let output = tool.execute(params).await.unwrap();
        assert_eq!(output.content, "未找到匹配文件");
    }

    /// Verifies that GrepTool returns a "no matches" message when the pattern is not found.
    #[tokio::test]
    async fn test_grep_empty_result() {
        let tool = GrepTool;
        // 运行时生成随机字符串，避免匹配到测试源码自身
        let random_pattern = format!("NOTFOUND_{}", std::process::id());
        let params = serde_json::json!({"pattern": random_pattern, "path": "src"});
        let output = tool.execute(params).await.unwrap();
        assert_eq!(output.content, "未找到匹配结果");
    }

    /// Verifies that GlobTool's output metadata includes a positive file count.
    #[tokio::test]
    async fn test_glob_metadata_file_count() {
        let tool = GlobTool;
        let params = serde_json::json!({"pattern": "*.rs", "path": "src"});
        let output = tool.execute(params).await.unwrap();
        let file_count = output.metadata.unwrap()["file_count"].as_u64().unwrap();
        assert!(file_count > 0);
    }

    /// Verifies that GrepTool's output metadata includes a positive match count.
    #[tokio::test]
    async fn test_grep_metadata_match_count() {
        let tool = GrepTool;
        let params = serde_json::json!({"pattern": "fn", "path": "src", "include": "*.rs"});
        let output = tool.execute(params).await.unwrap();
        let match_count = output.metadata.unwrap()["match_count"].as_u64().unwrap();
        assert!(match_count > 0);
    }
}
