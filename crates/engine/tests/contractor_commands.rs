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
fn blockreplace_preserves_generated_member_geometry_and_metadata() {
    let mut s = session();
    let source = board(&mut s, json!({"start":[0,0],"end":[120,0],"size":"2x10"}));
    let target = board(&mut s, json!({"start":[0,24],"end":[120,24],"size":"2x12"}));
    let block_name = |h| match &s.doc().unwrap().entity(h).unwrap().kind {
        EntityKind::Insert(insert) => insert.block.clone(),
        _ => panic!("member must be a block reference"),
    };
    let (old, new) = (block_name(source), block_name(target));
    let before = s.doc().unwrap().clone();
    let error = s.execute("blockreplace", &json!({"block":old,"with":new,"purge":true})).unwrap_err();
    assert!(error.to_string().contains("Contractor properties"));
    assert_eq!(&before, s.doc().unwrap());
    s.start("blockreplace").unwrap();
    s.log.clear();
    s.cmdline(&old).unwrap();
    assert!(s.log.iter().any(|line| line.contains("Contractor properties")));
    assert!(s.prompt_text().contains("block to replace"));
    assert_eq!(&before, s.doc().unwrap());
    s.input(cadcraft_engine::Input::Cancel).unwrap();
    assert_eq!(model::members(s.doc().unwrap()).len(), 2);
    assert!(model::resolve(s.doc().unwrap(), source).is_ok());
    assert!(model::resolve(s.doc().unwrap(), target).is_ok());
}

#[test]
fn selected_pilings_have_independent_depths_and_atomic_batch_properties() {
    let mut s = session();
    let mut hs = Vec::new();
    for i in 0..12 {
        let v = s
            .execute(
                "contractor.post",
                &json!({"at":[i*16,0],"shape":"pile","top":"0\"","depthBelowTop":if i<4 {"10'"} else {"8'"},"diameter":"9.25\""}),
            )
            .unwrap();
        hs.push(v["handle"].as_str().unwrap().to_string());
    }
    s.execute("contractor.set", &json!({"handles":hs.iter().take(4).collect::<Vec<_>>(),"depthBelowTop":"12'","label":"Deeper supports"})).unwrap();
    for (i, h) in hs.iter().enumerate() {
        let m = model::resolve(s.doc().unwrap(), Handle::parse_hex(h).unwrap()).unwrap();
        assert_eq!(m.top(), 0.0);
        assert_eq!(m.bottom(), if i < 4 { -144.0 } else { -96.0 });
    }
    s.execute("u", &json!({})).unwrap();
    assert_eq!(model::members(s.doc().unwrap())[0].bottom(), -120.0);
    s.execute("redo", &json!({})).unwrap();
    let before = format!("{:?}", s.doc().unwrap());
    assert!(s.execute("contractor.set", &json!({"handles":[hs[0],"FFFFFFFF"],"diameter":"12\""})).is_err());
    assert_eq!(before, format!("{:?}", s.doc().unwrap()));
    s.execute("contractor.set", &json!({"handle":hs[0],"shape":"post","width":"6\"","depth":"8\""})).unwrap();
    assert!(matches!(
        model::resolve(s.doc().unwrap(), Handle::parse_hex(&hs[0]).unwrap()).unwrap().record.shape,
        Shape::Post { width: 6.0, depth: 8.0, .. }
    ));
}

#[test]
fn explicit_length_units_convert_to_the_drawing_unit() {
    let mut s = session();
    s.execute("setvar", &json!({"name":"INSUNITS","value":4})).unwrap();
    let h = board(&mut s, json!({"start":[0,0],"end":[3000,0],"size":"custom","width":"1 1/2\"","depth":"9.25\"","bottom":"1.2'"}));
    let m = model::resolve(s.doc().unwrap(), h).unwrap();
    assert!((m.bottom() - 365.76).abs() < 1e-8);
    assert!(matches!(m.record.shape,Shape::Board{width,depth,..} if (width-38.1).abs()<1e-8 && (depth-234.95).abs()<1e-8));
    s.execute("contractor.set", &json!({"handle":h.hex(),"bottom":"25 cm","length":"4 m"})).unwrap();
    let m = model::resolve(s.doc().unwrap(), h).unwrap();
    assert_eq!(m.bottom(), 250.0);
    assert_eq!(m.record.cut_length(), Some(4000.0));
    for bad in ["NaN", "inf", "1e999'", "-2/0\"", "junk"] {
        assert!(s.execute("contractor.set", &json!({"handle":h.hex(),"bottom":bad})).is_err());
    }
}

#[test]
fn choose_base_and_butt_joints_use_the_same_interactive_and_json_commands() {
    use cadcraft_engine::Input;
    let mut s = session();
    let rim = board(&mut s, json!({"start":[0,0],"end":[0,144]}));
    let joist = board(&mut s, json!({"start":[0,48],"end":[120,48]}));
    assert_eq!(s.execute("contractor.collisions", &json!({})).unwrap()["overlaps"].as_array().unwrap().len(), 1);
    s.start("contractor.join").unwrap();
    s.input(Input::Pick(vec![rim])).unwrap();
    s.cmdline("Start").unwrap();
    let m = model::resolve(s.doc().unwrap(), joist).unwrap();
    assert!((m.record.setbacks.start - 0.75).abs() < 1e-8);
    assert!(m.axis().0.x.abs() < 1e-8);
    assert!(s.execute("contractor.collisions", &json!({})).unwrap()["overlaps"].as_array().unwrap().is_empty());
    s.execute("copy", &json!({"handles":[joist.hex()],"delta":[0,16]})).unwrap();
    s.execute("contractor.repeat", &json!({"handle":joist.hex(),"spacing":"16\"","count":2})).unwrap();
    assert!(model::members(s.doc().unwrap()).iter().filter(|m| m.record.version == 2).all(|m| (m.record.setbacks.start - 0.75).abs() < 1e-8));
    s.execute("select", &json!({"handles":[joist.hex()]})).unwrap();
    s.start("contractor.base").unwrap();
    s.input(Input::Pick(vec![rim])).unwrap();
    s.cmdline("Below").unwrap();
    let m = model::resolve(s.doc().unwrap(), joist).unwrap();
    assert_eq!(m.top(), 0.0);
    assert_eq!(m.bottom(), -9.25);
    s.execute("contractor.base", &json!({"handle":joist.hex(),"reference":rim.hex(),"side":"above"})).unwrap();
    assert_eq!(model::resolve(s.doc().unwrap(), joist).unwrap().bottom(), 9.25);
    s.execute("select", &json!({"handles":[]})).unwrap();
    s.execute("contractor.tool", &json!({"kind":"pile","shape":"pile","depthBelowTop":"8'","top":0})).unwrap();
    s.start("contractor.base").unwrap();
    s.input(Input::Pick(vec![rim])).unwrap();
    s.cmdline("Below").unwrap();
    s.cmdline("contractor.post 144,144").unwrap();
    let piles = model::members(s.doc().unwrap()).into_iter().filter(|m| matches!(m.record.shape, Shape::Pile { .. })).collect::<Vec<_>>();
    assert_eq!(piles[0].top(), 0.0);
    assert_eq!(piles[0].bottom(), -96.0);
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
