use loro::{cursor::Side, ExportMode, LoroDoc, TextDelta, UndoManager};

#[test]
fn restoration_reports_original_scalar_ids_without_document_marks() {
    let doc = LoroDoc::new();
    doc.set_peer_id(1).unwrap();
    let text = doc.get_text("text");
    text.insert(0, "ab😀cd").unwrap();
    doc.commit();
    let original: Vec<_> = (0..5)
        .map(|i| text.get_cursor(i, Side::Middle).unwrap().id.unwrap())
        .collect();
    let peer = doc.fork();
    peer.set_peer_id(2).unwrap();
    let mut undo = UndoManager::new(&doc);
    undo.set_merge_interval(0);
    doc.inner().capture_text_restoration(true);
    text.delete(1, 3).unwrap();
    doc.commit();
    peer.get_text("text").insert(3, "X").unwrap();
    peer.commit();
    doc.import(&peer.export(ExportMode::Snapshot).unwrap())
        .unwrap();
    assert!(undo.undo_one().unwrap());
    let reports = doc.inner().take_text_restoration();
    let sources: Vec<_> = reports.iter().map(|(_, _, old)| *old).collect();
    assert_eq!(sources, original[1..4]);
    for (_, restored, _) in reports {
        assert!((0..text.len_unicode())
            .any(|i| text.get_cursor(i, Side::Middle).unwrap().id == Some(restored)));
    }
    assert!(text.to_delta().iter().all(|item| matches!(
        item,
        TextDelta::Insert {
            attributes: None,
            ..
        }
    )));
    assert!(undo.redo_one().unwrap());
    assert!(doc.inner().take_text_restoration().is_empty());
    assert!(undo.undo_one().unwrap());
    assert_eq!(doc.inner().take_text_restoration().len(), 3);
}

#[test]
fn restoration_capture_is_opt_in() {
    let doc = LoroDoc::new();
    let text = doc.get_text("text");
    text.insert(0, "abc").unwrap();
    doc.commit();
    let mut undo = UndoManager::new(&doc);
    text.delete(0, 3).unwrap();
    doc.commit();
    undo.undo_one().unwrap();
    assert!(doc.inner().take_text_restoration().is_empty());
}

#[test]
fn grouped_typing_skips_disjoint_excluded_metadata() {
    use loro::CommitOptions;
    let doc = LoroDoc::new();
    let text = doc.get_text("text");
    let metadata = doc.get_list("metadata");
    let mut undo = UndoManager::new(&doc);
    undo.set_merge_interval(0);
    undo.add_exclude_origin_prefix("metadata");
    undo.group_start().unwrap();
    for i in 0..20 {
        text.insert(i, "x").unwrap();
        doc.commit();
        metadata.push(i as i64).unwrap();
        doc.commit_with(CommitOptions::new().origin("metadata"));
    }
    undo.group_end();
    assert_eq!(undo.undo_count(), 1);
    undo.undo_one().unwrap();
    assert_eq!(text.to_string(), "");
    assert_eq!(metadata.len(), 20);
    undo.redo_one().unwrap();
    assert_eq!(text.to_string(), "x".repeat(20));
    assert_eq!(metadata.len(), 20);
}

#[test]
fn live_identity_lookup_does_not_recover_deleted_characters() {
    let doc = LoroDoc::new();
    let text = doc.get_text("text");
    text.insert(0, "a😀b").unwrap();
    doc.commit();
    let cursor = text.get_cursor(1, loro::cursor::Side::Middle).unwrap();
    assert_eq!(doc.inner().query_live_pos(&cursor), Some(1));
    text.delete(1, 1).unwrap();
    doc.commit();
    assert_eq!(doc.inner().query_live_pos(&cursor), None);
    assert!(doc.get_cursor_pos(&cursor).is_ok());
}
