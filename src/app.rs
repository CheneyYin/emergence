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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new_with_no_args() {
        let app = App::new(None, None);
        assert!(app.is_ok());
    }

    #[test]
    fn test_app_new_with_session_and_model() {
        let app = App::new(Some("sess-1".into()), Some("deepseek/v4".into()));
        assert!(app.is_ok());
    }

    #[tokio::test]
    async fn test_app_run_returns_ok() {
        let app = App::new(None, None).unwrap();
        let result = app.run().await;
        assert!(result.is_ok());
    }
}
