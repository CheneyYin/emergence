# emergence

[![CI](https://github.com/CheneyYin/emergence/actions/workflows/ci.yml/badge.svg)](https://github.com/CheneyYin/emergence/actions/workflows/ci.yml)
[![Nightly](https://github.com/CheneyYin/emergence/actions/workflows/nightly.yml/badge.svg)](https://github.com/CheneyYin/emergence/actions/workflows/nightly.yml)

Claude Code 风格的智能体 CLI 工具，Rust + tokio + ratatui 构建，支持多 LLM 提供商、流式 Markdown 渲染、工具调用、会话管理。

## 特性

- **TUI 终端界面** — ratatui 全屏终端 UI，鼠标滚动、光标编辑、Markdown 实时渲染
- **多提供商** — OpenAI 兼容 API（DeepSeek、Groq 等），可扩展适配器
- **流式 Markdown** — 代码块高亮、表格（含外框）、粗体/斜体/标题/引用
- **工具调用** — Bash、文件读写、搜索（grep/glob）、网页抓取/搜索，自动权限分级
- **会话管理** — JSON 文件持久化，别名、压缩、摘要
- **技能系统** — Markdown 技能文件，模糊匹配加载，项目级别覆盖
- **Hook 系统** — 事件驱动钩子（PreLLMCall / PostLLMCall / UserInput 等）
- **权限控制** — ReadOnly / Write / System 三级，一次性/永久/拒绝

## 安装

### 预编译二进制

从 [Releases](https://github.com/CheneyYin/emergence/releases) 下载最新的 Nightly Build 对应平台二进制。

### 从源码构建

```bash
git clone git@github.com:CheneyYin/emergence.git
cd emergence
cargo build --release
```

## 使用

```bash
# 启动 TUI
emergence

# 指定模型
emergence --model deepseek-v4-pro

# 恢复会话
emergence --session abc123
```

### TUI 快捷键

| 键 | 功能 |
|---|------|
| `Enter` / `Ctrl-S` | 提交输入 |
| `Ctrl-C` | 取消流式输出 |
| `Left` / `Right` | 移动光标 |
| `Home` / `End` | 行首/行尾 |
| `Up` / `Down` | 浏览输入历史 |
| `Esc` | 清空输入 |
| 鼠标滚轮 | 滚动对话 |
| `a` | 允许工具（单次） |
| `y` | 允许工具（永久） |
| `d` / `Esc` | 拒绝工具 |

### 内置命令

| 命令 | 功能 |
|------|------|
| `/help` | 显示帮助 |
| `/model` | 查看/切换模型 |
| `/sessions` | 管理会话（list/load/delete/alias） |
| `/skills` | 管理技能（list/activate/deactivate） |
| `/tools` | 列出可用工具 |
| `/config` | 查看/重载配置 |
| `/tokens` | 显示 token 用量 |
| `/clear` | 清空当前会话 |
| `/compact` | 显示压缩状态 |
| `/quit` | 退出 |

## 配置

配置文件 `~/.config/emergence/settings.json`：

```json
{
  "provider": "deepseek",
  "model": "deepseek-v4-pro",
  "generation": {
    "max_tokens": 32000,
    "temperature": 0.0,
    "thinking": 16000
  },
  "session": {
    "store_dir": "~/.local/share/emergence/sessions",
    "compaction_threshold_tokens": 80000
  }
}
```

项目级配置 `.emergence/settings.json` 会覆盖用户级。

### 提供商配置

```json
{
  "providers": {
    "deepseek": {
      "base_url": "https://api.deepseek.com/v1",
      "api_key": "${DEEPSEEK_API_KEY}",
      "models": [
        { "id": "deepseek-v4-pro", "name": "DeepSeek V4 Pro", "max_tokens": 128000 }
      ]
    }
  }
}
```

环境变量 `${VAR}` 自动展开。

## 架构

```
src/
├── main.rs          # 入口
├── app.rs           # Agent 状态机、LLM 调用、流处理、工具执行
├── protocol.rs      # Action / Event 消息通道
├── config/          # 配置管理（用户级 + 项目级）
├── llm/             # LLM 适配器（OpenAI）、消息格式、Provider 注册
├── tui/             # 终端 UI（ratatui）
│   ├── mod.rs       # 主循环、状态、键盘/鼠标处理
│   ├── widgets.rs   # 渲染（对话面板、状态栏、输入框）
│   ├── markdown.rs  # pulldown-cmark → ratatui 样式
│   ├── themes.rs    # 主题颜色
│   └── popups.rs    # 权限弹窗
├── tools/           # 工具注册与执行
│   ├── bash.rs      # Shell 命令
│   ├── file.rs      # 文件读写/编辑
│   ├── search.rs    # grep / glob 搜索
│   └── web.rs       # 网页抓取/搜索
├── commands/        # 内置命令（/help、/sessions 等）
├── session/         # 会话持久化、上下文构建、摘要压缩
├── skills/          # 技能系统（Markdown 加载、模糊匹配）
├── hooks/           # 事件钩子系统
├── permissions/     # 工具权限分级
└── utils/           # 环境变量展开、模糊匹配
```

## 测试

```bash
cargo test    # 266 单元测试 + 62 集成测试 = 328 total
```
