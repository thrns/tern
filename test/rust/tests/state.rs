use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use tern_core::error::ErrorKind;
use tern_core::state::{Conversation, Role, StateStore, StoredMessage};

const FAR_FUTURE_MS: i64 = 9_000_000_000_000;

fn scratch_db(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let dir = env::temp_dir().join(format!("tern-state-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("state.db")
}

fn open_store(name: &str) -> StateStore {
    StateStore::open(&scratch_db(name)).expect("open state store")
}

fn message(role: Role, text: &str, created_unix_ms: i64) -> StoredMessage {
    StoredMessage {
        role,
        text: text.to_owned(),
        attachments: Vec::new(),
        created_unix_ms,
    }
}

fn summary_ids(summaries: &[(Conversation, u32)]) -> Vec<&str> {
    summaries
        .iter()
        .map(|(conversation, _)| conversation.id.as_str())
        .collect()
}

#[test]
fn open_rejects_unopenable_path() {
    let missing_dir = scratch_db("unopenable").join("missing").join("state.db");
    assert!(StateStore::open(&missing_dir).is_err_and(|err| err.kind == ErrorKind::Database));
}

#[test]
fn open_keeps_data_across_reopen() {
    let path = scratch_db("reopen");
    let created = {
        let store = StateStore::open(&path).expect("open state store");
        store
            .create_conversation("claude", "hello")
            .expect("create conversation")
    };
    let reopened = StateStore::open(&path).expect("reopen state store");
    assert_eq!(
        reopened.get_conversation(&created.id).expect("get"),
        Some(created)
    );
}

#[test]
fn create_conversation_assigns_uuid_and_timestamps() {
    let store = open_store("create");
    let conversation = store
        .create_conversation("claude", "what is this")
        .expect("create conversation");
    assert_eq!(conversation.id.len(), 36);
    assert_eq!(conversation.id.matches('-').count(), 4);
    assert_eq!(conversation.provider_id, "claude");
    assert_eq!(conversation.provider_session_id, None);
    assert_eq!(conversation.title, "what is this");
    assert_eq!(conversation.created_unix_ms, conversation.updated_unix_ms);
    assert!(conversation.created_unix_ms > 0);

    let other = store
        .create_conversation("claude", "second")
        .expect("create second conversation");
    assert_ne!(conversation.id, other.id);
}

#[test]
fn title_is_single_line_and_capped_at_sixty_chars() {
    let store = open_store("title-shape");
    let multi = store
        .create_conversation("claude", "first line\n  second\tline")
        .expect("create");
    assert_eq!(multi.title, "first line second line");

    let long = store
        .create_conversation("claude", &"a".repeat(100))
        .expect("create");
    assert_eq!(long.title.chars().count(), 60);

    let accented = store
        .create_conversation("claude", &"é".repeat(70))
        .expect("create");
    assert_eq!(accented.title, "é".repeat(60));

    let trailing_space = store
        .create_conversation("claude", &format!("{} b", "a".repeat(59)))
        .expect("create");
    assert_eq!(trailing_space.title, "a".repeat(59));
}

#[test]
fn empty_prompt_gets_image_question_title() {
    let store = open_store("title-empty");
    for prompt in ["", "   \n\t "] {
        let conversation = store.create_conversation("claude", prompt).expect("create");
        assert_eq!(conversation.title, "Image question");
    }
}

#[test]
fn get_conversation_returns_none_for_unknown_id() {
    let store = open_store("get-missing");
    assert_eq!(store.get_conversation("missing").expect("get"), None);
}

#[test]
fn set_session_id_stores_provider_session_without_touching_timestamps() {
    let store = open_store("session");
    let conversation = store.create_conversation("claude", "hi").expect("create");
    store
        .set_session_id(&conversation.id, "session-1")
        .expect("set session id");
    let fetched = store
        .get_conversation(&conversation.id)
        .expect("get")
        .expect("conversation exists");
    assert_eq!(fetched.provider_session_id.as_deref(), Some("session-1"));
    assert_eq!(fetched.updated_unix_ms, conversation.updated_unix_ms);
}

#[test]
fn set_session_id_rejects_unknown_conversation() {
    let store = open_store("session-missing");
    let err = store
        .set_session_id("missing", "session-1")
        .expect_err("unknown id");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

#[test]
fn append_message_stores_message_and_advances_updated_timestamp() {
    let store = open_store("append");
    let conversation = store.create_conversation("claude", "hi").expect("create");
    let created = conversation.created_unix_ms + 5_000;
    store
        .append_message(&conversation.id, &message(Role::User, "hi", created))
        .expect("append");
    let fetched = store
        .get_conversation(&conversation.id)
        .expect("get")
        .expect("conversation exists");
    assert_eq!(fetched.updated_unix_ms, created);
    assert_eq!(
        store.messages(&conversation.id).expect("messages"),
        vec![message(Role::User, "hi", created)]
    );
}

#[test]
fn append_message_never_moves_updated_timestamp_backwards() {
    let store = open_store("append-backwards");
    let conversation = store.create_conversation("claude", "hi").expect("create");
    store
        .append_message(
            &conversation.id,
            &message(Role::User, "late", conversation.created_unix_ms - 5_000),
        )
        .expect("append");
    let fetched = store
        .get_conversation(&conversation.id)
        .expect("get")
        .expect("conversation exists");
    assert_eq!(fetched.updated_unix_ms, conversation.updated_unix_ms);
}

#[test]
fn append_message_round_trips_attachments_with_non_ascii_paths() {
    let store = open_store("attachments");
    let conversation = store.create_conversation("claude", "look").expect("create");
    let attachments = vec![
        "/Users/zoë/Desktop/Ångström 📷.png".to_owned(),
        "/tmp/with space.png".to_owned(),
    ];
    store
        .append_message(
            &conversation.id,
            &StoredMessage {
                role: Role::User,
                text: "look".to_owned(),
                attachments: attachments.clone(),
                created_unix_ms: conversation.created_unix_ms,
            },
        )
        .expect("append");
    let stored = store.messages(&conversation.id).expect("messages");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].attachments, attachments);
}

#[test]
fn append_message_rejects_unknown_conversation_and_stores_nothing() {
    let store = open_store("append-missing");
    let err = store
        .append_message("missing", &message(Role::User, "hi", FAR_FUTURE_MS))
        .expect_err("unknown id");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
    assert!(store.messages("missing").expect("messages").is_empty());
}

#[test]
fn messages_keep_insertion_order_and_all_roles() {
    let store = open_store("messages-order");
    let conversation = store
        .create_conversation("claude", "question")
        .expect("create");
    let entries = [
        message(Role::User, "question", 300),
        message(Role::Assistant, "answer", 200),
        message(Role::Error, "failed", 100),
    ];
    for entry in &entries {
        store
            .append_message(&conversation.id, entry)
            .expect("append");
    }
    assert_eq!(
        store.messages(&conversation.id).expect("messages"),
        entries.to_vec()
    );
}

#[test]
fn messages_for_unknown_conversation_is_empty() {
    let store = open_store("messages-missing");
    assert!(store.messages("missing").expect("messages").is_empty());
}

#[test]
fn list_conversations_orders_by_updated_desc_with_counts_and_limit() {
    let store = open_store("list");
    let first = store.create_conversation("claude", "a").expect("create");
    let second = store.create_conversation("claude", "b").expect("create");
    let third = store.create_conversation("claude", "c").expect("create");

    store
        .append_message(&first.id, &message(Role::User, "a1", FAR_FUTURE_MS + 1))
        .expect("append");
    store
        .append_message(
            &first.id,
            &message(Role::Assistant, "a2", FAR_FUTURE_MS + 2),
        )
        .expect("append");
    store
        .append_message(&second.id, &message(Role::User, "b1", FAR_FUTURE_MS + 10))
        .expect("append");

    let all = store.list_conversations(10).expect("list");
    assert_eq!(
        summary_ids(&all),
        vec![second.id.as_str(), first.id.as_str(), third.id.as_str()]
    );
    let counts: Vec<u32> = all.iter().map(|(_, count)| *count).collect();
    assert_eq!(counts, vec![1, 2, 0]);

    let limited = store.list_conversations(2).expect("list");
    assert_eq!(
        summary_ids(&limited),
        vec![second.id.as_str(), first.id.as_str()]
    );
}

#[test]
fn prune_keeps_newest_conversations_and_removes_their_messages() {
    let store = open_store("prune");
    let mut conversations = Vec::new();
    for index in 0..5 {
        let conversation = store
            .create_conversation("claude", &format!("prompt {index}"))
            .expect("create");
        store
            .append_message(
                &conversation.id,
                &message(Role::User, "text", FAR_FUTURE_MS + index * 10),
            )
            .expect("append");
        conversations.push(conversation);
    }

    let deleted = store.prune(2).expect("prune");
    assert_eq!(deleted, 3);

    let remaining = store.list_conversations(10).expect("list");
    assert_eq!(
        summary_ids(&remaining),
        vec![conversations[4].id.as_str(), conversations[3].id.as_str()]
    );
    assert_eq!(
        store.get_conversation(&conversations[0].id).expect("get"),
        None
    );
    assert!(
        store
            .messages(&conversations[0].id)
            .expect("messages")
            .is_empty()
    );
    assert_eq!(store.prune(2).expect("prune again"), 0);
}

#[test]
fn shrink_memory_succeeds() {
    let store = open_store("shrink");
    store.shrink_memory().expect("shrink memory");
}

#[test]
fn concurrent_appends_from_two_threads_are_all_stored() {
    let store = open_store("concurrent");
    let conversation = store
        .create_conversation("claude", "parallel")
        .expect("create");
    let conversation_id = conversation.id.as_str();
    thread::scope(|scope| {
        for worker in 0..2_i64 {
            let store = &store;
            scope.spawn(move || {
                for index in 0..100_i64 {
                    store
                        .append_message(
                            conversation_id,
                            &message(
                                Role::User,
                                &format!("{worker}-{index}"),
                                FAR_FUTURE_MS + worker * 100 + index,
                            ),
                        )
                        .expect("append");
                }
            });
        }
    });
    assert_eq!(
        store.messages(&conversation.id).expect("messages").len(),
        200
    );
}
