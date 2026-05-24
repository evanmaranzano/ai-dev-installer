use ai_dev_installer::storage::db::HistoryDatabase;
use ai_dev_installer::storage::history::DEFAULT_SESSION_ID;

#[test]
fn inserts_and_reads_chat_history_rows() {
    let db = HistoryDatabase::open_in_memory().unwrap();

    db.insert_chat_message(DEFAULT_SESSION_ID, "user", "hello").unwrap();

    let rows = db.list_messages(DEFAULT_SESSION_ID).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].role, "user");
    assert_eq!(rows[0].content, "hello");
}
