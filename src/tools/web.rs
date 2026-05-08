use super::*;
use crate::permissions::RiskLevel;

pub struct WebFetchTool;

#[async_trait::async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }
    fn description(&self) -> &str { "发起 HTTP GET 请求，提取页面内容（转为 markdown）" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "要获取的 URL（会自动升级 HTTP 到 HTTPS）"
                },
                "prompt": {
                    "type": "string",
                    "description": "用于提取特定信息的提示（可选）"
                }
            },
            "required": ["url"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::System }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let url = params["url"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 url 参数"))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("emergence/0.1.0")
            .build()?;

        let response = client.get(url).send().await
            .map_err(|e| anyhow::anyhow!("HTTP 请求失败: {}", e))?;

        let status = response.status();
        let body = response.text().await?;

        // 简单 HTML → text：去除所有 HTML 标签
        let text = strip_html_tags(&body);

        Ok(ToolOutput {
            content: format!("状态码: {}\n\n{}",
                status,
                if text.len() > 10000 { format!("{}...(截断)", &text[..10000]) } else { text }
            ),
            metadata: Some(serde_json::json!({
                "status_code": status.as_u16(),
                "content_length": body.len(),
            })),
        })
    }
}

fn strip_html_tags(html: &str) -> String {
    let re = regex::Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(html, "");
    let text = text.replace("&nbsp;", " ").replace("&amp;", "&")
        .replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"");
    // 合并多余空白
    let re_ws = regex::Regex::new(r"\n\s*\n").unwrap();
    re_ws.replace_all(&text, "\n\n").trim().to_string()
}

pub struct WebSearchTool;

#[async_trait::async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }
    fn description(&self) -> &str { "调用搜索 API 返回搜索结果" }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "搜索查询词" }
            },
            "required": ["query"]
        })
    }

    fn risk_level(&self, _params: &serde_json::Value) -> RiskLevel { RiskLevel::System }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let query = params["query"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少 query 参数"))?;

        // v1: 使用 DuckDuckGo HTML 重定向（无需 API key）
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("emergence/0.1.0")
            .build()?;

        let body = client.get(&url).send().await?.text().await?;

        // 简单提取搜索结果
        let results = extract_search_results(&body);

        Ok(ToolOutput {
            content: if results.is_empty() { "未找到结果".into() } else { results.join("\n\n") },
            metadata: Some(serde_json::json!({"result_count": results.len()})),
        })
    }
}

fn extract_search_results(html: &str) -> Vec<String> {
    let mut results = Vec::new();
    // 简单解析 DuckDuckGo HTML 结果
    let re_link = regex::Regex::new(r#"<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>([^<]*)</a>"#).unwrap();

    for cap in re_link.captures_iter(html).take(10) {
        let url = html_escape::decode_html_entities(&cap[1]).to_string();
        let title = html_escape::decode_html_entities(&cap[2]).to_string();
        results.push(format!("- [{}]({})", title.trim(), url.trim()));
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::RiskLevel;

    #[test]
    fn test_strip_html_tags() {
        let html = "<html><body><p>Hello</p><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_web_fetch_risk() {
        assert_eq!(
            WebFetchTool.risk_level(&serde_json::json!({})),
            RiskLevel::System
        );
    }

    #[test]
    fn test_web_search_risk() {
        assert_eq!(
            WebSearchTool.risk_level(&serde_json::json!({})),
            RiskLevel::System
        );
    }

    #[test]
    fn test_web_fetch_name_and_parameters() {
        let tool = WebFetchTool;
        assert_eq!(tool.name(), "web_fetch");
        let params = tool.parameters();
        assert!(params["required"].as_array().unwrap().contains(&serde_json::json!("url")));
    }

    #[test]
    fn test_web_search_name_and_parameters() {
        let tool = WebSearchTool;
        assert_eq!(tool.name(), "web_search");
        let params = tool.parameters();
        assert!(params["required"].as_array().unwrap().contains(&serde_json::json!("query")));
    }

    #[test]
    fn test_strip_html_entities() {
        let html = "<p>&amp; &lt; &gt; &quot; &nbsp;</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("&"));
        assert!(text.contains("<"));
        assert!(text.contains(">"));
        assert!(text.contains("\""));
        assert!(!text.contains("nbsp"));
    }

    #[test]
    fn test_strip_html_multiline() {
        let html = "<div>\n\n<p>A</p>\n\n\n<p>B</p>\n\n</div>";
        let text = strip_html_tags(html);
        // 多余空行应合并为单个空行
        assert!(!text.contains("\n\n\n"));
    }
}
