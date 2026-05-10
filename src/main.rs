use clap::Parser;

#[derive(Parser)]
#[command(name = "emergence", version = "0.1.0")]
struct Cli {
    /// 要加载的会话 ID 或别名
    #[arg(short, long)]
    session: Option<String>,

    /// 使用的模型，如 "deepseek/deepseek-v4-pro"
    #[arg(short, long)]
    model: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "emergence=info".into()),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!("emergence v0.1.0 启动");

    emergence::app::App::new(cli.session, cli.model)?
        .run()
        .await
}
