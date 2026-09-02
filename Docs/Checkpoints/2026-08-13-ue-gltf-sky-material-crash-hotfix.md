# 任务检查点：UE 5.5 glTF sky material 导出崩溃 hotfix

## 元数据

- 更新时间：2026-08-13 16:03，Asia/Shanghai
- 状态：已完成（PC 导出与 Zevy runtime 验证）；未做 Android/VR 视觉验收
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ 0aa98d12f554ef8e92323b4b3b962641797b5087`
- 恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

- 最终目标：在不修改安装版 UE、不使用 Map/Actor 名称特判、也不把降质隐藏起来的前提下，让 `Map_S02VR.umap` 可重复导出为 Zevy split level。
- 完成标准：UE 插件编译；正常 SM6 commandlet 完整退出；70 个非天空资产、1299 个实体及全部外部依赖存在；清单显式记录 1 个 omitted sky Actor/component；Zevy importer 能递归加载并实例化全部 scene；第二个独立 fixture 覆盖 sky omission；旧 monolithic `.glb` 路径同样通过。

## 根因与已证伪方案

- [PC 事实] 两次 Editor 导出都在 `SM_S02VR_Ground_A` 成功后的下一资产崩溃，RHI breadcrumb 为 `DrawTileMesh -> CanvasFlush -> FRDGBuilder::Execute`；失败输出的下一目录是空的 `SM_SkySphere_0e3ddf6d`。
- [PC 事实] UE 5.5.4 glTF exporter 通过 `MaterialBaking` 调用 `FRendererModule::DrawTileMesh`。该 pass 的 RDG 参数未提供相关全局 static uniform buffer，`M_SimpleSkyDome` 烘焙时缺少 `VirtualShadowMap` slot 9 并触发 fatal。
- [失败实验] `-FeatureLevelES31` 不是 workaround：首个普通材质在同一 `DrawTileMesh` breadcrumb 以 pixel slot 2 missing uniform buffer 崩溃。
- [决定] 不修改 `F:\Program Files\Epic Games\UE_5.5`，也不以关闭 VSM/切低 feature level/Actor 名称过滤作为产品修复。

## 已完成内容

- [实现] `ZevyLevelExporterModule.cpp` 通过 base `UMaterial::bIsSky` 通用识别 sky-material static-mesh component。
- [实现] split export 不把纯 sky Actor 放入 asset/entity 表；混合 Actor 和 LevelInstance 导出期间只临时隐藏 sky component，随后恢复原 `bHiddenInGame`。
- [实现] monolithic export 使用相同过滤；每次 glTF 调用前记录 asset、Actor、selected Actor 数和 omitted component 数。
- [实现] schema 1/2 content 新增 `omitted_sky_material_actors` 和 `omitted_sky_material_components`；diagnostics 写明 Actor/component 与 omission 原因，不把天空丢失伪装为完整导出。
- [实现] `-GenerateFixture` 新增 transient `bIsSky` sphere，作为与 Map_S02VR 无关的回归夹具；README 已记录行为与未来 Zevy sky/environment metadata 缺口。
- [PC 产物] 正式 manifest：`zevy_engine/assets/levels/Map_S02VR/Map_S02VR.zevy-level.json`（assets 目录被 `.gitignore` 忽略），639 files、532,167,319 bytes；manifest SHA-256 `FD709215682F66E5E7CDC3A50D03A2C1C5E0D175FD342C10A136F21EA18B29D6`。

## 当前文件和修改状态

- hotfix 源码/文档：
  - `M ue_project/Plugins/ZevyLevelExporter/Source/ZevyLevelExporter/Private/ZevyLevelExporterModule.cpp`
  - `M ue_project/Plugins/ZevyLevelExporter/Source/ZevyLevelExporter/Private/ZevyLevelExportCommandlet.cpp`
  - `M ue_project/Plugins/ZevyLevelExporter/README.md`
  - `M Docs/Checkpoints/CURRENT.md`
  - `?? Docs/Checkpoints/2026-08-13-ue-gltf-sky-material-crash-hotfix.md`
- 正式 assets 与编译 DLL 均为 ignored 产物，未进入 `git status`。
- 工作树另有大量 P2/P3 连续阶段的既有未提交修改；本 hotfix 没有覆盖、暂存或提交它们。分支和 HEAD 均未变化，无 staged 文件。

## 测试结果

- [PC 通过] `Build.bat zevy_ueEditor Win64 Development ...`，修改后的两个 C++ translation unit 编译、DLL 链接成功。
- [PC 通过] 独立 Map_S02VR SM6 导出：70/70 assets、1299 entities，commandlet clean exit 0；越过原 Ground 后 fatal 位置。
- [PC 通过] 正式 Map_S02VR SM6 导出：exit 0；schema 2、70 assets、1299 entities、omitted sky 1/1、1 warning、0 error。
- [PC 通过] 自写静态审计：70 个 manifest scene 全存在；所有 glTF external buffer/image URI 全存在；0 missing scene、0 missing URI。
- [PC 通过] `cargo run --offline --bin validate_zevy_level -- levels/Map_S02VR/Map_S02VR.zevy-level.json`：70 assets 递归加载，1299/1299 scene instances，5373 ECS entities、1476 meshes、92 materials，hierarchy/local transforms `ok`。
- [PC 通过] split `-GenerateFixture`：6 个正常资产成功，transient sky sphere omitted，1 warning/0 error。
- [PC 通过] monolithic `.glb` `-GenerateFixture`：omitted sky component=1，GLB 和 schema-1 manifest 成功写出，exit 0。
- [PC 通过] `git diff --check`。
- [未执行] Android/VR 视觉验收；天空本来就未被 Zevy renderer 语义化，需未来 environment metadata 实现后另测。
- [无关警告] Zevy validator 仅报告既有 vendored `bevy_mod_openxr` mismatched lifetime syntax。

## 关键决定、风险和下一步

- Sky material 是 view/world dependent 的 UE 环境语义，不作为普通 glTF PBR mesh 假装导出； omission 必须可见、可计数、可回归。
- 一个 static-mesh component 只要任一 slot 使用 `bIsSky`，当前就省略整个 component，避免 section 级部分导出掩盖语义错误；未来若需要混合 slot，必须实现 section 级过滤或 Zevy environment schema。
- 正式输出中保留了上次 fatal 遗留的未引用空目录 `assets/SM_SkySphere_0e3ddf6d`（0 files）；删除命令被本机策略拒绝。它不在 manifest 中，不影响 importer。
- 未修改 UE 安装目录，未提交、未暂存。
- hotfix 无剩余必做步骤。项目唯一下一步恢复为 `CURRENT.md` 中 P3：设备恢复后重建/安装最新 profiling APK，取得 foveation capability 矩阵和 Forward/Deferred 真机成本，再实现 ReducedRate render node。

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. 本历史快照
4. `G:\zevy_engine\ue_project\Plugins\ZevyLevelExporter\Source\ZevyLevelExporter\Private\ZevyLevelExporterModule.cpp`
5. `G:\zevy_engine\ue_project\Plugins\ZevyLevelExporter\Source\ZevyLevelExporter\Private\ZevyLevelExportCommandlet.cpp`
6. `G:\zevy_engine\ue_project\Plugins\ZevyLevelExporter\README.md`
7. 实际 branch/HEAD、`git status --short`、正式 manifest 与最新测试日志
