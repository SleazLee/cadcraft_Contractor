# Contractor UI adapter

Read root AGENTS.md and both Contractor model/command AGENTS.md files. This module owns temporary controls, the 3D camera, and GPU/CPU viewport resources. Drawing mutations always use app.run/app.start; selection uses the engine select command. Do not maintain a second editable model.

Use theme tokens for UI colors. Key property drafts and scene caches by drawing UID as well as handle/revision: separate drawings reuse handles and revision numbers. The mesh comes from validated model members and carries source handles for picking. The offscreen GPU renderer owns its own depth attachment; the bounded software z-buffer remains usable without a GPU. Legacy geometry is a flat reference, never inferred structural geometry. Release native textures when resized or dropped.

Shared hooks are CadApp state/renderer setup, the third Tool Sets tab and the model-space canvas dispatcher. Keep them feature-gated and small. Check native/WASM and feature-disabled builds; inspect plan, engineering and 3D images, selection, property edits, resizing, and reference overlays. Keep future upstream merge guidance in docs/contractor.md.

Keep extension command/menu translations in i18n.rs. The host translator merges them conditionally and preserves upstream entries. Follow the host's required Ukrainian reference coverage and English fallback policy; never translate command IDs, profile IDs, or stored member data.
