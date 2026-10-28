//! Synthetic overlapping CAD line bounds: no project geometry or file fixture is included.
use rstar::RTree;
use rstar::primitives::{GeomWithData, Rectangle};

use cadcraft_doc::{Common, EntityKind, Line, Space};
use cadcraft_engine::{Session, spatial};
use cadcraft_geom::{Bounds2, Vec2, Vec3};
use serde_json::json;

fn check_cached_index(s: &Session) {
    let d = s.doc().unwrap();
    let cached = spatial::index(d, &Space::Model).unwrap();
    let fresh = spatial::SpatialIndex::build(d, &d.model);
    let area = Bounds2::new(Vec2::new(-100.0, -100.0), Vec2::new(200.0, 100.0));
    let hits = cached.query(&area, true).unwrap();
    assert_eq!(hits, fresh.query(&area, true).unwrap());
    assert_eq!(cached.len(), d.model.len());
    for i in hits {
        assert_eq!(cached.entity(i).unwrap().handle, fresh.entity(i).unwrap().handle);
    }
}

#[test]
fn drawing_erase_and_undo_keep_snapping_index_consistent() {
    let mut s = Session::new();
    s.new_drawing(false);
    let mut original = Vec::new();
    for i in 0..1607 {
        let x = f64::from(i % 100);
        original.push(
            s.doc_mut()
                .unwrap()
                .add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(x, 0.0, 0.0), b: Vec3::new(x + 1.0, 0.0, 0.0) }))
                .unwrap(),
        );
    }
    check_cached_index(&s);
    for h in original.iter().take(6) {
        s.execute("line", &json!({"points":[[-50,-50],[20,0]]})).unwrap();
        check_cached_index(&s);
        s.execute("erase", &json!({"handle":h.hex()})).unwrap();
        check_cached_index(&s);
        s.execute("u", &json!({})).unwrap();
        check_cached_index(&s);
        assert!(s.doc().unwrap().entity(*h).is_some());
        s.execute("redo", &json!({})).unwrap();
        check_cached_index(&s);
        assert!(s.doc().unwrap().entity(*h).is_none());
    }
}
#[test]
fn overlapping_line_bounds_survive_chunk_replacement_and_insert() {
    const COUNT: usize = 1607;
    const CHUNK: usize = 1024;
    let mut items: Vec<_> = (0..COUNT)
        .map(|i| {
            let x = (i % 100) as f64;
            GeomWithData::new(Rectangle::from_corners([x, 0.0], [x + 1.0, 0.0]), i)
        })
        .collect();
    let mut tree = RTree::bulk_load(items.clone());
    let mut additions = Vec::new();
    // An edit replaces one entity-store chunk; adding a line also changes its size. Repeated
    // collinear bounds expose a mixed-height tree in rstar 0.13's bulk-load/edit path, which
    // panics in recursive_insert outside the command guard while the UI is snapping.
    for cycle in 0..6 {
        let start = if cycle % 2 == 0 { (COUNT - 1) / CHUNK * CHUNK } else { 0 };
        for item in items.iter().skip(start).take(CHUNK) {
            assert!(tree.remove(item).is_some());
        }
        for (i, item) in items.iter_mut().enumerate().skip(start).take(CHUNK) {
            let x = (i % 100) as f64;
            *item = GeomWithData::new(Rectangle::from_corners([x, 0.0], [x + 1.0, 0.0]), i + (cycle + 1) * COUNT);
            tree.insert(*item);
        }
        let id = COUNT * (cycle + 10);
        tree.insert(GeomWithData::new(Rectangle::from_corners([-50.0, -50.0], [20.0, 0.0]), id));
        additions.push(id);
        let mut expected: Vec<_> = items.iter().map(|item| item.data).chain(additions.iter().copied()).collect();
        let mut actual: Vec<_> = tree.iter().map(|item| item.data).collect();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected, "replacement cycle {cycle}");
        assert_eq!(tree.size(), COUNT + cycle + 1);
    }
}
