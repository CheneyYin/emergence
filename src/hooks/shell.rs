use super::*;
use std::process::Stdio;

pub struct ShellExecutor {
    command: String,
    timeout_ms: u64,
    abort_on_error: bool,
}

impl ShellExecutor {
    pub fn new(command: String, timeout_ms: u64, abort_on_error: bool) -> Self {
        Self {
            command,
            timeout_ms,
            abort_on_error,
        }
    }
}

#[async_trait]
impl HookExecutor for ShellExecutor {
    fn hook_type(&self) -> &str {
        "shell"
    }

    async fn execute(&self, event: &HookEvent) -> anyhow::Result<HookOutcome> {
        // 模板变量替换：从 HookEvent 提取 payload
        let mut cmd = self.command.clone();
        if let HookEvent::PreToolExecute { tool, .. } = event {
            cmd = cmd.replace("{{tool}}", tool);
        }
        if let HookEvent::PostToolExecute { tool, .. } = event {
            cmd = cmd.replace("{{tool}}", tool);
        }

        // 将整个事件序列化为 JSON 通过 stdin 传入子进程
        let event_json = serde_json::to_string(event)?;

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(self.timeout_ms),
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .and_then(|mut child| {
                        use std::io::Write;
                        if let Some(stdin) = child.stdin.as_mut() {
                            let _ = stdin.write_all(event_json.as_bytes());
                        }
                        child.wait_with_output()
                    })
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Shell hook 超时 ({}ms)", self.timeout_ms))?
        .map_err(|e| anyhow::anyhow!("Shell hook 错误: {}", e))?;

        match result {
            Ok(output) => {
                if output.status.success() || !self.abort_on_error {
                    Ok(HookOutcome::Continue)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Ok(HookOutcome::Abort {
                        reason: format!("Shell hook 失败: {}", stderr),
                    })
                }
            }
            Err(e) => {
                if self.abort_on_error {
                    Ok(HookOutcome::Abort {
                        reason: e.to_string(),
                    })
                } else {
                    tracing::warn!("Shell hook 错误: {}", e);
                    Ok(HookOutcome::Continue)
                }
            }
        }
    }
}
