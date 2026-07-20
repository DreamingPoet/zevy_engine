# 任务检查点：Wave A 全帧遥测与固定 A/B 实现

## 元数据

- 更新时间：2026-07-20 16:15，Asia/Shanghai
- 状态：实现子阶段已完成；Android/VR 固定路径 A/B 数据待用户实测
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ 6a09a9f`（`up`）
- 下一恢复入口：`Docs/Checkpoints/CURRENT.md`
- 本阶段历史快照：`Docs/Checkpoints/2026-07-20-wave-a-telemetry-implementation.md`

## 最终目标和完成标准

### 最终目标

建立面向 Android VR 一体机的高性能、现代动态多灯光/阴影渲染器，支持大量动态灯、动态阴影、PBR、可编辑 UE Level 导入和稳定双眼输出。实现路径允许升级、fork、vendor、回移或替换 Bevy、wgpu、Naga、OpenXR 插件及渲染管线。

### 产品完成标准

- Map_S03B 至少 16 盏 PointLight 的直接光和阴影可同时存在。
- 灯光物理照射范围与相机可见距离解耦；不扩大 `light.range`，不按相机距离开关灯光或阴影。
- 左右眼共享 visibility、LOD、灯光、阴影、随机采样和历史状态，无 binocular mismatch。
- 20～30 分钟 thermal soak 后 P95 ≤ 13.89 ms（72 Hz）；支持设备上争取 P95 ≤ 11.11 ms（90 Hz）。
- UE 导入继续保持独立资产、Actor Attach 层级、局部变换、材质/纹理/灯光参数和手动编辑能力。

### 当前 Wave A 完成标准

- [已实现] HUD 能分开显示 main、visibility、static shadow、dynamic shadow、post、UI 和 other/compute 的可获得时间与 GPU counter。
- [已实现] 显示主视图 geometry/draw/batch、静态/动态 shadow caster、redraw/reuse、shadow texel 和 fragment/overdraw 代理。
- [已实现] 提供固定、双眼一致且非相机距离驱动的 direct/shadow 四组合 A/B。
- [已验证] Rust 单测、Windows 运行、Android 交叉编译与无调试 feature 的 Shipping 编译。
- [待真机] 用同一路径跑四组 A/B，定位约 50 ms 中最大的两个模块并记录 VR P50/P95/P99。

## 已完成内容

### 实现

- HUD 从三页扩展为四页：`Overview → Full-frame Workload → GPU/Render Passes → Materials/Lights`；F3/右手柄 A 显隐，F4/右手柄 B 切页。
- 新增最近 10 秒 frame-time P50/P95/P99，不再只看瞬时 FPS。
- 将 Bevy 0.16 render diagnostics 聚合为七类 workload；支持 GPU timestamp、vertex invocations、clipper primitives、fragment invocations 和 compute invocations。
- 不支持 GPU query 的设备明确显示 CPU command-recording fallback 或 `N/A`，不把不支持误报成零成本。
- 新增主视图可见 vertex/triangle、opaque/transparent entity、draw 估算、batch savings；新增加载态 static/dynamic shadow caster entity/triangle。
- 新增 static/dynamic updated faces、shadow texel 更新量和 face-frustum 前 caster triangle 上界；真实提交量仍以 GPU counter/AGI 为准。
- 修正 bottleneck 判断：按聚合后的 shadow/main/visibility/post workload 判断，不再因每个 shadow face 被拆散而漏报，也不再建议扩大/缩小物理灯光 range 解决可见性问题。
- `RenderQualityConfig` 新增：
  - `point_light_direct_lighting`；
  - `point_light_shadows`。
- direct 关闭时，Zevy WGSL 用编译期常量移除 PointLight 候选扫描、BRDF 与 shadow lookup，不是仅把 intensity 设为零。
- shadow 关闭时，Map_S03B 的固定 shadow residency 变为零；仍不依赖任何相机或眼睛。
- 四种稳定 profile：Full、Direct Only、Shadow Submission Only、Geometry/Post Floor。
- `render_debug` feature 关闭后，HUD、render queries、字符串构造及 shadow telemetry atomic 写入均不进入 Shipping 路径。

### PC 运行证据

- Map_S03B 成功加载：schema 2、39 个独立资产、41 个实体、16 个 PointLight、96 个 PointLight cubemap shadow views。
- 新 Zevy PBR WGSL 被实际安装并运行；未出现 shader/RenderGraph panic。
- 截图成功生成：`zevy_engine/target/render_debug/Map_S03B_render_passes.png`。
- PC 截图只证明运行正确性和 HUD 可见性，不代表 Android VR 性能与双眼视觉验收。

### 文档

- `zevy_engine/docs/VR_Renderring.md` 新增 Wave A HUD 指标语义、GPU/CPU fallback、四组 A/B 矩阵、cache-hot/full-redraw 测法和 Shipping feature 说明。

## 当前文件和修改状态

阶段完成时工作区未提交，HEAD 仍为 `6a09a9f`。修改文件：

```text
 M Docs/Checkpoints/CURRENT.md
 M zevy_engine/docs/VR_Renderring.md
 M zevy_engine/src/config.rs
 M zevy_engine/src/render_debug.rs
 M zevy_engine/src/scalable_lighting.rs
 M zevy_engine/src/scene.rs
 M zevy_engine/src/shaders/zevy_pbr_functions.wgsl
 M zevy_engine/src/shadow_cache.rs
?? Docs/Checkpoints/2026-07-20-wave-a-telemetry-implementation.md
```

用途：

- `config.rs`：固定 A/B 开关及 profile 标签。
- `render_debug.rs`：四页 HUD、百分位、workload 分类、geometry/shadow/draw/fragment 统计与测试。
- `scalable_lighting.rs`、`zevy_pbr_functions.wgsl`：direct-lighting 编译期剔除。
- `scene.rs`：Map_S03B 固定 shadow A/B residency。
- `shadow_cache.rs`：共享 dynamic-caster 分类，并在 Shipping 去除 telemetry atomic 成本。
- `VR_Renderring.md`：使用方法和指标边界。
- 未修改 Map_S03B 导出资产、Level JSON、UE 插件或场景布局。

## 关键决定与禁止事项

### 已决定

- Map_S03B 是可调整的测试场景，不是产品架构边界。
- 下一项大优化由 Android VR 的四组 A/B 和分类 workload 决定；PC 数值仅用于正确性。
- `point_light_direct_lighting=false` 的有效成本隔离要求 `scalable_point_lighting=true`。
- `point_light_shadows` 当前接入 Map_S03B imported-light profile；其他关卡的统一质量策略后续再泛化。
- `Loaded caster tris × updated faces` 是保守上界，不冒充实际 rasterized primitives。
- Pipeline-statistics query 缺失时必须借助 AGI/厂商 profiler，不能从 `N/A` 推断 GPU 没有工作。

### 产品不变量

- 不按相机距离让灯光/阴影出现或消失。
- 不通过扩大 `light.range` 解决 culling/可见性。
- 不让左右眼独立选择灯、阴影、LOD、随机序列或历史。
- 不以删除 16 灯目标、明显 popping、裂缝或双眼不一致换 FPS。

### 禁止

- 禁止把 PC 运行成功写成 Android/VR 性能成功。
- 禁止把 CPU command-recording fallback 当成 GPU ms。
- 禁止把 caster triangle 上界当成 shadow pass 实际提交量。
- 禁止覆盖、reset、checkout 或重排当前未提交修改。
- 禁止在没有同路径 A/B、P95/P99 和画质误差时宣称优化有效。

## 测试结果

### 已执行并通过

- `cargo test --all-targets --message-format=short`：35 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`：通过。
- `cargo check --no-default-features --all-targets --message-format=short`：通过。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`：通过。
- PC Map_S03B 运行与截图：进程正常退出，WGSL、16 灯、shadow cache、mipmap 和 screenshot 路径均正常。
- `git diff --check`：通过；仅有 Windows 工作区 LF→CRLF 提示。

### 已知非阻塞警告

- 第三方 `bevy_mod_openxr` 的 `mismatched_lifetime_syntaxes`。
- PC cargo 同名 lib/bin PDB filename collision 警告。
- Map_S03B 部分 glTF 有 Bevy 未消费的 `TEXCOORD_2/3` 警告。

### 未执行/待用户

- 未在 Android VR 真机运行本次新 HUD。
- 未获得四组 A/B 的 P50/P95/P99、分类 GPU ms、fragment/primitive 或 thermal 数据。
- 未做 20～30 分钟 thermal soak。
- 未验证 direct/shadow A/B 四组的头显视觉截图；PC 只运行了默认 Full profile。

## 未完成步骤和下一步

1. 用户在同一 Android VR 设备、刷新率、Map_S03B 起点和移动路径，分别构建/运行：
   - `direct=false, shadows=false`；
   - `direct=true, shadows=false`；
   - `direct=false, shadows=true`；
   - `direct=true, shadows=true`。
2. 每组等待加载和 shadow warmup 后记录 Overview 的 P50/P95/P99，以及 Workload 页七类 ms/VS/Prim/Frag、static/dynamic updated faces 和 caster 上界。
3. Full profile 再比较 `max_cached_point_shadow_updates_per_frame=0/1/2/4`；每次只改一个变量。
4. 用四组差分指出 50 ms 中最大的两个模块。若 shadow projection/update 居首，优先进入 ContinuousProxy/KeyframedCrossFade；若 Main 3D/visibility 居首，优先进入 UE LOD/shadow LOD + room/PVS；若 fragment 居首，优先推进 tile 选灯、材质 tier 与 foveation。
5. Wave A 尚未补齐固定 camera-path 自动回放、cluster occupancy/overflow、精确 culling reason、upload bandwidth、OpenXR missed/reprojected frame 和 thermal/频率；这些继续保留在路线中。

### 唯一明确的下一步

先取得 Map_S03B 四组 VR A/B 的 Overview 与 Full-frame Workload 数据；拿到数据后立即选择前两大成本模块并进入下一轮算法优化，同时保持 16 灯、无距离 popping 和双眼一致。

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Checkpoints\2026-07-20-wave-a-telemetry-implementation.md`
4. `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md`，重点 19、20.2、20.9、20.16 节
5. `G:\zevy_engine\zevy_engine\src\config.rs`
6. `G:\zevy_engine\zevy_engine\src\render_debug.rs`
7. `G:\zevy_engine\zevy_engine\src\scalable_lighting.rs`
8. `G:\zevy_engine\zevy_engine\src\shaders\zevy_pbr_functions.wgsl`
9. `G:\zevy_engine\zevy_engine\src\scene.rs`
10. `G:\zevy_engine\zevy_engine\src\shadow_cache.rs`
11. 实际 `git status --short`、`git diff`、`git diff --cached`、分支与 HEAD
