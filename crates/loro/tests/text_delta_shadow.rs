use loro::{
    event::Diff, ExpandType, ExportMode, LoroDoc, LoroValue, StyleConfig, StyleConfigMap,
    TextDelta, UndoManager,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

// Native events count Unicode scalars, not bytes or UTF-16 code units.
type Atom = (char, BTreeMap<String, LoroValue>);

fn event_width(c: char) -> usize {
    // The dependency's wasm feature can be exercised on a native test runner.
    // Detect its event coordinates rather than assuming the facade's features.
    static UTF16: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *UTF16.get_or_init(|| {
        let doc = loro_internal::LoroDoc::new_auto_commit();
        let text = doc.get_text("probe");
        text.insert(0, "😀", loro::cursor::PosType::Unicode)
            .unwrap();
        text.len_event() == 2
    }) {
        c.len_utf16()
    } else {
        1
    }
}

fn scalar_end(shadow: &[Atom], start: usize, units: usize) -> usize {
    let mut left = units;
    let mut end = start;
    while left > 0 {
        let width = event_width(shadow.get(end).expect("delta outside shadow").0);
        assert!(left >= width, "delta splits a surrogate pair");
        left -= width;
        end += 1;
    }
    end
}

fn apply(shadow: &mut Vec<Atom>, delta: &[TextDelta]) {
    let mut pos = 0;
    for op in delta {
        match op {
            TextDelta::Retain { retain, attributes } => {
                let end = scalar_end(shadow, pos, *retain);
                if let Some(attrs) = attributes {
                    for (_, marks) in &mut shadow[pos..end] {
                        for (key, value) in attrs {
                            if value == &LoroValue::Null {
                                marks.remove(key);
                            } else {
                                marks.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }
                pos = end;
            }
            TextDelta::Insert { insert, attributes } => {
                let marks: BTreeMap<_, _> = attributes
                    .iter()
                    .flat_map(|a| a.iter())
                    .filter(|(_, v)| *v != &LoroValue::Null)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let atoms: Vec<_> = insert.chars().map(|c| (c, marks.clone())).collect();
                let len = atoms.len();
                shadow.splice(pos..pos, atoms);
                pos += len;
            }
            TextDelta::Delete { delete } => {
                let end = scalar_end(shadow, pos, *delete);
                shadow.drain(pos..end);
            }
        }
    }
}

fn tracked(peer: u64) -> (LoroDoc, Arc<Mutex<Vec<Atom>>>, loro::Subscription) {
    let doc = LoroDoc::new();
    doc.set_peer_id(peer).unwrap();
    let mut config = StyleConfigMap::new();
    config.insert(
        "after".into(),
        StyleConfig {
            expand: ExpandType::After,
        },
    );
    config.insert(
        "none".into(),
        StyleConfig {
            expand: ExpandType::None,
        },
    );
    doc.config_text_style(config);
    let shadow = Arc::new(Mutex::new(Vec::new()));
    let captured = shadow.clone();
    let subscription = doc.subscribe_root(Arc::new(move |event| {
        for container in &event.events {
            if let Diff::Text(delta) = &container.diff {
                apply(&mut captured.lock().unwrap(), delta);
            }
        }
    }));
    (doc, shadow, subscription)
}

fn check(doc: &LoroDoc, shadow: &Mutex<Vec<Atom>>, context: &str) {
    let text = doc.get_text("text");
    let actual = shadow.lock().unwrap();
    assert_eq!(
        actual.iter().map(|a| a.0).collect::<String>(),
        text.to_string(),
        "{context}"
    );
    let mut expected = Vec::new();
    apply(&mut expected, &text.to_delta());
    assert_eq!(*actual, expected, "{context}");
}

#[test]
fn backwards_batched_deletes_with_astral_prefix_emit_a_valid_delta() {
    let (doc, shadow, _sub) = tracked(1);
    let text = doc.get_text("text");
    text.insert(0, "😀x").unwrap();
    doc.commit();
    text.delete(1, 1).unwrap();
    text.delete(0, 1).unwrap();
    doc.commit();
    check(&doc, &shadow, "backwards astral deletes");
}

/// Replay every event into an independent string + per-character marks model.
/// Fixed seeds retain concurrent histories, batched deletes, expansion boundaries,
/// unmarks and astral scalars as ordinary suite coverage.
#[test]
fn emitted_text_deltas_match_shadow_after_commits_and_concurrent_imports() {
    let seeds = std::env::var("LORO_TEXT_DELTA_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12u64);
    for seed in 0..seeds {
        let mut rng = StdRng::seed_from_u64(seed);
        let (a, sa, _a) = tracked(1);
        let (b, sb, _b) = tracked(2);
        let initial = "a😀bc文𐐷0123456789".repeat(20);
        a.get_text("text").insert(0, &initial).unwrap();
        for i in 0..30 {
            let start = i * 10;
            a.get_text("text")
                .mark(
                    start..start + 7,
                    if i % 2 == 0 { "after" } else { "none" },
                    true,
                )
                .unwrap();
        }
        a.commit();
        b.import(&a.export(ExportMode::all_updates()).unwrap())
            .unwrap();
        check(&a, &sa, "initial A");
        check(&b, &sb, "initial B");
        let mut ua = UndoManager::new(&a);
        let mut ub = UndoManager::new(&b);
        ua.set_merge_interval(0);
        ub.set_merge_interval(0);
        let docs = [&a, &b];
        let shadows = [&sa, &sb];
        let mut history = Vec::new();
        let mut versions = [vec![a.state_frontiers()], vec![b.state_frontiers()]];
        for step in 0..160 {
            let peer = rng.gen_range(0..2);
            let doc = docs[peer];
            let text = doc.get_text("text");
            for _ in 0..rng.gen_range(1..=5) {
                let len = text.len_unicode();
                let op = rng.gen_range(0..5);
                if len == 0 || op == 0 {
                    let pos = rng.gen_range(0..=len);
                    let content = ["abc", "😀", "文𐐷", "x", "0123456789"][rng.gen_range(0..5)];
                    history.push(format!("{peer}: insert({pos}, {content:?})"));
                    text.insert(pos, content).unwrap();
                } else {
                    let start = rng.gen_range(0..len);
                    let end = rng.gen_range(start + 1..=(start + 8).min(len));
                    let key = ["after", "none"][rng.gen_range(0..2)];
                    match op {
                        1 | 2 => {
                            history.push(format!("{peer}: delete({start}, {})", end - start));
                            text.delete(start, end - start).unwrap();
                        }
                        3 => {
                            history.push(format!("{peer}: mark({start}..{end}, {key})"));
                            text.mark(start..end, key, true).unwrap();
                        }
                        _ => {
                            history.push(format!("{peer}: unmark({start}..{end}, {key})"));
                            text.unmark(start..end, key).unwrap();
                        }
                    }
                }
            }
            if std::env::var_os("LORO_TEXT_DELTA_TRACE").is_some() {
                eprintln!("seed={seed} step={step} history={history:?}");
            }
            doc.commit();
            history.push(format!("{peer}: commit"));
            check(
                doc,
                shadows[peer],
                &format!("seed={seed} step={step} history={history:?}"),
            );
            versions[peer].push(doc.state_frontiers());
            if rng.gen_ratio(1, 5) {
                let version = &versions[peer][rng.gen_range(0..versions[peer].len())];
                history.push(format!("{peer}: checkout {version:?}"));
                doc.checkout(version).unwrap();
                check(
                    doc,
                    shadows[peer],
                    &format!("seed={seed} step={step} history={history:?}"),
                );
                history.push(format!("{peer}: checkout latest"));
                doc.checkout_to_latest();
                check(
                    doc,
                    shadows[peer],
                    &format!("seed={seed} step={step} history={history:?}"),
                );
            }
            if rng.gen_ratio(1, 5) {
                let undo = if peer == 0 { &mut ua } else { &mut ub };
                history.push(format!("{peer}: undo"));
                undo.undo_one().unwrap();
                check(
                    doc,
                    shadows[peer],
                    &format!("seed={seed} step={step} history={history:?}"),
                );
                if rng.gen() {
                    history.push(format!("{peer}: redo"));
                    undo.redo_one().unwrap();
                    check(
                        doc,
                        shadows[peer],
                        &format!("seed={seed} step={step} history={history:?}"),
                    );
                }
            }
            if rng.gen_ratio(1, 3) {
                let other = 1 - peer;
                docs[other]
                    .import(&doc.export(ExportMode::all_updates()).unwrap())
                    .unwrap();
                history.push(format!("{peer} -> {other}: import"));
                check(
                    docs[other],
                    shadows[other],
                    &format!("seed={seed} step={step} history={history:?}"),
                );
            }
        }
        b.import(&a.export(ExportMode::all_updates()).unwrap())
            .unwrap();
        a.import(&b.export(ExportMode::all_updates()).unwrap())
            .unwrap();
        check(&a, &sa, "final sync A");
        check(&b, &sb, "final sync B");
        assert_eq!(a.get_text("text").to_delta(), b.get_text("text").to_delta());
    }
}
