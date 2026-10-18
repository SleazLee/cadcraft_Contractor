#![cfg(feature = "contractor")]
use cadcraft_contractor::{self as model, Shape};
use cadcraft_doc::{EntityKind, Handle};
use cadcraft_engine::Session;
use serde_json::{Value, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_drawing(false);
    s
}
fn board(s: &mut Session, p: Value) -> Handle {
    let r = s.execute("contractor.board", &p).unwrap();
    Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap()
}

#[test]
fn stack_repeat_edit_copy_and_undo_share_one_model() {
    let mut s = session();
    let beam = board(&mut s, json!({"start":[0,0],"end":[0,144],"size":"2x12","role":"beam"}));
    let joist = board(&mut s, json!({"start":[0,0],"end":[144,0],"onTopOf":beam.hex()}));
    assert_eq!(model::resolve(s.doc().unwrap(), joist).unwrap().bottom(), 11.25);
    s.execute("contractor.repeat", &json!({"handle":joist.hex(),"count":4,"spacing":16})).unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 5);
    let members = model::members(s.doc().unwrap());
    assert!(members.iter().any(|m| m.insert.insert.y == 48.0));
    s.execute("contractor.repeat", &json!({"handle":joist.hex(),"count":2,"spacing":16,"direction":[1e300,0]})).unwrap();
    assert!(model::members(s.doc().unwrap()).iter().any(|m| m.insert.insert.x == 16.0));
    s.execute("contractor.set", &json!({"handle":beam.hex(),"size":"2x8"})).unwrap();
    // Independent placement does not create a hidden support constraint.
    assert_eq!(model::resolve(s.doc().unwrap(), joist).unwrap().bottom(), 11.25);
    s.execute("u", &json!({})).unwrap();
    assert_eq!(model::resolve(s.doc().unwrap(), beam).unwrap().top(), 11.25);
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(model::resolve(s.doc().unwrap(), beam).unwrap().top(), 7.25);
    s.execute("u", &json!({})).unwrap();
    s.execute("copy", &json!({"handles":[joist.hex()],"delta":[200,0]})).unwrap();
    s.execute("contractor.set", &json!({"handle":joist.hex(),"length":120})).unwrap();
    assert!(
        model::members(s.doc().unwrap())
            .iter()
            .any(|m| m.insert.insert.x == 200.0 && matches!(m.record.shape,Shape::Board {length,..} if length==144.0))
    );
}

#[test]
fn conversion_is_explicit_and_atomic_and_preserves_hvac() {
    let mut s = session();
    s.execute("line", &json!({"points":[[0,0],[120,0]]})).unwrap();
    s.execute("circle", &json!({"center":[0,0],"radius":5})).unwrap();
    s.execute("line", &json!({"points":[[2,3],[8,7]]})).unwrap();
    let hs = s.doc().unwrap().model.iter().map(|e| e.handle).collect::<Vec<_>>();
    let hvac = s.doc().unwrap().entity(hs[2]).unwrap().clone();
    s.execute("contractor.convert", &json!({"handles":[hs[0].hex(),hs[1].hex()],"size":"2x12","bottom":-96,"top":0})).unwrap();
    assert_eq!(s.doc().unwrap().model.len(), 5);
    assert_eq!(s.doc().unwrap().entity(hs[2]), Some(&hvac));
    assert_eq!(model::members(s.doc().unwrap()).len(), 2);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("contractor.convert", &json!({"handles":[hs[0].hex(),"FFFFFFFF"],"replace":true})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
    s.execute("contractor.convert", &json!({"handle":hs[0].hex(),"replace":true,"alignment":"left"})).unwrap();
    assert!(s.doc().unwrap().entity(hs[0]).is_none());
    assert_eq!(s.doc().unwrap().entity(hs[2]), Some(&hvac));
    let last = model::members(s.doc().unwrap()).into_iter().last().unwrap();
    assert_eq!(last.insert.insert.y, -0.75);
    s.execute("contractor.convert", &json!({"handle":hs[2].hex(),"alignment":"right"})).unwrap();
    let m = model::members(s.doc().unwrap()).into_iter().last().unwrap();
    let expected = cadcraft_geom::Vec2::new(2.0, 3.0) + cadcraft_geom::Vec2::new(6.0, 4.0).normalized().perp() * 0.75;
    assert!(m.insert.insert.xy().dist(expected) < 1e-10);
}

#[test]
fn nominal_members_reject_generic_resize_and_block_edit() {
    let mut s = session();
    let h = board(&mut s, json!({"start":[0,0],"end":[120,0],"bottom":11.25}));
    let before = s.doc().unwrap().clone();
    assert!(s.execute("scale", &json!({"handle":h.hex(),"base":[0,0],"factor":2})).is_err());
    assert!(s.execute("properties.set", &json!({"handle":h.hex(),"scale":2})).is_err());
    assert_eq!(&before, s.doc().unwrap());
    assert!(s.execute("bedit", &json!({"handle":h.hex()})).is_err());
    assert!(s.execute("refedit", &json!({"handle":h.hex()})).is_err());
    s.execute("rotate", &json!({"handle":h.hex(),"base":[0,0],"angle":45})).unwrap();
    s.execute("mirror", &json!({"handle":h.hex(),"p1":[0,0],"p2":[1,0],"erase":true})).unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 1);
    s.execute("explode", &json!({"handle":h.hex()})).unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 0);
    assert!(s.doc().unwrap().model.iter().any(|e| matches!(e.kind, EntityKind::Face3d(_))));
    let elevations = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| if let EntityKind::Face3d(f) = &e.kind { Some(f.corners) } else { None })
        .flatten()
        .map(|p| p.z)
        .collect::<Vec<_>>();
    assert_eq!(elevations.iter().copied().reduce(f64::min), Some(11.25));
    assert_eq!(elevations.iter().copied().reduce(f64::max), Some(20.5));
    assert!(!s.doc().unwrap().model.iter().any(|e| model::tagged(e)));
    s.execute("u", &json!({})).unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 1);
}

#[test]
fn interactive_and_json_placement_agree() {
    let mut s = session();
    s.execute("contractor.tool", &json!({"size":"2x12","role":"beam","bottom":20})).unwrap();
    s.cmdline("contractor.board 0,0 120,0").unwrap();
    s.cmdline("contractor.repeat 3 16").unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 3);
    let m = model::members(s.doc().unwrap()).pop().unwrap();
    assert_eq!(m.bottom(), 20.0);
    assert_eq!(m.top(), 31.25);
    assert_eq!(m.record.profile, "2x12");
}

#[test]
fn interactive_placement_uses_plan_and_cancel_creates_no_members() {
    let mut s = session();
    board(&mut s, json!({"start":[0,0],"end":[120,0]}));
    for command in ["contractor.board", "contractor.post", "contractor.repeat"] {
        s.execute("contractor.view", &json!({"mode":"model3d"})).unwrap();
        s.start(command).unwrap();
        assert_eq!(model::representation(s.doc().unwrap()), model::Representation::Plan);
        s.cancel();
        assert_eq!(model::members(s.doc().unwrap()).len(), 1);
    }
}

#[test]
fn interactive_repeat_can_select_a_member_or_cancel() {
    let mut s = session();
    let h = board(&mut s, json!({"start":[0,0],"end":[120,0]}));
    s.set_selection(Vec::new());
    let before = s.doc().unwrap().clone();
    s.start("contractor.repeat").unwrap();
    s.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(200.0, 200.0))).unwrap();
    s.cancel();
    assert_eq!(s.doc().unwrap(), &before);
    s.start("contractor.repeat").unwrap();
    s.input(cadcraft_engine::Input::Pick(vec![h])).unwrap();
    s.input(cadcraft_engine::Input::Text("3".into())).unwrap();
    s.input(cadcraft_engine::Input::Text("16".into())).unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 3);
}

#[test]
fn metric_interactive_preview_uses_the_drawing_units() {
    let mut s = Session::new();
    s.new_drawing(true);
    s.start("contractor.board").unwrap();
    s.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::ZERO)).unwrap();
    assert!(!s.preview(cadcraft_geom::Vec2::new(1000.0, 0.0)).is_empty());
}

#[test]
fn hostile_batches_and_parameters_do_not_change_the_drawing() {
    let mut s = session();
    let h = board(&mut s, json!({"start":[0,0],"end":[120,0]}));
    for (cmd, p) in [
        ("contractor.repeat", json!({"handle":h.hex(),"count":18446744073709551615u64,"spacing":16})),
        ("contractor.repeat", json!({"handle":h.hex(),"count":2,"spacing":1e300})),
        ("contractor.repeat", json!({"handle":h.hex(),"count":2,"spacing":16,"direction":[0,0]})),
        ("contractor.board", json!({"start":[0,0],"end":[0,0]})),
        ("contractor.board", json!({"start":[0,0],"end":[1e300,0]})),
        ("contractor.set", json!({"handle":h.hex(),"width":-5})),
        ("contractor.set", json!({"handle":h.hex(),"label":"x".repeat(129)})),
    ] {
        let before = s.doc().unwrap().clone();
        assert!(s.execute(cmd, &p).is_err(), "{cmd}");
        assert_eq!(s.doc().unwrap(), &before, "{cmd}");
    }
}

#[test]
fn blueprint_exports_keep_the_source_and_reopen_as_members() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, p| cadcraft_io::plotter::plot(d, space, p).map_err(|e| e.to_string())),
    });
    let mut s = session();
    s.execute("contractor.sample", &json!({})).unwrap();
    let before = s.doc().unwrap().clone();
    let folder = std::env::var_os("CADCRAFT_CONTRACTOR_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("cadcraft-contractor-export-{}", std::process::id())));
    std::fs::create_dir_all(&folder).unwrap();
    for ext in ["pdf", "dxf", "dwg"] {
        for representation in ["plan", "engineering"] {
            let path = folder.join(format!("{representation}.{ext}"));
            s.execute("contractor.export", &json!({"path":path.to_string_lossy(),"representation":representation,"paper":"ANSI D","scale":1.0/48.0}))
                .unwrap();
            assert_eq!(s.doc().unwrap(), &before);
            let bytes = std::fs::read(&path).unwrap();
            if ext == "pdf" {
                assert!(bytes.starts_with(b"%PDF"));
                assert!(bytes.len() > 1000);
            } else {
                let back = cadcraft_io::read(&bytes, path.to_str().unwrap()).unwrap();
                assert_eq!(model::members(&back).len(), 31);
                assert!(back.model.iter().any(|e| e.common.layer == model::NOTES));
            }
        }
    }
}

#[test]
fn blueprint_annotations_do_not_overlap_or_accumulate_on_reexport() {
    let mut s = session();
    s.execute("contractor.sample", &json!({})).unwrap();
    let drawing = model::export_drawing(s.doc().unwrap(), model::Representation::Engineering, 2.5 / 25.4 * 48.0).unwrap();
    let boxes = drawing
        .model
        .iter()
        .filter(|e| e.common.layer == model::NOTES && matches!(e.kind, EntityKind::Text(_)))
        .map(|e| cadcraft_doc::entity_bounds(&drawing, e, 0))
        .collect::<Vec<_>>();
    assert_eq!(boxes.len(), 64);
    for (i, a) in boxes.iter().enumerate() {
        for b in boxes.iter().skip(i + 1) {
            assert!(!a.intersects(b), "annotation overlap: {a:?} {b:?}");
        }
    }
    let count = drawing.model.len();
    let reopened = cadcraft_io::read(&cadcraft_io::write(&drawing, "annotations.dwg").unwrap(), "annotations.dwg").unwrap();
    let again = model::export_drawing(&reopened, model::Representation::Engineering, 2.5 / 25.4 * 48.0).unwrap();
    assert_eq!(again.model.len(), count, "generated notes are replaced, not duplicated");
}
