#![cfg(feature = "contractor")]
use cadcraft_ui_egui::control::{ControlRequest, Outcome, handle};
use cadcraft_ui_egui::{CadApp, Services};
use serde_json::json;

#[test]
fn framing_workspace_and_headless_views_render_without_a_window() {
    let mut s = cadcraft_engine::Session::new();
    s.new_drawing(false);
    s.execute("contractor.sample", &json!({})).unwrap();
    let mut app = CadApp::new(s, Services::default());
    app.ui.start_tab = false;
    app.ui.toolset_tab = "Contractor".into();
    let context = egui::Context::default();
    let folder = std::env::var_os("CADCRAFT_CONTRACTOR_EVIDENCE").map(std::path::PathBuf::from);
    if let Some(folder) = &folder {
        std::fs::create_dir_all(folder).unwrap();
    }
    for mode in ["plan", "engineering", "model3d"] {
        app.run("contractor.view", json!({"mode":mode})).unwrap();
        let mut request = json!({"width":1000,"height":800,"fit":true});
        if let Some(folder) = &folder {
            request["path"] = json!(folder.join(format!("{mode}.png")).to_string_lossy());
        }
        let (req, _) = ControlRequest::new("ui.render", request);
        let Outcome::Done(reply) = handle(&mut app, &context, &req) else { panic!("expected rendered image") };
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(reply["result"]["bytes"].as_u64().unwrap() > 1000);
        let count = app.session.doc().unwrap().model.len();
        let mut frame = context.run_ui(egui::RawInput::default(), |ui| cadcraft_ui_egui::contractor::panel(&mut app, ui));
        frame.textures_delta.clear();
        assert_eq!(app.session.doc().unwrap().model.len(), count, "presentation must not mutate the drawing");
    }
    if let Some(folder) = folder {
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).wgpu().build_ui_state(
            |ui, app: &mut CadApp| {
                app.logic(&ui.ctx().clone());
                app.ui(ui);
            },
            app,
        );
        for mode in ["plan", "engineering", "model3d"] {
            harness.state_mut().run("contractor.view", json!({"mode":mode})).unwrap();
            if mode != "engineering" {
                harness.state_mut().session.zoom_extents().unwrap();
            }
            harness.run_steps(3);
            harness.render().unwrap().save(folder.join(format!("workspace-{mode}.png"))).unwrap();
        }
    }
}

#[test]
fn contractor_shader_is_valid_wgsl() {
    let module = naga::front::wgsl::parse_str(cadcraft_ui_egui::contractor::gpu::SHADER).unwrap();
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&module).unwrap();
}
