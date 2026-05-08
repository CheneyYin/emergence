use super::*;
use std::sync::Mutex;

/// 创建内建监听器
pub fn create_builtin(listener: &str, config: serde_json::Value) -> anyhow::Result<Box<dyn HookExecutor>> {
    match listener {
        "log" => Ok(Box::new(LogListener::new(config)?)),
        "validate-tool" => Ok(Box::new(ValidateToolListener::new(config)?)),
        "notify" => Ok(Box::new(NotifyListener::new(config)?)),
        "rate-limit" => Ok(Box::new(RateLimitListener::new(config)?)),
        _ => anyhow::bail!("未知的内建监听器: {}", listener),
    }
}

/// 日志监听器 — 将事件 JSON 写入文件
pub struct LogListener {
    path: std::path::PathBuf,
    format: String,
}

impl LogListener {
    fn new(config: serde_json::Value) -> anyhow::Result<Self> {
        let path = config["path"].as_str().unwrap_or("~/.emergence/hooks.log");
        let format = config["format"].as_str().unwrap_or("json").to_string();
        let path = if path.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(&path[2..])
        } else {
            std::path::PathBuf::from(path)
        };
        Ok(Self { path, format })
    }
}

#[async_trait]
impl HookExecutor for LogListener {
    fn hook_type(&self) -> &str { "builtin:log" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        let log_entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "event": format!("{:?}", event),
        });

        let line = if self.format == "json" {
            serde_json::to_string(&log_entry)?
        } else {
            format!("{} | {:?}", chrono::Utc::now().to_rfc3339(), event)
        };

        // 追加写入
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", line)?;

        Ok(HookOutcome::Continue)
    }
}

/// 工具校验监听器 — 检查 deny_patterns
pub struct ValidateToolListener {
    deny_patterns: Vec<String>,
}

impl ValidateToolListener {
    fn new(config: serde_json::Value) -> anyhow::Result<Self> {
        let mut deny_patterns = Vec::new();
        if let Some(rules) = config["rules"].as_array() {
            for rule in rules {
                if let Some(patterns) = rule["deny_patterns"].as_array() {
                    for p in patterns {
                        deny_patterns.push(p.as_str().unwrap_or("").to_string());
                    }
                }
            }
        }
        Ok(Self { deny_patterns })
    }
}

#[async_trait]
impl HookExecutor for ValidateToolListener {
    fn hook_type(&self) -> &str { "builtin:validate-tool" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        let params = match event {
            HookEvent::PreToolExecute { params, .. } => {
                params["command"].as_str().unwrap_or("")
            }
            _ => "",
        };

        for pattern in &self.deny_patterns {
            if params.contains(pattern) {
                return Ok(HookOutcome::Abort {
                    reason: format!("匹配拒绝模式: {}", pattern),
                });
            }
        }

        Ok(HookOutcome::Continue)
    }
}

/// 通知监听器 — 系统通知 (notify-send)
pub struct NotifyListener;

impl NotifyListener {
    fn new(_config: serde_json::Value) -> anyhow::Result<Self> { Ok(Self) }
}

#[async_trait]
impl HookExecutor for NotifyListener {
    fn hook_type(&self) -> &str { "builtin:notify" }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        let message = format!("emergence: {:?}", event.event_type());
        let _ = std::process::Command::new("notify-send")
            .arg("emergence")
            .arg(&message)
            .spawn();
        Ok(HookOutcome::Continue)
    }
}

/// 频率限制监听器
pub struct RateLimitListener {
    limits: Mutex<std::collections::HashMap<String, (u32, std::time::Instant)>>,
    max_per_hour: u32,
}

impl RateLimitListener {
    fn new(config: serde_json::Value) -> anyhow::Result<Self> {
        let max_per_hour = config["max_per_hour"].as_u64().unwrap_or(50) as u32;
        Ok(Self { limits: Mutex::new(std::collections::HashMap::new()), max_per_hour })
    }
}

#[async_trait]
impl HookExecutor for RateLimitListener {
    fn hook_type(&self) -> &str { "builtin:rate-limit" }

    async fn execute(&self, _event: &HookEvent) -> anyhow::Result<HookOutcome> {
        let now = std::time::Instant::now();
        let hour = std::time::Duration::from_secs(3600);

        let mut limits = self.limits.lock().unwrap();
        let key = "default".to_string();
        let (count, timestamp) = limits.entry(key).or_insert((0, now));

        if now - *timestamp > hour {
            *count = 0;
            *timestamp = now;
        }

        *count += 1;
        if *count > self.max_per_hour {
            Ok(HookOutcome::Abort {
                reason: format!("超过频率限制 ({} 次/小时)", self.max_per_hour),
            })
        } else {
            Ok(HookOutcome::Continue)
        }
    }
}
