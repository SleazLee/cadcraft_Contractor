# Contractor mode — floor framing

**Status:** v1 implemented and validated. **Updated:** 2026-10-17.

Contractor mode adds individual wood members to the existing drawing. Choose the **Contractor** Tool Sets tab. **Plan** shows physical footprints; **3D** shows actual member volumes; **Engineering** shows centroid axes and current elevation labels. Existing drafting, dimensions, layouts, and plot tools remain available. This is drawing geometry, not load analysis or code compliance checking.

## Working with a floor

For a quick demonstration, create an imperial drawing, choose **Tools → Contractor → Sample Floor Framing**, then choose the **Contractor** Tool Sets tab and **3D**. The sample is an illustrative 24-foot floor; it does not reconstruct measurements from the project screenshot.

Choose 2×2, 2×4, 2×6, 2×8, 2×10, 2×12, or a custom section. Stock names use dry dressed US sizes: 1½ inches thick, with depths 1½, 3½, 5½, 7¼, 9¼ and 11¼ inches. Numeric source: [PS 20-25, Table 3](https://alsc.org/uploaded/PS%2020-25%20Final.pdf). No source artwork or text is embedded. Select **Lay flat** to swap the section width and depth. Values elsewhere are in the drawing's INSUNITS; inches, feet, millimetres, centimetres and metres are supported. Unitless drawings must be assigned units first.

Set the bottom elevation, then **Place board** and pick two plan points. Or select a support and **Use selected member’s top**, which samples its current top elevation. There is no ongoing support link. A 2×12 beam at bottom 0 has top 11¼ inches; an on-edge 2×10 placed on it has bottom 11¼ and top 20½, with an axis at 15⅞. These are geometric examples, not span recommendations.

Posts have rectangular sections; pilings have entered diameters. Both have a bottom and height. **Repeat** uses entered on-center spacing and count (including the original), producing independent members. Boards can be placed at any plan angle. Slopes, cuts, assemblies, fasteners, and structural connections are outside v1.

To use an existing drawing, explicitly select LINEs and/or CIRCLEs and choose **Convert**. Lines become boards using center/left/right alignment relative to the line's start-to-end direction; circles become pilings with their original diameter. Original objects remain unless **Replace** is enabled. A failed batch changes nothing. Unselected objects, including HVAC notes, are never classified, converted, or moved.

Orbit in 3D by dragging; Shift/right drag pans; the wheel zooms; double-click or **Fit model** fits the members. Click a member and edit its properties in the Contractor palette. Interactive placement enters Plan. Repeat can use a preselected member or prompt for one. **2D reference** displays ordinary geometry as a flat contextual reference, without inventing elevations.

Move/copy/rotate/mirror/erase and undo/redo work on the normal INSERT. Resize through Contractor properties. SCALE and generated-block editing are protected; EXPLODE explicitly converts a member into ordinary CAD geometry. Externally modified blocks and unknown metadata versions remain ordinary CAD objects, with diagnostic reasons available from `contractor.list`.

## Commands and file exchange

All commands have JSON forms, listed with `cadcraft-cli commands contractor`. JSON execution never opens a dialog. The same catalog is exposed over control and MCP.

```json
{"command":"contractor.board","params":{"start":[0,0],"end":[0,288],"size":"2x12","role":"beam","bottom":0}}
{"command":"contractor.board","params":{"start":[0,0],"end":[288,0],"size":"2x10","onTopOf":"MEMBER_HANDLE"}}
{"command":"contractor.repeat","params":{"handle":"MEMBER_HANDLE","spacing":16,"count":19}}
{"command":"contractor.view","params":{"mode":"model3d"}}
```

Handles in examples are placeholders; use the returned hexadecimal handle. `contractor.tool` sets per-document interactive placement defaults, then `contractor.board 0,0 120,0` uses the usual command-line point grammar. `contractor.post`, `contractor.convert`, `contractor.set`, `contractor.list`, and `contractor.sample` cover the other workflows.

`contractor.export` takes `path`, `representation` (`plan`, `engineering`, or `model3d`), and for PDF `paper`, `landscape`, `scale`, and `fit`. Scale is a physical paper/model ratio: 1/48 means ¼ inch per foot regardless of model units. The default is ANSI D landscape, 1:48, **fit off**. Choose a paper and scale that contain the drawing; explicit scale may clip a drawing larger than the page. Existing dimension entities are retained; add project dimensions using the normal dimension tools. Exported elevation labels use a 2.5 mm paper text height at the selected scale.

PDF supports Plan and Engineering. Short member tags link to a key with sizes and bottom/top elevations; native background masks keep lines clear of the text. Tags and keys are derived on a clone, including in the Engineering canvas. DXF/DWG support all three representations and retain metadata and the hidden representations for subsequent Contractor editing. The existing native DWG bridge writes AC1032 (2018) by default. DWG remains native-only, as in upstream; browser builds support Contractor modeling, viewing, PDF and DXF. Export clones the source drawing and does not change it or mark it saved. Standard Save/Save As also retain members.

`ui.render` honors the current Contractor 3D representation using the bounded CPU renderer, even without a presented frame. An optional `camera` object has `center:{x,y,z}`, `yaw`, `pitch` (radians), and `height`; `fit:false` preserves it. Large images or excessive raster work return an error. Native screenshots use the GPU viewport when available.

## Architecture and upstream maintenance

The optional `contractor` Cargo feature is default-on for this fork's apps and available on engine, UI and MCP crates. Each app and crate also builds with `--no-default-features`. No Contractor dependency is introduced into doc, render or IO.

The L2 `cadcraft-contractor` crate owns the catalog, bounded schema, native geometry, decoding, centroid axes, camera, picking and software depth renderer. Engine adapters own commands and transactions. The egui adapter owns temporary controls and an independent offscreen GPU renderer with a Depth32Float attachment. Neither renderer owns a second editable model.

One INSERT carries `CADCRAFT_CONTRACTOR` xdata with version 1, material, nominal profile, role, label, and shape dimensions. Placement comes from the INSERT transform. The payload is limited to 8 KiB and split at UTF-8 boundaries into 255-byte DXF strings. Metadata lives on INSERTs because the DWG bridge preserves entity xdata but has gaps for block/table metadata. Recognized definitions must match the generated geometry. Editing creates a private replacement block, preserving the source handle and other references' definitions.

Reserved representation layers are `CC_CONTRACTOR_PLAN`, `CC_CONTRACTOR_AXIS`, and `CC_CONTRACTOR_MESH`. Generated export annotations use `CC_CONTRACTOR_NOTES`. Layer flags save the active representation. All three representations come from one parameter set. Standard geometry gives other CAD software useful blocks and 3D faces without this extension; these are faces, not ACIS solids.

Shared integration points, intentionally kept small:

| Area | Integration |
|---|---|
| Workspace manifests | Optional dependency and feature forwarding; layer-table entry |
| Engine registry / DocState | Command registration and per-document tool preferences |
| Modify / Properties / block editing | Reject unsupported resizing; protect generated definitions; explode recognized members with full world coordinates |
| File adapter | Reuse atomic native storage / browser delivery and installed IO hooks |
| CadApp / Tool Sets | Contractor state, renderer setup, third tab |
| Display-time translator | Merge the Contractor reference-locale catalog; upstream entries win |
| Canvas / rebuild / control | 3D dispatch and presentation-only engineering annotations |

After an upstream merge, inspect these points first. Run the Contractor storage/command/UI tests, feature-disabled app checks, and `cargo xtask ci`. Visually inspect the three views and repeat a DWG save/reopen/edit cycle. Do not alter native parity assessments to imply a general-purpose 3D CAD kernel. New materials and section types should extend the isolated model and metadata version deliberately; do not add speculative fields or a second database.

Agent notes live beside the model crate and both adapter modules. Reusable learning for shared crafting standards: put extension metadata on existing graphical entities where possible, verify the actual interchange bridge before designing storage, and keep native fallback geometry when an extension is unavailable. The referenced shared `craftrules` checkout and local `plan/` files were unavailable in this checkout; no shared repository was modified.

## Validation record

- Native storage tests exercise all three shapes and representations through two DXF/DWG cycles, then edit reopened members. Generated-name collision tests protect unrelated block definitions during creation and editing.
- Command tests cover stack elevations, independent placement, on-center repetition, copy-on-edit, conversion preservation/atomicity, transforms, interactive input, hostile numbers and undo/redo.
- Rendering tests check depth-order-independent CPU picking and GPU drawing, positive elevation appearing upward, bounded software rendering and shader validation; UI evidence is generated under ignored `temp/` when requested.
- Visually inspected full-window Plan, Engineering and 3D views, plus exported Plan/Engineering PDFs. ANSI D landscape is 2448 × 1584 PDF points; 24-foot members measure 432 points (6 inches) at 1:48 in both exports.
- `cargo xtask ci` passed all six gates: formatting, warning-free native Clippy, 2,033 tests, assets, layers and all 12 WASM crate checks. Validation used Rust 1.99.1 GNU on Windows; test debug symbols were omitted to bound disk use. Existing WASM worker-helper dead-code warnings remain.
- Native desktop, CLI, web, engine, UI and MCP feature-disabled checks passed. The browser application also checked successfully for WASM with Contractor both enabled and disabled.
- Optimized Windows desktop and CLI builds succeeded. The release CLI exposed all 13 Contractor commands, exported a scaled engineering PDF, reopened all 31 DWG sample members without metadata issues and verified joist bottoms equal beam tops. The native desktop started with an isolated configuration and rendered the synthetic assembly through its control channel; the resulting 900 × 700 image was visually inspected.

## Revision history

| Date | Change |
|---|---|
| 2026-10-17 | Initial Contractor floor-framing implementation, storage schema, agent boundaries and maintenance checklist |
