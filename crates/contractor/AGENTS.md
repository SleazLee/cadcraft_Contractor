# Contractor model boundary

Read the root AGENTS.md first. This crate is L2 and may depend on doc, geom, color and serialization utilities, never engine, io, render, or a UI toolkit. Integration tests needing IO live in engine/tests/contractor_storage.rs to preserve layering.

Each member is an ordinary INSERT with CADCRAFT_CONTRACTOR xdata on the INSERT, not the BLOCK or BLOCK_RECORD (the native DWG bridge loses block metadata). Version 1 stores material/profile/role/label and a rectangular or round shape. The INSERT transform owns world placement. Keep the named representation layers and all geometry generated from that record. Decode unknown versions or modified definitions as ordinary CAD geometry; do not overwrite them.

Metadata is capped at 8 KiB with UTF-8-safe DXF chunks of at most 255 bytes. Bound dimensions, positions, counts, image allocations, and raster work. No panic or unsafe code. Edits use a new block definition so copied boards remain independent. Check generated names case-insensitively and never replace an existing definition; collision probing is bounded at 1024 suffixes.

Version 1's circular pile uses a 32-sided visual mesh and an exact plan circle. Preserve this generator/validator for existing v1 files when introducing a different topology. Generated blueprint tags, keys, and background masks have their own versioned CADCRAFT_CONTRACTOR_NOTES marker and are regenerated on a clone. Reserve drawing handles with checked arithmetic before generating members or notes.

Version 2 adds square start/end setbacks for boards. Shape.length is the engineering span; physical cut length subtracts both setbacks. Keep v1 zero-setback geometry byte-compatible and validate both versions. Engineering axes never shrink to the cut ends. Joint placement samples a reference; do not introduce hidden constraints. Collision checks distinguish positive volume overlap from valid surface contact, bound their broad-phase work and handle round pilings analytically. Future cutting optimization consumes physical lengths and must remain a separate module.

Required checks: repeated DXF and DWG round trips for all shapes and representations; centimeter/inch conversions; physical top/bottom and centroid axes; hostile metadata and raster dimensions; depth-order-independent picking. Run cargo xtask ci and feature-disabled checks before committing. Lumber dimensions are numeric facts from PS 20-25 Table 3; descriptions and geometry are original.
