# 当前任务检查点：P3 Reduced-Rate Local Lighting substrate（Map_S02VR hotfix 已完成）

## 元数据

- 更新时间：2026-08-13 16:03，Asia/Shanghai
- 状态：P3 进行中；Map_S02VR UE 导出 hotfix 已完成并通过 PC/runtime 验证
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ 0aa98d12f554ef8e92323b4b3b962641797b5087`
- 上一阶段：`Docs/Checkpoints/2026-07-24-shadow-motion-lab-pico-cost-decomposition.md`
- 当前设计：`Docs/Design/Reduced_Rate_Local_Lighting.md`
- 最新已完成阶段：`Docs/Checkpoints/2026-08-13-ue-gltf-sky-material-crash-hotfix.md`

## 当前紧急任务：Map_S02VR UE 5.5 glTF 导出崩溃

- 最终目标：在不引入 Map/Actor/固定坐标特判、不静默降质的前提下，让 `Map_S02VR.umap` 可重复导出为 Zevy split manifest，并把无法直接移植的 UE sky 语义显式记录。
- 完成标准：插件编译通过；正常 SM6 commandlet 完整退出；70 个非天空资产及 1299 个实体写入清单；所有资产引用存在；清单记录 1 个 omitted sky Actor/component 和具体诊断；不得改动安装在 `F:` 的 UE 引擎。
- [PC 事实] 两次 Editor 导出均在 `SM_S02VR_Ground_A` 成功后的下一资产崩溃，RHI breadcrumb 为 `DrawTileMesh -> CanvasFlush -> FRDGBuilder::Execute`，缺少 `VirtualShadowMap` static UB；失败输出中的下一目录为无内容的 `SM_SkySphere_0e3ddf6d`。
- [PC 事实] UE 5.5.4 源码显示 glTF `MaterialBaking` 调用 `FRendererModule::DrawTileMesh`，该 RDG pass 未声明 `VirtualShadowMap` UB；`M_SimpleSkyDome` 为触发材质。强制 `-FeatureLevelES31` 在首个普通材质以另一 missing UB 同路径崩溃，已证伪“切低 feature level/关 VSM”方案。
- [实现，未提交] `ZevyLevelExporterModule.cpp` 按 `UMaterial::bIsSky` 通用识别 sky-material component；纯 sky Actor 不进入 split 资产表；混合 Actor/LevelInstance 导出时临时隐藏相关 component 并恢复；monolithic 路径同样过滤；导出前记录资产/Actor；manifest 新增 omitted sky 计数和 warning。`-GenerateFixture` 已加入 transient sky sphere 回归夹具，`README.md` 已记录语义限制。
- [PC 已验证] 插件编译；独立与正式 Map_S02VR SM6 导出均 exit 0；正式结果为 70 assets、1299 entities、omitted sky 1/1、1 warning/0 error、0 missing scene/URI。Zevy validator 加载 70 assets 并实例化 1299/1299 scenes，hierarchy/local transforms `ok`。split 与 monolithic 独立 fixture 都验证 sky omission；详见最新历史快照。
- 当前 hotfix 文件：`M ZevyLevelExporterModule.cpp`、`M ZevyLevelExportCommandlet.cpp`、`M README.md`、本检查点和新历史快照；其余 P2/P3 工作树修改均为既有用户/连续阶段状态，禁止覆盖、回退或混入 hotfix 判断。正式 Map_S02VR 产物和插件 DLL 均被 `.gitignore` 忽略。
- hotfix 无剩余必做步骤。唯一下一步恢复为 P3：设备恢复后重建并安装最新 profiling APK，取得 capability 矩阵和 Forward/Deferred 真机成本，再写 ReducedRate render node。

## 最终目标和当前阶段完成标准

最终目标仍是建立面向 VR 一体机的、高性能、现代、支持大量动态灯光与动态阴影的 Zevy renderer。当前真机证据表明，仅压低每片元灯样本 $K$ 不足以达到 72 Hz；P3 必须建立能压低昂贵光照片元数 $P$ 的可回退架构。

当前阶段完成标准：

1. ForwardReference 保持默认和画质 fallback，不改变灯 range、residency、双眼状态或 motion policy。
2. 建立同一 StandardMaterial/灯光/阴影 shader 的 full-resolution DeferredReference，并在 PC/Pico 验证正确性与成本。
3. 明确区分 OpenXR runtime available、app enabled、swapchain 实际 FDM 和 PICO system policy；能力探针不得写持久属性。
4. 在 Deferred substrate 上实现首个 half/quarter-rate local direct-light buffer 与可视化 raw 输出，证明实际执行量约随 $rP$ 缩放。
5. 加 depth/normal/material-ID edge-aware reconstruction；转头、灯交界、阴影边缘、薄几何和运动 caster 不得出现块状亮度、漏光、漂浮或左右眼不一致。
6. Pico 同条件 A/B 的 GPU P95 至少优于 ForwardReference 25%，否则触发 kill criterion，改走 tile-local/subpass、quad/subgroup 或 backend FDM。

成本模型：

\[
C_{reduced}\approx PC_{gbuffer}+rPK(C_{BRDF}+C_{visibility})+PC_{reconstruct}+C_{attachment}+C_{fixed}.
\]

## 已完成内容

### [上一阶段，已实现/Android-VR 验证，未提交]

- P2 sparse SlowMoving cross-fade、DynamicOverlay、ShadowMotionLab 16/32/64 与四档/K 下界成本分解。
- 16 灯 full GPU 约 52.07 ms；固定 4/2/1 个完整 shadowed samples 后约 51.55/41.36/31.49 ms，证实必须同时攻击 $K$ 与 $P$。
- `scripts/profile_shadow_motion_lab.ps1` 已支持 repeatable cold-start profile。
- 默认 XR hand debug gizmo 已移除。

### [本阶段已实现]

- 新增公开 `LocalLightingPipeline::{Forward, DeferredReference}` 与 `RenderQualityConfig.local_lighting_pipeline`；默认仍为 Forward。
- DeferredReference 自动切换 Bevy default opaque renderer、给 3D camera 增加 G-buffer prepass，并显式使用有效 MSAA 1x。
- 用 `ZevyDeferredLightingCamera` 记录 camera 之前的 prepass 状态；切回 Forward 只撤销 Zevy 自己添加的组件。
- profiling-only Android property `debug.zevy.local_lighting=forward|deferred`，HUD 显示实际 pipeline；Shipping 不读取该 override。
- profiler script 新增 `-LightingPipeline forward|deferred`。
- vendored Bevy deferred fullscreen position 增加 `@invariant`，消除 Equal depth comparison 的跨 GPU 精度风险。
- vendored OpenXR 新增独立 `OxrAvailableExtensions` resource，不再把 advertised 与 enabled 混为一谈。
- 新增只读 Android foveation capability log：FB swapchain/foveation/config/vulkan、META eye-tracked 的 available/enabled 矩阵，以及 vendor-opaque PICO property；不写系统属性，不启用新扩展。
- 新增 `Docs/Design/Reduced_Rate_Local_Lighting.md`，记录数学模型、重建约束、实验顺序和 kill criteria。

### [PC 已验证]

- 临时将默认切到 DeferredReference，实际运行 `shadow-motion-16` 两次并恢复 Forward。
- 16 灯、96 resident shadow views、4 DynamicOverlay caster、4/4 SlowMoving transitions 正常；截图：`target/shadow_motion_lab/pc_16_deferred_invariant.png`。
- shader/runtime 无 panic/wgpu validation failure；修复后不再出现 fullscreen position invariant warning。
- 画面、点灯和阴影结构正常；该截图只证明正确性，不代表真机性能。

### [Android 状态]

- foveation 探针版 release profiling APK 已完成构建、4K alignment 和签名。
- 安装尝试失败：ADB 找不到 `PA9410MGJ9260457G`；已立即通知用户，未自行排查设备。
- 该 APK 构建发生在 DeferredReference 最终代码之前；设备恢复后必须重建最新 APK，不能把旧包当 P3 验证包。
- 旧只读日志曾看到 `persist.pvr.foveation.level=12` 和 system event level 12；这不是 Zevy swapchain 使用 FDM 的证明。

## 当前文件和修改状态

- 当前分支 `main`，HEAD 未变化；无 staged 文件，无本阶段 commit。
- 工作树同时包含 P2、ShadowMotionLab 和 P3 的连续未提交修改，不得拆错、覆盖或回退。
- P3 新增/修改重点：
  - `Docs/Design/Reduced_Rate_Local_Lighting.md`
  - `Docs/Design/Shadow_Motion_Lab.md`
  - `Docs/render_debug.md`
  - `zevy_engine/docs/VR_Renderring.md`
  - `zevy_engine/src/config.rs`
  - `zevy_engine/src/app.rs`
  - `zevy_engine/src/lib.rs`
  - `zevy_engine/src/platform.rs`
  - `zevy_engine/src/render_debug.rs`
  - `zevy_engine/scripts/profile_shadow_motion_lab.ps1`
  - `zevy_engine/third_party/crates/bevy_mod_openxr-0.3.0/src/openxr/{exts,init}.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/deferred/deferred_lighting.wgsl`
- `zevy_engine/src/config.rs` 的 `xr_render_scale: 1.0` 是用户明确保留的配置；当前 `local_lighting_pipeline` 已恢复为 `Forward`。
- ignored 工件：`target/shadow_motion_lab/*.png` 与 `target/release/apk/zevy_engine.apk`。

## 关键决定、产品不变量和禁止事项

- DeferredReference 是 substrate/cost reference，不是 reduced-rate 优化结果，不得写成已提升性能。
- 不统一降低 render scale 代替局部光照结构优化；目标是只降低昂贵 local-light work 的空间采样率。
- raw 2×2/4×4、screen supercluster 或 world-reservoir block 不得直接输出到双眼；必须 edge-aware reconstruct，并保留低置信度 full-rate fallback。
- 两眼共享灯 ID、shadow residency、随机 epoch、质量环和历史调度；每眼使用自己的 depth/world position/radiance。
- runtime advertised extension、instance enabled extension、Vulkan device feature、swapchain FDM attachment和 vendor FFR 策略必须分开记录。
- 不写 `persist.pvr.foveation.level`；若最终需要修改设备持久策略，必须先获得明确授权并建立恢复值。
- Map_S03B/ShadowMotionLab 只作为 fixture；产品路径不查询 Level/Actor 名称。
- 灯物理 range 与相机可见/驻留分离，不按相机距离突然关闭灯/影。
- 未经用户明确要求不提交。设备安装失败立即通知用户，设备可用性由用户处理。

## 测试结果

### 已执行

- `cargo fmt --all -- --check`：最终工作树通过。
- `cargo test --lib`：75/75 通过；包含 Deferred camera prepass 状态恢复测试。
- `cargo check --target aarch64-linux-android`：通过。
- `cargo check --no-default-features --target aarch64-linux-android`：最终工作树通过，无 Zevy 新 warning。
- `scripts/profile_shadow_motion_lab.ps1`：PowerShell parser 通过。
- PC DeferredReference 两次实际启动/截图：通过；第二次确认 `@invariant` warning 消失。
- foveation probe profiling APK：构建、alignment、签名通过；安装失败（设备未找到），未做 Android runtime 验证。
- 既有无关警告：vendored `bevy_mod_openxr` mismatched lifetime syntax；Cargo bin/lib PDB filename collision。

### 未执行

- 最新 P3 源码的 release APK 重建、安装。
- Pico capability log、Forward vs Deferred 四档 A/B、左右眼视觉与 GPU P95。
- ReducedRateDeferred pass/reconstruction（尚未实现）。
- GPU capture、误差图、固定相机路径和 thermal soak。

### 设备残留风险

设备断开前 profiling properties 不是默认值；最后运行过 direct-only/K=1 实验。设备恢复后必须先显式设置本次所需矩阵，阶段结束再清空 `debug.zevy.level`、`hud_page`、`point_direct`、`point_shadows`、`local_lighting`、`exact_lights`、`world_reservoir`、`cluster_preselection`、`hero_samples`、`tail_samples`。

## 未完成步骤、风险和唯一下一步

1. 设备恢复后重建最新 profiling APK；安装失败立即通知用户。
2. 冷启动读取 foveation available/enabled/system-policy 三层日志，不能仅依据 property 12 下结论。
3. 使用同一最新 APK、16 灯、相同频率/热状态分别测 Forward/Deferred 的 geometry/direct/shadow/full。
4. 根据 Deferred 固定成本决定 ReducedRate pass 是基于普通 texture + bilateral composite，还是直接 fork tile-local/subpass/backend FDM。
5. 实现 quarter-rate diffuse local-light raw buffer，先证明 $rP$，再加入重建。

唯一明确下一步：**完成最终 clean checks；设备恢复后重建并安装最新 APK，先取得 capability 矩阵和 Forward/Deferred 真机成本，再写 ReducedRate render node。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Design\Reduced_Rate_Local_Lighting.md`
4. `G:\zevy_engine\Docs\Design\Shadow_Motion_Lab.md`
5. `G:\zevy_engine\zevy_engine\src\config.rs`
6. `G:\zevy_engine\zevy_engine\src\app.rs`
7. `G:\zevy_engine\zevy_engine\src\platform.rs`
8. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_mod_openxr-0.3.0\src\openxr\{exts,init}.rs`
9. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\src\deferred\deferred_lighting.wgsl`
10. 实际 branch/HEAD、`git status --short`、`git diff --check` 与最新测试输出。
