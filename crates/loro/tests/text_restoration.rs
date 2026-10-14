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
