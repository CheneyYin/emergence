pub struct App {
    session: Option<String>,
    model: Option<String>,
}

impl App {
    pub fn new(session: Option<String>, model: Option<String>) -> anyhow::Result<Self> {
        Ok(Self { session, model })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("App::run() — 占位，将在任务 27 中实现");
        Ok(())
    }
}
