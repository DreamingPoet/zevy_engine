# UE Level → Zevy/Bevy 导出插件开发规范

状态：Phase 1.1 已实现并通过真实关卡验证  
基线：Unreal Engine 5.5.4、Bevy 0.16.1、glTF 2.0  
最后验证日期：2026-07-16

## 1. 项目目标

在 Unreal Editor 中编辑 Level，然后导出为可由 `zevy_engine` 直接加载的内容。导出结果必须继续适合人工维护，而不是把整个关卡烘焙成一个不可拆分的大文件。

当前阶段覆盖：

- Static Mesh Actor / Static Mesh Component。
- Actor 材质覆盖、PBR 材质烘焙与 PNG 纹理。
- Actor Attach 层级。
- 每个 Actor 相对父 Actor 的可编辑局部位置、旋转和缩放。
- 相同模型和材质组合的实例复用。
- Directional、Point、Spot Light。
- Packed Level Actor / Level Instance 的 glTF 场景导出入口。
- Windows Zevy 加载、无窗口验证和自动取景截图。

后续阶段再覆盖 Collision、Gameplay metadata、Rect/Sky Light、Light Function、IES、静态光照贴图、Landscape、Niagara、Skeletal Mesh、World Partition 流送等内容。

## 2. 最终文件结构

schema v2 使用“Level 清单 + 独立资产目录”，不再要求整个 Level 共用一个 GLB：

```text
assets/levels/Map_S03B/
  Map_S03B.zevy-level.json
  Map_S03B_preview.png
  assets/
    SM_S03_DiMian_L_1f06d359/
      SM_S03_DiMian_L_1f06d359.gltf
      SM_S03_DiMian_L_1f06d359.bin
      *.png
    SM_S03_Qiang_L_8e92a0c7/
      SM_S03_Qiang_L_8e92a0c7.gltf
      SM_S03_Qiang_L_8e92a0c7.bin
      *.png
```

职责划分：

- `.zevy-level.json`：Actor ID、名称、父子层级、局部 TRS、可见性和资产引用。
- `.gltf`：单个唯一模型/材质组合的节点、网格和可人工编辑的材质参数。
- `.bin`：顶点、索引等二进制网格数据。
- `.png`：外部纹理，可直接用图像工具替换或修改。
- `preview.png`：Zevy 桌面渲染入口生成的验收截图，不参与运行时加载。

旧 schema v1 的单 GLB 清单仍可被 Zevy 加载，以保持已有资产兼容。

## 3. schema v2

最小示例：

```json
{
  "schema_version": 2,
  "level_name": "Demo",
  "assets": [
    {
      "id": "asset_crate",
      "name": "SM_Crate",
      "scene": "assets/SM_Crate/SM_Crate.gltf",
      "scene_index": 0
    }
  ],
  "entities": [
    {
      "id": "actor_parent",
      "name": "Parent",
      "parent": null,
      "asset": null,
      "transform": {
        "translation": [0.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0, 1.0],
        "scale": [1.0, 1.0, 1.0]
      },
      "visible": true
    },
    {
      "id": "actor_child",
      "name": "Crate",
      "parent": "actor_parent",
      "asset": "asset_crate",
      "transform": {
        "translation": [1.0, 0.0, 0.0],
        "rotation": [0.0, 0.0, 0.0, 1.0],
        "scale": [1.0, 1.0, 1.0]
      },
      "visible": true
    }
  ]
}
```

### 3.1 Actor 层级与局部 Transform

- `id` 在同一个清单内必须唯一。
- `parent: null` 表示 Actor 直接挂在 Level 根节点下。
- `parent` 填另一个 Actor 的 `id`，表示 Bevy `ChildOf` 关系。
- `translation` 单位为米，坐标系为 glTF/Bevy 的右手 Y-up。
- `rotation` 是 `[x, y, z, w]` 四元数；手工编辑后不能为零四元数，加载时会归一化。
- `scale` 是局部 XYZ 缩放。
- `asset: null` 允许保留没有模型的 Attach 中间父节点。
- `visible` 控制整个 Actor 子树的可见性。

Zevy 加载器会拒绝以下错误：

- 重复 Actor ID 或资产 ID。
- 引用不存在的父 Actor 或资产。
- Actor 层级循环。
- Transform 中的 NaN/Infinity 或零旋转四元数。
- 越出 Level 目录的资产路径。

因此可以直接修改 `parent` 和局部 TRS，重新运行 Zevy 即可看到新的层级和摆放结果。

### 3.2 资产复用与单独修改

UE 导出器按 Actor 内容签名去重。普通 Static Mesh Actor 的签名包含：

- Actor class。
- Static Mesh 路径。
- Actor 内部 Component 层级和局部变换。
- 每个材质槽实际使用的材质。

多个 Actor 使用相同签名时，共享同一个 `assets[]` 项，但在 `entities[]` 中仍保留各自独立的位置、旋转、缩放和父节点。

修改共享 `.gltf` 或 PNG 会影响所有引用该资产的 Actor。如果只想修改一个实例：

1. 复制对应资产文件夹。
2. 在 `assets[]` 中增加一个新 ID 和新 `scene` 路径。
3. 把目标 Actor 的 `asset` 改为新 ID。

Level Instance、灯光、ISM/HISM 和 Spline Mesh 当前优先保证准确性，默认使用唯一资产签名，避免错误合并。

## 4. 坐标转换

Unreal 使用左手 Z-up、厘米；glTF/Bevy 使用右手 Y-up、米：

- 位置：UE `(X, Y, Z) × 0.01` → Bevy `(X, Z, Y)`。
- 旋转：UE 四元数 `(x, y, z, w)` → Bevy `(-x, -z, -y, w)`。
- 缩放：UE `(X, Y, Z)` → Bevy `(X, Z, Y)`。

Attach Actor 的清单 Transform 使用当前世界变换相对父 Actor 世界变换计算，确保导入后的静态摆放与 UE 一致。源 Attach Component 和 Socket 名会记录在 `source_attachment` 中，当前 Zevy 运行时不执行 Socket 动画语义。

已知风险：父节点非均匀缩放叠加子节点旋转时，UE 与 glTF 的 TRS 分解都可能无法表示矩阵剪切；后续可增加矩阵烘焙模式。

## 5. 材质与纹理

UE 5.5 官方 GLTFExporter 负责把材质转换或烘焙为 glTF Metallic-Roughness PBR：

| UE/glTF | Bevy `StandardMaterial` |
| --- | --- |
| Base Color | `base_color` / `base_color_texture` |
| Metallic | `metallic` |
| Roughness | `perceptual_roughness` |
| Normal | `normal_map_texture` |
| Ambient Occlusion | `occlusion_texture` |
| Emissive | `emissive` / `emissive_texture` |
| Masked | `AlphaMode::Mask` |
| Translucent | `AlphaMode::Blend` |
| Two Sided | 关闭背面剔除 |
| UV Offset/Tiling | `KHR_texture_transform` |

当前导出设置：

- 纹理固定为外部 PNG。
- `BakeMaterialInputs = UseMeshData`。
- 调整法线贴图绿色通道以匹配 glTF。
- 不启用 Mesh Quantization，优先保证兼容和精度。

`.gltf` 是 JSON，可以手工修改 `materials` 中的颜色、金属度、粗糙度、透明模式和纹理引用。PNG 可直接替换，但要保持文件名或同步修改 `.gltf` URI。

已知降级：`Map_S03B` 的 `MI_S03B_Fog` 使用 Additive 混合，glTF 没有等价的标准混合模式，UE 导出器会降级为 Translucent。

## 6. 灯光

当前通过 `KHR_lights_punctual` 支持：

| Unreal | glTF | Bevy |
| --- | --- | --- |
| DirectionalLightComponent | directional | `DirectionalLight` |
| PointLightComponent | point | `PointLight` |
| SpotLightComponent | spot | `SpotLight` |

灯光 Actor 与模型 Actor 一样拥有独立清单节点、父节点和局部 TRS。测试夹具已验证一个附着在模型父节点上的 Point Light，以及独立 Directional/Spot Light。

schema v2 还会在对应 `entities[]` 项中写入可手工编辑的 `lights[]`：

- `bevy.color_srgb`：最终应用到 Bevy 灯光的颜色。
- `bevy.intensity`：Point/Spot 使用流明，Directional 使用 lux。
- `bevy.range_m`：Point/Spot 的有效范围，单位为米。
- `bevy.radius_m`：Point/Spot 的发光源半径，用于高光尺寸和软阴影半影。
- `bevy.attenuation_model`：当前为 Bevy 标准 `inverse_square_cutoff`。
- `bevy.shadows_enabled`、Spot 内外锥角和灯光启用状态。
- `unreal.*`：保留 UE 原始强度与单位、颜色、色温、Attenuation Radius、Inverse Exposure Blend、Inverse Squared Falloff、Falloff Exponent、Source/Soft Source Radius、Source Length、Shadow Bias 和 Mobility。

Zevy 在 glTF 实例化后用 `lights[].bevy` 精确覆盖实际 `PointLight` / `SpotLight` / `DirectionalLight`，并把完整定义挂在公开组件 `ImportedZevyLight` 上，便于运行时或编辑工具继续修改。UE 的自定义 `LightFalloffExponent` 在 Bevy 0.16 标准 PBR 灯光中没有完全等价实现；这类灯光目前使用 inverse-square cutoff 近似渲染，但原始衰减方式和指数不会丢失。

运行时必须尊重显式的 `lights[].unreal.mobility = "static"`。Map_S03B 的 authored-to-runtime 灯光校准（当前强度 `×1000`、范围 `×4`）会对所有 PointLight 应用一次，保证 static 灯也具有该关卡所需的有效照明；mobility 决定校准后的值是否随时间变化。static 灯保持颜色和 Transform，校准后的亮度与范围固定，不生成蜡烛发光体，不参与亮度、范围、位置动画，也不进入周期性 candle shadow invalidation；若启用阴影，其静态 depth 在首次生成后进入持久化缓存。只有显式 `static` 才启用该规则，旧清单缺失 mobility 时保持兼容行为。`stationary` 的 UE 混合光照语义尚未完整实现，当前不会被误判为 static。

Zevy 桌面预览只在导入关卡没有灯光时启用非破坏性的相机补光；检测到导入灯光后会隐藏默认 Directional Light。

## 7. UE 插件入口

编辑器菜单：

```text
Tools > Zevy > Export Current Level to Zevy...
```

默认输出：

```text
zevy_engine/assets/levels/<LevelName>/<LevelName>.zevy-level.json
```

Commandlet：

```powershell
UnrealEditor-Cmd.exe <Project.uproject> `
  -run=ZevyLevelExport `
  -Map='/Game/Path/MapName' `
  -Output='.../MapName/MapName.zevy-level.json' `
  -unattended -AllowCommandletRendering
```

当 `-Output` 以 `.glb` 结尾时仍使用兼容的 schema v1 单文件导出；其他输出默认使用 schema v2。

## 8. Zevy 加载与验证

运行真实桌面 Level：

```powershell
cargo run --offline -- --desktop `
  --level=levels/Map_S03B/Map_S03B.zevy-level.json
```

PC 模式会给默认 `Camera3d` 挂载独立的自由漫游玩家，手感参考 UE
DefaultPawn/编辑器飞行视口：

- `W/A/S/D`：相对当前视角移动。
- 按住鼠标右键：锁定鼠标并旋转视角。
- `Q/E`：下降/上升。
- `Shift`：冲刺加速。
- 鼠标滚轮：动态降低/提高基础移动速度，范围为 0.5–80 m/s。
- `Esc`：释放鼠标。

控制器带加速和减速，不会瞬间切换速度。它只在 `LaunchMode::Desktop`
的相机上创建；`LaunchMode::Xr` 继续使用原有 OpenXR Camera、
`XrTrackingRoot` 和手柄摇杆移动。当前为便于检查模型细节而使用 no-clip
漫游，待 Collision 数据进入 Level schema 后再增加可选碰撞模式。

自动取景并保存截图：

```powershell
cargo run --offline -- --desktop `
  --level=levels/Map_S03B/Map_S03B.zevy-level.json `
  --screenshot=assets/levels/Map_S03B/Map_S03B_preview.png
```

无窗口验证：

```powershell
cargo run --offline --bin validate_zevy_level -- `
  levels/Map_S03B/Map_S03B.zevy-level.json
```

验证器会检查：

- 清单和全部递归 glTF/纹理依赖加载。
- 每个资产 SceneRoot 实例化完成。
- Actor ID、名称、父节点与局部 Transform 与清单一致。
- 模型、材质和三类灯光已生成，且 Bevy 灯光组件值与 `lights[]` 可复用参数一致。
- schema v1 与 v2 兼容性。

## 9. 2026-07-16 验证结果

### 自动夹具

- 6 个 Actor 节点。
- 父、子、孙三层 Static Mesh Attach。
- 3 个独立模型资产、2 种源材质、3 类灯光。
- v2 运行结果：27 个后代实体、6/6 个 Scene 实例、3 个 Mesh、3 个运行时材质实例、1/1/1 个 Directional/Point/Spot Light。
- Actor 层级和每一级局部 Transform 数值比对通过。

### `Map_S03B`

源 Level：`/Game/crates/Emperor/S03/Arts/Map/Map_S03B.Map_S03B`

- 32 个 Static Mesh Actor / Component。
- 39 个可编辑 Actor 节点，其中 32 个 Static Mesh Actor、7 个 PointLight Actor。
- 32 个独立资产，其中 25 个模型/材质组合资产、7 个灯光资产。
- 23 种 UE 源材质。
- 32 个 `.gltf`、25 个 `.bin`、24 个资产 PNG。
- 0/7/0 个 Directional/Point/Spot Light；7 个 PointLight 均保留 UE 参数并应用 Bevy 参数覆盖。
- 当前 7 个 PointLight 均为 8 cd、1.5 m Attenuation Radius、inverse-square falloff、启用阴影；转换后的 Bevy 强度为约 100.531 lm。
- Zevy 实例化：163 个后代实体、39/39 个 Scene 实例、32 个 Mesh、25 个运行时材质实例、7/7 个 PointLight 可复用参数组件。
- Actor 层级和局部 Transform 校验通过。
- 1600×900 Zevy 实际渲染截图已生成。

## 10. 后续开发计划

### Phase 2：资产生产化

- 增量导出与内容 Hash 缓存。
- 共享纹理去重，避免不同 glTF 文件夹重复 PNG。
- 可选“每 Actor 强制独立资产”和“共享唯一资产”模式。
- 过期导出文件清理与导出报告面板。
- glTF Validator 自动执行。

### Phase 3：材质和灯光语义增强

- Additive、复杂透明、Substrate 的专用降级策略。
- Rect Light、Sky Light、IES、Light Function、阴影与温度扩展字段。
- Zevy Lighting Profile 和移动 VR 灯光预算。

### Phase 4：场景能力

- Collision、Gameplay Tags、自定义 Actor metadata。
- Fog、Sky、Reflection Probe。
- Level Instance 的增量/流送语义。
- Landscape、Skeletal Mesh、Animation、Niagara 替代管线。

### Phase 5：PICO 4 Ultra 验收

- Android 资产完整性检查。
- APK 体积、纹理内存、Draw Call、灯光与阴影预算。
- Windows 与 PICO 的 Transform、材质和视觉对照。
