# Zevy exported mip texture sidecar v1

Each exported glTF PNG texture may have a sibling file with the same stem and the
extension `.zevy-mips`.

Example:

```text
T_S03B_Floor_D.png
T_S03B_Floor_D.zevy-mips
```

The PNG remains the glTF-compatible fallback. Zevy loads the sidecar when present.

All integers are unsigned 32-bit little-endian values.

| Offset | Size | Field |
| --- | ---: | --- |
| 0 | 8 | ASCII magic `ZEVYMIP\0` |
| 8 | 4 | Version (`1`) |
| 12 | 4 | Flags; bit 0 means debug mip numbers are embedded |
| 16 | 4 | Base width |
| 20 | 4 | Base height |
| 24 | 4 | Mip level count |
| 28 | 4 | Reserved (`0`) |

The header is followed by `mip level count` records. Each record contains:

| Size | Field |
| ---: | --- |
| 4 | Level width |
| 4 | Level height |
| 4 | PNG byte length |
| N | PNG bytes containing RGBA8 pixels |

Levels are stored from mip 0 through the final 1x1 level. Each level must have
dimensions `max(previous / 2, 1)`.

Color-space interpretation is intentionally not stored in the sidecar. The Bevy
loader preserves the texture format selected by the glTF material usage: color and
emissive textures remain sRGB, while data textures remain linear.

When the debug-number flag is enabled, each level contains a repeated, colored
numeric label for that mip level. This makes implicit GPU LOD selection visible while
the camera moves.
