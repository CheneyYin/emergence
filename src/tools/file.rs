use super::*;
use crate::permissions::RiskLevel;

pub struct ReadTool;

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "读取文件内容，支持 offset/limit 分页"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "offset": { "type": "integer", "description": "起始行 (0-indexed)", "default": 0 },
                "limit": { "type": "integer", "description": "读取行数，默认 2000" }
            },
            "required": ["file_path"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 file_path 参数"))?;
        let offset = params["offset"].as_u64().unwrap_or(0) as usize;
        let limit = params["limit"].as_u64().unwrap_or(2000) as usize;

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("读取文件失败: {}", e))?;
        let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();
        let result = lines.join("\n");

        Ok(ToolOutput {
            content: format!(
                "{}(共 {} 行，显示第 {}-{} 行):\n{}",
                file_path,
                content.lines().count(),
                offset,
                offset + lines.len(),
                result,
            ),
            metadata: None,
        })
    }
}

pub struct WriteTool;

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "创建或覆盖文件"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "content": { "type": "string", "description": "要写入的内容" }
            },
            "required": ["file_path", "content"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 file_path 参数"))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 content 参数"))?;

        if let Some(parent) = std::path::Path::new(file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file_path, content)?;
        let size = content.len();

        Ok(ToolOutput {
            content: format!("成功写入 {} ({} 字节)", file_path, size),
            metadata: Some(serde_json::json!({"byte_count": size})),
        })
    }
}

pub struct EditTool;

#[async_trait::async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "精确字符串替换——在文件中查找 old_string 并替换为 new_string"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "文件路径" },
                "old_string": { "type": "string", "description": "要替换的文本（必须精确匹配且唯一）" },
                "new_string": { "type": "string", "description": "替换后的文本" }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 file_path 参数"))?;
        let old_string = params["old_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 old_string 参数"))?;
        let new_string = params["new_string"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 new_string 参数"))?;

        if old_string == new_string {
            return Ok(ToolOutput {
                content: "old_string 和 new_string 相同，无需修改".into(),
                metadata: None,
            });
        }

        let content = std::fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("读取文件失败: {}", e))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            anyhow::bail!("未找到匹配的文本: {}", old_string);
        }
        if count > 1 {
            anyhow::bail!(
                "找到 {} 处匹配，old_string 必须唯一。请在 old_string 前后添加更多上下文。",
                count
            );
        }

        let edited = content.replacen(old_string, new_string, 1);
        std::fs::write(file_path, &edited)?;

        Ok(ToolOutput {
            content: format!("成功替换 {} 中的 1 处匹配", file_path),
            metadata: Some(serde_json::json!({"replacements": 1})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::RiskLevel;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ---------- ReadTool ----------

    /// Verifies that ReadTool reads file content and includes expected text in the output.
    #[tokio::test]
    async fn test_read_file() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "line1\nline2\nline3").unwrap();
        let tool = ReadTool;
        let params = serde_json::json!({"file_path": f.path()});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("line1"));
    }

    /// Verifies that ReadTool respects offset and limit parameters to return only a subset of lines.
    #[tokio::test]
    async fn test_read_file_with_offset_limit() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "line1\nline2\nline3\nline4").unwrap();
        let tool = ReadTool;
        let params = serde_json::json!({"file_path": f.path(), "offset": 1, "limit": 2});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("line2"));
        assert!(output.content.contains("line3"));
        assert!(!output.content.contains("line4"));
    }

    /// Verifies that ReadTool reports ReadOnly risk level.
    #[test]
    fn test_read_risk_level() {
        let tool = ReadTool;
        assert_eq!(tool.risk_level(&serde_json::json!({})), RiskLevel::ReadOnly);
    }

    /// Verifies that ReadTool returns the correct name and a non-empty description.
    #[test]
    fn test_read_name_and_description() {
        let tool = ReadTool;
        assert_eq!(tool.name(), "read");
        assert!(tool.description().contains("读取"));
    }

    /// Verifies that ReadTool returns an error when the target file does not exist.
    #[tokio::test]
    async fn test_read_nonexistent_file_returns_error() {
        let tool = ReadTool;
        let params = serde_json::json!({"file_path": "/nonexistent/path/xyz.txt"});
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    // ---------- WriteTool ----------

    /// Verifies that WriteTool creates a file with the expected content on disk.
    #[tokio::test]
    async fn test_write_file() {
        let path = std::env::temp_dir().join("emergence_test_write.txt");
        let tool = WriteTool;
        let params = serde_json::json!({"file_path": path, "content": "hello world"});
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("成功"));
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "hello world");
        std::fs::remove_file(&path).ok();
    }

    /// Verifies that WriteTool reports Write risk level.
    #[test]
    fn test_write_risk_level() {
        let tool = WriteTool;
        assert_eq!(tool.risk_level(&serde_json::json!({})), RiskLevel::Write);
    }

    /// Verifies that WriteTool returns the correct name and a non-empty description.
    #[test]
    fn test_write_name_and_description() {
        let tool = WriteTool;
        assert_eq!(tool.name(), "write");
        assert!(tool.description().contains("创建"));
    }

    /// Verifies that WriteTool's output metadata includes the correct byte count of written content.
    #[tokio::test]
    async fn test_write_metadata_contains_byte_count() {
        let path = std::env::temp_dir().join("emergence_test_metadata.txt");
        let tool = WriteTool;
        let params = serde_json::json!({"file_path": path, "content": "1234567890"});
        let output = tool.execute(params).await.unwrap();
        assert_eq!(output.metadata.unwrap()["byte_count"], 10);
        std::fs::remove_file(&path).ok();
    }

    // ---------- EditTool ----------

    /// Verifies that EditTool performs an exact string replacement and updates the file on disk.
    #[tokio::test]
    async fn test_edit_file_replace() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello world").unwrap();
        let path = f.path().to_path_buf();

        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": path,
            "old_string": "hello",
            "new_string": "hi",
        });
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("成功"));
        let edited = std::fs::read_to_string(&path).unwrap();
        assert_eq!(edited, "hi world");
    }

    /// Verifies that EditTool returns an error when the target file does not exist.
    #[tokio::test]
    async fn test_edit_file_not_found_returns_error() {
        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": "/nonexistent/path/test.txt",
            "old_string": "hello",
            "new_string": "hi",
        });
        let result = tool.execute(params).await;
        assert!(result.is_err());
    }

    /// Verifies that EditTool reports Write risk level.
    #[test]
    fn test_edit_risk_level() {
        let tool = EditTool;
        assert_eq!(tool.risk_level(&serde_json::json!({})), RiskLevel::Write);
    }

    /// Verifies that EditTool returns the correct name and a non-empty description.
    #[test]
    fn test_edit_name_and_description() {
        let tool = EditTool;
        assert_eq!(tool.name(), "edit");
        assert!(tool.description().contains("精确字符串替换"));
    }

    /// Verifies that EditTool returns a no-op result when old_string equals new_string without modifying the file.
    #[tokio::test]
    async fn test_edit_same_string_returns_noop() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "unchanged").unwrap();
        let path = f.path().to_path_buf();

        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": path,
            "old_string": "hello",
            "new_string": "hello",
        });
        let output = tool.execute(params).await.unwrap();
        assert!(output.content.contains("相同"));
        assert!(output.metadata.is_none());
        // 文件未被修改
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "unchanged");
    }

    /// Verifies that EditTool returns an error when old_string is not found in the file.
    #[tokio::test]
    async fn test_edit_no_match_returns_error() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "some content").unwrap();
        let path = f.path().to_path_buf();

        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": path,
            "old_string": "nonexistent_text",
            "new_string": "replacement",
        });
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("未找到匹配"));
    }

    /// Verifies that EditTool returns an error when old_string appears multiple times and is not unique.
    #[tokio::test]
    async fn test_edit_multiple_matches_returns_error() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "dup dup dup").unwrap();
        let path = f.path().to_path_buf();

        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": path,
            "old_string": "dup",
            "new_string": "replaced",
        });
        let result = tool.execute(params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("3 处匹配"));
    }

    /// Verifies that EditTool's output metadata includes the correct replacement count.
    #[tokio::test]
    async fn test_edit_metadata_contains_replacements() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "hello world").unwrap();
        let path = f.path().to_path_buf();

        let tool = EditTool;
        let params = serde_json::json!({
            "file_path": path,
            "old_string": "hello",
            "new_string": "hi",
        });
        let output = tool.execute(params).await.unwrap();
        assert_eq!(output.metadata.unwrap()["replacements"], 1);
    }
}
