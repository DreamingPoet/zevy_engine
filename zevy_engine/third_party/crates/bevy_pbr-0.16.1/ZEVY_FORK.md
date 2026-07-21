# Zevy bevy_pbr fork

- Upstream package: `bevy_pbr 0.16.1`
- Upstream source SHA recorded by Cargo: `383b3510455c431f34cf3f2c6e3c2d40eddce744`
- License: upstream dual MIT OR Apache-2.0; original license files are retained.

## Purpose

This fork adds a fixed-budget PointLight selection payload to storage-buffer
cluster headers. It lets Zevy test one shared `2 Hero + 2 Tail` set per
Cyclopean XR supercluster without adding another binding or render pass.

The 2x2 screen-supercluster consumer is retained as an explicit performance
reference only: headset motion testing exposed screen-locked brightness blocks.
The product path first evaluates local lists of up to eight lights exactly;
dense overflow currently uses a world-stable experimental path and must gain
stereo-shared reconstruction before raw stochastic shadows can ship.

## ABI changes

- Storage `ClusterOffsetsAndCounts` entries contain four `vec4<u32>` values.
- Entries 0 and 1 preserve Bevy's original offset/count layout.
- Entry 2 stores four global PointLight indices.
- Entry 3 stores four estimator weights as f32 bit patterns.
- Uniform-buffer platforms retain the original packed layout and report an
  invalid preselection, so Zevy falls back to its scalar shader path.
- No mesh-view binding or render-pass count is added.

The Zevy-facing API is limited to cluster dimensions/AABB access,
`point_light_entities`, and `set_preselected_point_lights`. Keep these changes
isolated when rebasing to another Bevy version.
