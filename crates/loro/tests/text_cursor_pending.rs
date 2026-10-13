use loro::{cursor::Side, CommitOptions, ExportMode, LoroDoc, ID};
use std::sync::{Arc, Mutex};

#[test]
fn text_cursor_selects_each_run_id_without_committing_pending_edits() {
    let doc = LoroDoc::new();
    doc.set_peer_id(7).unwrap();
    let origins = Arc::new(Mutex::new(Vec::new()));
    let captured = origins.clone();
    let _sub = doc.subscribe_root(Arc::new(move |event| {
        captured.lock().unwrap().push(event.origin.to_string());
    }));
    let text = doc.get_text("text");
    // Separate insert calls with adjacent, consecutive IDs, plus an astral char.
    for (pos, content) in [(0, "ab"), (2, "😀"), (3, "cd")] {
        text.insert(pos, content).unwrap();
    }
    let vv = doc.oplog_vv();
    let frontiers = doc.state_frontiers();
    let pending = doc.get_pending_txn_len();
    assert!(pending > 0);
    for pos in 0..5 {
        let cursor = text.get_cursor(pos, Side::Middle).unwrap();
        assert_eq!(cursor.id, Some(ID::new(7, pos as i32)));
    }
    assert_eq!(doc.oplog_vv(), vv);
    assert_eq!(doc.state_frontiers(), frontiers);
    assert_eq!(doc.get_pending_txn_len(), pending);
    assert!(origins.lock().unwrap().is_empty());
    doc.commit_with(CommitOptions::new().origin("user-edit"));
    assert_eq!(*origins.lock().unwrap(), ["user-edit"]);

    let restored = LoroDoc::new();
    restored
        .import(&doc.export(ExportMode::Snapshot).unwrap())
        .unwrap();
    let text = restored.get_text("text");
    for pos in 0..5 {
        let cursor = text.get_cursor(pos, Side::Middle).unwrap();
        assert_eq!(cursor.id, Some(ID::new(7, pos as i32)));
        assert_eq!(restored.get_cursor_pos(&cursor).unwrap().current.pos, pos);
    }
}
