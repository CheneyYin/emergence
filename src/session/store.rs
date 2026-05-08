use std::path::PathBuf;
use async_trait::async_trait;
use super::{Session, SessionId, SessionKey, SessionMeta};

/// 会话持久化 trait
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: &Session) -> anyhow::Result<()>;
    async fn load(&self, key: &SessionKey) -> anyhow::Result<Option<Session>>;
    async fn list(&self) -> anyhow::Result<Vec<SessionMeta>>;
    async fn delete(&self, key: &SessionKey) -> anyhow::Result<()>;
    async fn set_alias(&self, id: &str, alias: &str) -> anyhow::Result<()>;
}

/// JSON 文件存储实现
pub struct JsonFileStore {
    store_dir: PathBuf,
}

impl JsonFileStore {
    pub fn new(store_dir: PathBuf) -> Self {
        Self { store_dir }
    }

    fn index_path(&self) -> PathBuf {
        self.store_dir.join("index.json")
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.store_dir.join(format!("{}.json", id))
    }

    async fn read_index(&self) -> anyhow::Result<Vec<SessionMeta>> {
        if self.index_path().exists() {
            let content = tokio::fs::read_to_string(self.index_path()).await?;
            let metas: Vec<SessionMeta> = serde_json::from_str(&content)?;
            return Ok(metas);
        }
        Ok(Vec::new())
    }

    async fn write_index(&self, metas: &[SessionMeta]) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.store_dir).await?;
        let json = serde_json::to_string_pretty(metas)?;
        tokio::fs::write(self.index_path(), json).await?;
        Ok(())
    }

    /// 解析别名到 SessionId
    async fn resolve_key(&self, key: &SessionKey) -> anyhow::Result<Option<SessionId>> {
        match key {
            SessionKey::Id(id) => {
                if self.session_path(id).exists() {
                    Ok(Some(id.clone()))
                } else {
                    Ok(None)
                }
            }
            SessionKey::Alias(alias) => {
                let index = self.read_index().await?;
                let found = index.iter().find(|m| m.alias.as_deref() == Some(alias));
                Ok(found.map(|m| m.id.clone()))
            }
        }
    }
}

#[async_trait]
impl SessionStore for JsonFileStore {
    async fn save(&self, session: &Session) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.store_dir).await?;

        // 保存会话文件
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(self.session_path(&session.id), json).await?;

        // 更新索引
        let mut index = self.read_index().await?;
        let meta = SessionMeta {
            id: session.id.clone(),
            alias: session.alias.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            message_count: session.message_count(),
            summary: session.summary.clone(),
        };

        if let Some(pos) = index.iter().position(|m| m.id == session.id) {
            index[pos] = meta;
        } else {
            index.push(meta);
        }

        self.write_index(&index).await
    }

    async fn load(&self, key: &SessionKey) -> anyhow::Result<Option<Session>> {
        let id = self.resolve_key(key).await?;
        match id {
            Some(session_id) => {
                let path = self.session_path(&session_id);
                if path.exists() {
                    let json = tokio::fs::read_to_string(path).await?;
                    let session: Session = serde_json::from_str(&json)?;
                    Ok(Some(session))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn list(&self) -> anyhow::Result<Vec<SessionMeta>> {
        self.read_index().await
    }

    async fn delete(&self, key: &SessionKey) -> anyhow::Result<()> {
        let id = self.resolve_key(key).await?;
        if let Some(session_id) = id {
            // 删除会话文件
            let path = self.session_path(&session_id);
            if path.exists() {
                tokio::fs::remove_file(path).await?;
            }
            // 更新索引
            let mut index = self.read_index().await?;
            index.retain(|m| m.id != session_id);
            self.write_index(&index).await?;
        }
        Ok(())
    }

    async fn set_alias(&self, id: &str, alias: &str) -> anyhow::Result<()> {
        let mut index = self.read_index().await?;
        if let Some(meta) = index.iter_mut().find(|m| m.id == id) {
            meta.alias = Some(alias.to_string());
            self.write_index(&index).await?;

            // 同时更新会话文件中的别名
            if let Some(session) = self.load(&SessionKey::Id(id.to_string())).await? {
                let mut session = session;
                session.alias = Some(alias.to_string());
                let json = serde_json::to_string_pretty(&session)?;
                tokio::fs::write(self.session_path(id), json).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Session, SessionKey};

    /// Verifies that a session can be saved and then loaded back with the same ID.
    #[tokio::test]
    async fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        let session = Session::new("2026-05-06-test".into());
        store.save(&session).await.unwrap();

        let loaded = store.load(&SessionKey::Id("2026-05-06-test".into())).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "2026-05-06-test");
    }

    /// Verifies that list() returns all saved sessions.
    #[tokio::test]
    async fn test_list_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        store.save(&Session::new("s1".into())).await.unwrap();
        store.save(&Session::new("s2".into())).await.unwrap();

        let list = store.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    /// Verifies that a session set with an alias can be loaded via SessionKey::Alias.
    #[tokio::test]
    async fn test_set_alias_and_load_by_alias() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        store.save(&Session::new("s1".into())).await.unwrap();
        store.set_alias("s1", "my-session").await.unwrap();

        let loaded = store.load(&SessionKey::Alias("my-session".into())).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, "s1");
    }

    /// Verifies that a deleted session cannot be loaded.
    #[tokio::test]
    async fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::new(dir.path().to_path_buf());

        store.save(&Session::new("to-delete".into())).await.unwrap();
        store.delete(&SessionKey::Id("to-delete".into())).await.unwrap();

        let loaded = store.load(&SessionKey::Id("to-delete".into())).await.unwrap();
        assert!(loaded.is_none());
    }
}
