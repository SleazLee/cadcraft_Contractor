#![cfg(feature = "contractor")]
use cadcraft_contractor::*;
use cadcraft_doc::{Common, Drawing, EntityKind};
use cadcraft_geom::Vec3;

fn board(d: &mut Drawing) -> cadcraft_doc::Handle {
    add(
        d,
        MemberRecord::wood("2x10", "joist", Shape::Board { length: 120.0, width: 1.5, depth: 9.25 }),
        Vec3::new(4.0, 3.0, 11.25),
        0.3,
        Common::default(),
    )
    .unwrap()
}

#[test]
fn catalog_units_and_stacking() {
    for (name, depth) in [("2x2", 1.5), ("2x4", 3.5), ("2x6", 5.5), ("2x8", 7.25), ("2x10", 9.25), ("2x12", 11.25)] {
        assert_eq!(lumber(name, 25.4, false).unwrap(), (1.5, depth));
        assert_eq!(lumber(name, 25.4, true).unwrap(), (depth, 1.5));
    }
    assert_eq!(lumber("2x12", 25.4, false).unwrap(), (1.5, 11.25));
    assert_eq!(lumber("2×10", 25.4, true).unwrap(), (9.25, 1.5));
    let (w, d) = lumber("2x10", 1.0, false).unwrap();
    assert!((w - 38.1).abs() < 1e-10 && (d - 234.95).abs() < 1e-10);
    let mut drawing = Drawing::default();
    let beam = add(
        &mut drawing,
        MemberRecord::wood("2x12", "beam", Shape::Board { length: 144.0, width: 1.5, depth: 11.25 }),
        Vec3::ZERO,
        0.0,
        Common::default(),
    )
    .unwrap();
    let joist = board(&mut drawing);
    let (b, j) = (resolve(&drawing, beam).unwrap(), resolve(&drawing, joist).unwrap());
    assert_eq!(b.top(), j.bottom());
    assert_eq!(b.axis().0.z, 5.625);
    assert_eq!(j.axis().0.z, 15.875);
}

#[test]
fn dxf_and_dwg_repeated_roundtrip_keeps_editable_members() {
    for extension in ["dxf", "dwg"] {
        if cfg!(target_arch = "wasm32") && extension == "dwg" {
            continue;
        }
        let mut d = Drawing::default();
        let h = board(&mut d);
        add(
            &mut d,
            MemberRecord::wood("custom", "piling", Shape::Pile { diameter: 10.0, height: 96.0 }),
            Vec3::new(4.0, 3.0, -96.0),
            0.0,
            Common::default(),
        )
        .unwrap();
        add(
            &mut d,
            MemberRecord::wood("custom", "post", Shape::Post { width: 5.5, depth: 5.5, height: 80.0 }),
            Vec3::new(50.0, 0.0, -80.0),
            0.0,
            Common::default(),
        )
        .unwrap();
        for rep in [Representation::Plan, Representation::Engineering, Representation::Model3d] {
            set_representation(&mut d, rep);
            for _ in 0..2 {
                let name = format!("framing.{extension}");
                let bytes = cadcraft_io::write(&d, &name).unwrap();
                if extension == "dwg" {
                    assert_eq!(bytes.get(..6), Some(b"AC1032".as_slice()));
                }
                d = cadcraft_io::read(&bytes, &name).unwrap();
                assert_eq!(members(&d).len(), 3, "{extension}: {:?}", d.read_notes);
                let m = resolve(&d, h).unwrap();
                assert_eq!(m.record.profile, "2x10");
                assert!((m.top() - 20.5).abs() < 1e-8);
            }
        }
        let old = resolve(&d, h).unwrap();
        update(&mut d, h, old.record, Vec3::new(1.0, 2.0, 22.0), 0.0).unwrap();
        assert_eq!(resolve(&d, h).unwrap().bottom(), 22.0);
    }
}

#[test]
fn generated_names_do_not_replace_existing_blocks() {
    let mut d = Drawing::default();
    let reserved = format!("CC_MEMBER_{}", cadcraft_doc::Handle(d.handseed).hex());
    let original = cadcraft_doc::Block::new(&reserved);
    d.blocks.insert(reserved.clone(), std::sync::Arc::new(original.clone()));
    let h = board(&mut d);
    assert_eq!(d.block(&reserved).unwrap().as_ref(), &original);
    let m = resolve(&d, h).unwrap();
    assert_ne!(m.insert.block, reserved);
    let replacement = format!("CC_MEMBER_{}_{}", h.hex(), cadcraft_doc::Handle(d.handseed).hex());
    let original = cadcraft_doc::Block::new(&replacement);
    d.blocks.insert(replacement.clone(), std::sync::Arc::new(original.clone()));
    update(&mut d, h, m.record, m.insert.insert, m.insert.rotation).unwrap();
    assert_eq!(d.block(&replacement).unwrap().as_ref(), &original);
    assert_ne!(resolve(&d, h).unwrap().insert.block, replacement);
}

#[test]
fn externally_modified_or_future_blocks_are_preserved() {
    let mut d = Drawing::default();
    let h = board(&mut d);
    let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { panic!() };
    let name = i.block.clone();
    let b = std::sync::Arc::make_mut(d.blocks.get_mut(&name).unwrap());
    let first = b.entities.iter().next().unwrap().handle;
    b.entities.modify(first, |e| e.kind = EntityKind::Point(cadcraft_doc::Point { p: Vec3::ZERO, angle: 0.0 }));
    assert!(resolve(&d, h).is_err());
    assert!(members(&d).is_empty());
    assert!(cadcraft_io::write(&d, "modified.dxf").is_ok());
    let mut d = Drawing::default();
    let h = board(&mut d);
    d.modify_entity(h, |e| {
        for t in &mut e.common.xdata {
            t.value = t.value.replace("\"version\":1", "\"version\":99");
        }
    })
    .unwrap();
    assert!(tagged(d.entity(h).unwrap()));
    assert!(resolve(&d, h).is_err());
    d.modify_entity(h, |e| {
        for t in &mut e.common.xdata {
            if t.code == 1000 {
                t.value = "{malformed".into();
            }
        }
    })
    .unwrap();
    assert!(resolve(&d, h).is_err());
    assert!(cadcraft_io::write(&d, "malformed.dxf").is_ok());
}

#[test]
fn hostile_geometry_and_camera_do_not_panic() {
    let mut exhausted = Drawing { handseed: u64::MAX, ..Default::default() };
    assert!(
        add(
            &mut exhausted,
            MemberRecord::wood("2x10", "joist", Shape::Board { length: 120.0, width: 1.5, depth: 9.25 }),
            Vec3::ZERO,
            0.0,
            Common::default()
        )
        .is_err()
    );
    assert!(exhausted.model.is_empty() && exhausted.blocks.is_empty());
    for n in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1e300] {
        let mut d = Drawing::default();
        assert!(
            add(
                &mut d,
                MemberRecord::wood("custom", "board", Shape::Board { length: n, width: 1.5, depth: 3.5 }),
                Vec3::ZERO,
                0.0,
                Common::default()
            )
            .is_err()
        );
        assert_eq!(d.model.len(), 0);
    }
    let mut d = Drawing::default();
    board(&mut d);
    let scene = view3d::Scene::build(&d).unwrap();
    let mut camera = view3d::Camera::default();
    camera.fit(&scene, 1.0);
    let frame = view3d::raster(&scene, &camera, 128, 128, &[]).unwrap();
    assert!(frame.handles.iter().any(|h| h.0 != 0));
    assert!(view3d::raster(&scene, &camera, usize::MAX, 2, &[]).is_err());
    camera.height = f64::NAN;
    assert!(view3d::raster(&scene, &camera, 128, 128, &[]).is_err());
}

#[test]
fn elevated_points_project_above_lower_points() {
    let camera = view3d::Camera::default();
    let bottom = camera.project(Vec3::ZERO, 800, 600);
    let top = camera.project(Vec3::new(0.0, 0.0, 96.0), 800, 600);
    assert!(top.y < bottom.y, "positive elevation must appear upward");
    assert!(top.z > bottom.z, "higher points are closer when looking down");
}

#[test]
fn depth_picking_is_order_independent() {
    let mut d = Drawing::default();
    let lower = add(
        &mut d,
        MemberRecord::wood("custom", "beam", Shape::Board { length: 100.0, width: 20.0, depth: 11.25 }),
        Vec3::ZERO,
        0.0,
        Common::default(),
    )
    .unwrap();
    let upper = add(
        &mut d,
        MemberRecord::wood("custom", "joist", Shape::Board { length: 100.0, width: 20.0, depth: 9.25 }),
        Vec3::new(0.0, 0.0, 11.25),
        0.0,
        Common::default(),
    )
    .unwrap();
    let mut scene = view3d::Scene::build(&d).unwrap();
    let camera = view3d::Camera { center: Vec3::new(50.0, 0.0, 0.0), yaw: 0.0, pitch: std::f64::consts::FRAC_PI_2, height: 120.0 };
    assert_eq!(scene.pick(&camera, 128, 128, 64.0, 64.0), Some(upper));
    scene.triangles.reverse();
    assert_eq!(scene.pick(&camera, 128, 128, 64.0, 64.0), Some(upper));
    assert_ne!(lower, upper);
}
