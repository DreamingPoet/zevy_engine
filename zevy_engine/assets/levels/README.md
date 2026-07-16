# Zevy imported Levels

Unreal Engine exports each editable schema-v2 Level into its own folder here:

    assets/levels/<LevelName>/<LevelName>.zevy-level.json
    assets/levels/<LevelName>/assets/<AssetName>_<Hash>/<AssetName>_<Hash>.gltf
    assets/levels/<LevelName>/assets/<AssetName>_<Hash>/<AssetName>_<Hash>.bin
    assets/levels/<LevelName>/assets/<AssetName>_<Hash>/*.png

Load an exported Level on desktop with:

    cargo run -- --level=levels/<LevelName>/<LevelName>.zevy-level.json

The JSON path is relative to the Bevy asset root (`assets`). Asset paths inside
the manifest are relative to the Level folder. Actor parent relationships and
local transforms stay in the JSON and can be edited manually. Materials remain
editable in each glTF JSON file and textures are external PNG files.
Directional, point, and spot lights also keep editable Bevy-ready values plus
their retained Unreal attenuation, unit, temperature, source-size, and shadow
metadata in each entity's `lights` array.

`ZevyExporterFixture` keeps the schema-v1 compatibility fixture.
`ZevyExporterFixtureSplit` is the schema-v2 fixture with three attached Static
Mesh actors, independent assets, external textures, and one directional,
point, and spot light.

`Map_S03B` is the real exported Level used for the current visual validation.
