# Contractor mode — floor framing

**Status:** v1.1 implemented and validated; upstream 0.11.0 integration tracked in [upstream-updates.md](upstream-updates.md). **Updated:** 2026-10-22.

Contractor mode adds individual wood members to the existing drawing. Choose the **Contractor** Tool Sets tab. **Plan** shows physical footprints; **3D** shows actual member volumes; **Engineering** shows centroid axes and current elevation labels. Existing drafting, dimensions, layouts, and plot tools remain available. This is drawing geometry, not load analysis or code compliance checking.

## Working with a floor

For a quick demonstration, create an imperial drawing, choose **Tools → Contractor → Sample Floor Framing**, then choose the **Contractor** Tool Sets tab and **3D**. The sample is an illustrative 24-foot floor; it does not reconstruct measurements from the project screenshot.

Choose 2×2, 2×4, 2×6, 2×8, 2×10, 2×12, or a custom section. Stock names use dry dressed US sizes: 1½ inches thick, with depths 1½, 3½, 5½, 7¼, 9¼ and 11¼ inches. Numeric source: [PS 20-25, Table 3](https://alsc.org/uploaded/PS%2020-25%20Final.pdf). No source artwork or text is embedded. Select **Lay flat** to swap the section width and depth. Values elsewhere are in the drawing's INSUNITS; inches, feet, millimetres, centimetres and metres are supported. Unitless drawings must be assigned units first.

Choose **New lumber**, **New post**, or **New piling** in Tool Sets. Configure that type in the regular **Properties** panel, then **Place item**. The three types retain independent placement defaults. Lumber has size, orientation and bottom elevation; posts have section, height and bottom elevation; pilings have diameter, top elevation and depth below the top. Select an existing member to edit its own properties, with standard drawing properties such as layer and color available below. Select several members to edit them together: differing values show **Varies**, and only fields you actually change are applied. For example, select four of twelve pilings and change their depth without changing the other eight.

Length inputs accept `1"`, `9.25"`, `1.2'`, `1'-6 1/2"`, `250 mm`, `25 cm`, or `1 m`. Explicit units convert to the drawing's units; bare numbers use drawing units. **Choose base…** prompts for a placed reference and **Above** or **Below**, in Plan or 3D. It moves selected members without changing their height, or sets the current new-item elevation. It samples the reference at that moment; there is no ongoing support link. A 2×12 beam at bottom 0 has top 11¼ inches; an on-edge 2×10 placed on it has bottom 11¼ and top 20½, with an axis at 15⅞. These are geometric examples, not span recommendations.

Posts have rectangular sections; pilings have entered diameters. Both have a bottom and height. Support Properties can change a post into a piling or a piling into a post. **Repeat** uses entered on-center spacing and count (including the original), producing independent members. Boards can be placed at any plan angle. Slopes, miters, notches, assemblies, fasteners, and structural analysis remain outside this extension.

To use an existing drawing, explicitly select LINEs and/or CIRCLEs and choose **Convert**. Lines become boards using center/left/right alignment relative to the line's start-to-end direction; circles become pilings with their original diameter. Original objects remain unless **Replace** is enabled. A failed batch changes nothing. Unselected objects, including HVAC notes, are never classified, converted, or moved.

Orbit in 3D by dragging; Shift/right drag pans; the wheel zooms; double-click or **Fit model** fits the members. Click a member and edit it in **Properties**. Interactive placement enters Plan. Repeat can use a preselected member or prompt for one. **2D reference** displays ordinary geometry as a flat contextual reference, without inventing elevations.

## Physical joints and future cut optimization

Lumber Properties distinguish **Engineering span**, **Start/End setbacks**, and the resulting **Physical cut length**. Engineering endpoints remain the intended connection points. Setbacks shorten the plan footprint and 3D faces inward from those endpoints. This lets engineering centerlines connect while the actual wood meets a face instead of intersecting the other member.

Select a board, choose **Butt end to member…**, pick the reference, then choose Start, End or Auto. The command connects that engineering endpoint to the reference axis and calculates a square end setback from its physical face. An angled reference uses a conservative square cut that clears its whole width; miters and shaped cuts are future work. A member resting above a support keeps zero setback because the two do not overlap vertically. Optional JSON `gap` adds clearance. Both the joint and property changes are undoable and remain independent of later edits to the reference.

**Check collisions** reports positive-volume overlaps between recognized boards, rectangular posts and circular pilings. Touching faces and correctly stacked members are valid. Select a reported pair to review it, then use a butt joint, setbacks or elevations to resolve it. The check never moves other objects. Its pair and result budgets return errors on excessive assemblies. It is an interference check, not connection design or a structural analysis.

Future wood cutting optimization should consume `contractor.list`'s physical `cutLength`, actual section, nominal profile, material and `unitMm`. Engineering span is not a stock cutting length. Stock selection, kerf, waste optimization, cut diagrams, notches and angled cuts are not implemented. Keep that future optimizer separate from member storage and the UI.

Move/copy/rotate/mirror/erase and undo/redo work on the normal INSERT. Resize through Contractor properties. SCALE and generated-block editing are protected; EXPLODE explicitly converts a member into ordinary CAD geometry. Externally modified blocks and unknown metadata versions remain ordinary CAD objects, with diagnostic reasons available from `contractor.list`.

## Commands and file exchange

All commands have JSON forms, listed with `cadcraft-cli commands contractor`. JSON execution never opens a dialog. The same catalog is exposed over control and MCP.

```json
{"command":"contractor.board","params":{"start":[0,0],"end":[0,288],"size":"2x12","role":"beam","bottom":0}}
{"command":"contractor.board","params":{"start":[0,0],"end":[288,0],"size":"2x10","onTopOf":"MEMBER_HANDLE"}}
{"command":"contractor.repeat","params":{"handle":"MEMBER_HANDLE","spacing":16,"count":19}}
{"command":"contractor.view","params":{"mode":"model3d"}}
```

Handles in examples are placeholders; use the returned hexadecimal handle. `contractor.tool` sets per-document interactive placement defaults, then `contractor.board 0,0 120,0` uses the usual command-line point grammar. `contractor.post`, `contractor.convert`, `contractor.set`, `contractor.list`, and `contractor.sample` cover the other workflows. `contractor.set` accepts a single `handle` or a batch of `handles`; the whole batch succeeds or nothing changes. Length parameters accept numbers or unit strings. Support edits accept `top` and `depthBelowTop`; board edits accept `startSetback` and `endSetback`.

```json
{"command":"contractor.set","params":{"handles":["HANDLE_1","HANDLE_2"],"depthBelowTop":"12'"}}
{"command":"contractor.base","params":{"handle":"BOARD_HANDLE","reference":"BEAM_HANDLE","side":"above"}}
{"command":"contractor.join","params":{"handle":"BOARD_HANDLE","reference":"RIM_HANDLE","end":"start","gap":"0\""}}
{"command":"contractor.collisions","params":{}}
```

`contractor.export` takes `path`, `representation` (`plan`, `engineering`, or `model3d`), and for PDF `paper`, `landscape`, `scale`, and `fit`. Scale is a physical paper/model ratio: 1/48 means ¼ inch per foot regardless of model units. The default is ANSI D landscape, 1:48, **fit off**. Choose a paper and scale that contain the drawing; explicit scale may clip a drawing larger than the page. Existing dimension entities are retained; add project dimensions using the normal dimension tools. Exported elevation labels use a 2.5 mm paper text height at the selected scale.

PDF supports Plan and Engineering. Short member tags link to a key with sizes and bottom/top elevations; native background masks keep lines clear of the text. Tags and keys are derived on a clone, including in the Engineering canvas. DXF/DWG support all three representations and retain metadata and the hidden representations for subsequent Contractor editing. The existing native DWG bridge writes AC1032 (2018) by default. DWG remains native-only, as in upstream; browser builds support Contractor modeling, viewing, PDF and DXF. Export clones the source drawing and does not change it or mark it saved. Standard Save/Save As also retain members.

`ui.render` honors the current Contractor 3D representation using the bounded CPU renderer, even without a presented frame. An optional `camera` object has `center:{x,y,z}`, `yaw`, `pitch` (radians), and `height`; `fit:false` preserves it. Large images or excessive raster work return an error. Native screenshots use the GPU viewport when available.

## Architecture and upstream maintenance

The optional `contractor` Cargo feature is default-on for this fork's apps and available on engine, UI and MCP crates. Each app and crate also builds with `--no-default-features`. No Contractor dependency is introduced into doc, render or IO.

The L2 `cadcraft-contractor` crate owns the catalog, bounded schema, native geometry, decoding, centroid axes, camera, picking and software depth renderer. Engine adapters own commands and transactions. The egui adapter owns temporary controls and an independent offscreen GPU renderer with a Depth32Float attachment. Neither renderer owns a second editable model.

One INSERT carries `CADCRAFT_CONTRACTOR` xdata with material, nominal profile, role, label, and shape dimensions. Version 1 retains its original geometry and defaults to zero setbacks. Version 2 adds bounded `setbacks:{start,end}` for square board end cuts; its shape length remains the engineering span. Readers validate both versions and leave unsupported versions as ordinary geometry. Earlier v1 readers similarly leave v2 members ordinary instead of rebuilding them. Placement comes from the INSERT transform. The payload is limited to 8 KiB and split at UTF-8 boundaries into 255-byte DXF strings. Metadata lives on INSERTs because the DWG bridge preserves entity xdata but has gaps for block/table metadata. Recognized definitions must match the generated geometry. Editing creates a private replacement block, preserving the source handle and other references' definitions.

Reserved representation layers are `CC_CONTRACTOR_PLAN`, `CC_CONTRACTOR_AXIS`, and `CC_CONTRACTOR_MESH`. Generated export annotations use `CC_CONTRACTOR_NOTES`. Layer flags save the active representation. All three representations come from one parameter set. Standard geometry gives other CAD software useful blocks and 3D faces without this extension; these are faces, not ACIS solids.

Shared integration points, intentionally kept small:

| Area | Integration |
|---|---|
| Workspace manifests | Optional dependency and feature forwarding; layer-table entry |
| Engine registry / DocState | Command registration and per-document tool preferences |
| Modify / Properties / block editing | Reject unsupported resizing; protect generated definitions; explode recognized members with full world coordinates |
| File adapter | Reuse atomic native storage / browser delivery and installed IO hooks |
| CadApp / Tool Sets / Properties | Contractor state, renderer setup, third tab, specialized Properties body and friendly member type labels |
| Desktop workspace persistence | Accept Contractor as a saved tab; use Drafting when reading that preference with the feature disabled |
| Display-time translator | Merge the Contractor reference-locale catalog; upstream entries win |
| Canvas / rebuild / control | 3D dispatch, member picks for reference prompts and presentation-only engineering annotations |

After an upstream merge, inspect these points first. Run the Contractor storage/command/UI tests, feature-disabled app checks, and `cargo xtask ci`. Visually inspect the three views and repeat a DWG save/reopen/edit cycle. Do not alter native parity assessments to imply a general-purpose 3D CAD kernel. New materials and section types should extend the isolated model and metadata version deliberately; do not add speculative fields or a second database.

Agent notes live beside the model crate and both adapter modules. Reusable learning for shared crafting standards: put extension metadata on existing graphical entities where possible, verify the actual interchange bridge before designing storage, and keep native fallback geometry when an extension is unavailable. The referenced shared `craftrules` checkout and local `plan/` files were unavailable in this checkout; no shared repository was modified.

## Validation record

- The 2026-10-22 upstream integration passed all six repository gates with 2,291 tests and all
  native/browser feature combinations, plus eight feature-disabled workspace tests. The final
  0.11.0 Windows desktop preserved all 31 sample members/elevations through native DWG reopening;
  individual-depth editing, unaffected members and undo passed. All three views and Properties
  were inspected, and the 1:48 PDF retained 432-point joist axes. See the [update record](upstream-updates.md).

- Native storage tests exercise all three shapes and representations through two DXF/DWG cycles, then edit reopened members. Generated-name collision tests protect unrelated block definitions during creation and editing.
- Command tests cover stack elevations, independent placement, on-center repetition, copy-on-edit, conversion preservation/atomicity, transforms, interactive input, hostile numbers and undo/redo.
- Rendering tests check depth-order-independent CPU picking and GPU drawing, positive elevation appearing upward, bounded software rendering and shader validation; UI evidence is generated under ignored `temp/` when requested.
- Visually inspected full-window Plan, Engineering and 3D views, plus exported Plan/Engineering PDFs. ANSI D landscape is 2448 × 1584 PDF points; 24-foot members measure 432 points (6 inches) at 1:48 in both exports.
- `cargo xtask ci` passed all six gates: formatting, warning-free native Clippy, 2,033 tests, assets, layers and all 12 WASM crate checks. Validation used Rust 1.99.1 GNU on Windows; test debug symbols were omitted to bound disk use. Existing WASM worker-helper dead-code warnings remain.
- Native desktop, CLI, web, engine, UI and MCP feature-disabled checks passed. The browser application also checked successfully for WASM with Contractor both enabled and disabled.
- Original v1 optimized Windows desktop and CLI builds succeeded. Its release CLI exposed all 13 original Contractor commands, exported a scaled engineering PDF, reopened all 31 DWG sample members without metadata issues and verified joist bottoms equal beam tops. The native desktop started with an isolated configuration and rendered the synthetic assembly through its control channel; the resulting 900 × 700 image was visually inspected.
- The v1.1 update passed all six repository gates with 2,040 tests, plus eight native workspace tests with Contractor disabled. New tests cover selecting four of twelve pilings, independent depths, atomic batch failure, unit strings in metric drawings, Above/Below reference placement, angled square joints, valid surface contact, v2 DXF/DWG cycles in all representations, copy/repeat, mixed-value property drafts, and workspace-tab persistence. Full-window piling and lumber Properties images and all three views were inspected. A native desktop check used real 3D reference clicks for Above/Below and verified undo. The release CLI checked all 16 commands, joint cut length, zero overlaps and native DWG reopening. The joint PDF was visually inspected and its 120-inch engineering span measured 360 PDF points at 1:24. Feature-disabled and browser checks remain passing; existing WASM worker-helper warnings are unchanged.

## Revision history

| Date | Change |
|---|---|
| 2026-10-22 | Upstream 0.11.0 integration retains Contractor model, commands, Properties and member metadata; optional desktop Discord presence is a separate adapter |
| 2026-10-18 | Item-specific Properties, batch edits, physical unit entry, Above/Below base selection, version 2 square end setbacks and collision checks; future cut-optimization boundary |
| 2026-10-17 | Initial Contractor floor-framing implementation, storage schema, agent boundaries and maintenance checklist |
