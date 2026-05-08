use super::*;

pub struct ToolsCommand;

#[async_trait::async_trait]
impl Command for ToolsCommand {
    fn name(&self) -> &str { "tools" }
    fn description(&self) -> &str { "列出可用工具及风险等级" }
    fn usage(&self) -> &str { "/tools" }

    async fn execute(&self, _args: &[String], _ctx: &mut CommandContext<'_>) -> anyhow::Result<CommandOutput> {
        Ok(CommandOutput::Success {
            message: concat!(
                "可用工具 (8):\n",
                "  read       [ReadOnly]  读取文件\n",
                "  write      [Write]     创建/覆盖文件\n",
                "  edit       [Write]     精确字符串替换\n",
                "  grep       [ReadOnly]  文本搜索\n",
                "  glob       [ReadOnly]  文件模式匹配\n",
                "  bash       [分级]      执行 shell 命令\n",
                "  web_fetch  [System]    HTTP GET\n",
                "  web_search [System]    搜索 API",
            ).to_string(),
        })
    }
}
