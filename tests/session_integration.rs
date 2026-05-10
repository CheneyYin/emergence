use emergence::llm::{ChatMessage, Content, Role};
use emergence::session::store::{JsonFileStore, SessionStore};
use emergence::session::{Session, SessionKey, SessionManager};

fn make_user_msg(text: &str) -> ChatMessage {
    ChatMessage {
        role: Role::User,
        content: Content::Text(text.to_string()),
        name: None,
        tool_call_id: None,
    }
}

// ── Session + SessionManager ──

/// Verifies that a freshly created Session has the expected initial state: empty turns, no skills, no alias, zero messages.
#[test]
fn test_session_lifecycle() {
    let session = Session::new("integration-test-1".into());
    assert_eq!(session.id, "integration-test-1");
    assert!(session.turns.is_empty());
    assert!(session.active_skills.is_empty());
    assert!(session.alias.is_none());
    assert_eq!(session.message_count(), 0);
}

/// Verifies that SessionManager can complete multiple full turn cycles (begin_turn, push, complete_turn) and accumulate messages.
#[test]
fn test_session_manager_full_flow() {
    let mut sm = SessionManager::new("integration-flow".into());

    sm.begin_turn(make_user_msg("first question"));
    sm.push(make_user_msg("follow-up")).unwrap();
    sm.complete_turn().unwrap();

    sm.begin_turn(make_user_msg("second question"));
    sm.complete_turn().unwrap();

    assert_eq!(sm.turns().len(), 2);
    assert!(sm.session().message_count() > 0);
}

/// Verifies that SessionManager can activate and deactivate skills, tracking them as a list that persists across turns.
#[test]
fn test_session_manager_skills() {
    let mut sm = SessionManager::new("skills-test".into());

    sm.activate_skill("rust").unwrap();
    sm.activate_skill("typescript").unwrap();
    assert_eq!(sm.active_skills().len(), 2);

    sm.deactivate_skill("rust").unwrap();
    assert_eq!(sm.active_skills(), &["typescript"]);
}

/// Verifies that SessionManager.set_alias() is reflected in the underlying Session's alias field.
#[test]
fn test_session_manager_alias() {
    let mut sm = SessionManager::new("alias-test".into());
    sm.set_alias("my-alias".into());
    assert_eq!(sm.session().alias.as_deref(), Some("my-alias"));
}

// ── JsonFileStore persistence ──

/// Verifies that a SessionManager's session can be saved to JsonFileStore and reloaded by alias with all data intact.
#[tokio::test]
async fn test_save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let mut sm = SessionManager::new("persist-1".into());
    sm.set_alias("persisted-alias".into());
    sm.begin_turn(make_user_msg("persist me"));
    sm.complete_turn().unwrap();

    store.save(sm.session()).await.unwrap();

    let loaded = store
        .load(&SessionKey::Alias("persisted-alias".into()))
        .await
        .unwrap();
    assert!(loaded.is_some());
    let session = loaded.unwrap();
    assert_eq!(session.id, "persist-1");
    assert_eq!(session.message_count(), 1);
}

/// Verifies that JsonFileStore can list multiple sessions and delete a specific session by ID.
#[tokio::test]
async fn test_store_list_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    store.save(&Session::new("list-1".into())).await.unwrap();
    store.save(&Session::new("list-2".into())).await.unwrap();
    store.save(&Session::new("list-3".into())).await.unwrap();

    let list = store.list().await.unwrap();
    assert_eq!(list.len(), 3);

    store
        .delete(&SessionKey::Id("list-2".into()))
        .await
        .unwrap();
    let list = store.list().await.unwrap();
    assert_eq!(list.len(), 2);

    assert!(store
        .load(&SessionKey::Id("list-2".into()))
        .await
        .unwrap()
        .is_none());
}

/// Verifies that JsonFileStore.load() returns None for a session ID that does not exist.
#[tokio::test]
async fn test_store_load_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let result = store
        .load(&SessionKey::Id("nonexistent".into()))
        .await
        .unwrap();
    assert!(result.is_none());
}

/// Verifies that JsonFileStore can load a session by alias and by ID, and returns None for a nonexistent alias.
#[tokio::test]
async fn test_store_alias_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonFileStore::new(dir.path().to_path_buf());

    let mut sm = SessionManager::new("alias-resolve".into());
    sm.set_alias("quick-access".into());
    store.save(sm.session()).await.unwrap();

    // 通过 alias 加载
    let by_alias = store
        .load(&SessionKey::Alias("quick-access".into()))
        .await
        .unwrap();
    assert!(by_alias.is_some());

    // 通过 id 加载
    let by_id = store
        .load(&SessionKey::Id("alias-resolve".into()))
        .await
        .unwrap();
    assert!(by_id.is_some());

    // 不存在的 alias
    let missing = store
        .load(&SessionKey::Alias("no-such-alias".into()))
        .await
        .unwrap();
    assert!(missing.is_none());
}
