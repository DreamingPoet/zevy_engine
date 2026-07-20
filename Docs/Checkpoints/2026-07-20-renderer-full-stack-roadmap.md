# 任务检查点：移动 VR 渲染器全栈路线规划

## 元数据

- 更新时间：2026-07-20 15:43，Asia/Shanghai
- 状态：本阶段已完成；下一开发阶段尚未开始
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ e230022`（`规划后续开发路线`）
- 当前恢复入口：`Docs/Checkpoints/CURRENT.md`
- 阶段快照：`Docs/Checkpoints/2026-07-20-renderer-full-stack-roadmap.md`

## 最终目标和完成标准

### 最终目标

建立面向 Android VR 一体机的高性能、现代渲染引擎，支持大量动态灯光、动态阴影、PBR、可编辑 UE Level 导入和稳定双眼输出。技术路径不受 Bevy、wgpu、OpenXR 插件或既有 renderer 边界限制。

### 产品完成标准

- Map_S03B 至少 16 盏 PointLight 的直接光与阴影可同时存在。
- 灯光物理照射范围与相机可见距离分离，不靠扩大 `light.range` 或相机距离开关阴影。
- 左右眼共享可共享的 visibility、LOD、灯光、阴影、随机样本和历史状态，无 binocular mismatch。
- UE 导出保持“每个独立资产单独导出”，保留 Actor Attach 层级、局部变换、材质、纹理、灯光参数和手动编辑能力。
- 20～30 分钟 thermal soak 后 P95 ≤ 13.89 ms（72 Hz）；支持设备向 P95 ≤ 11.11 ms（90 Hz）扩展。
- 优化必须有固定路径 A/B、GPU/CPU 证据和明确画质误差，不以隐藏灯光、距离 popping 或破坏双眼一致换性能。

### 当前阶段完成标准

- 复盘 16 灯方案的突破与保守边界。
- 将后续路线扩展为几何、可见性、draw/ECS、材质/fragment、纹理/streaming、XR、光影和热管理全栈计划。
- 将“允许修改/fork 引擎源码”和阶段检查点机制写入根 `AGENTS.md`。
- 建立可跨上下文恢复的 `CURRENT.md`、模板和历史快照。

以上当前阶段目标已完成。

## 已完成内容

### 已实现并经用户 Android/VR 观察

- Map_S03B 当前包含 16 盏 PointLight，能同时产生动态直接光与阴影。
- 灯光不再因玩家靠近才突然工作；灯光物理范围没有为相机可见距离而扩大。
- 火焰 emissive、强度变化和阴影投影变化可见。
- 无饥饿阴影调度后所有灯都会轮转更新，不再永久冻结部分灯。
- 当前实机约 20 FPS；阴影变化仍有明显阶梯感。
- Mipmap 数字验证曾确认 mip 链工作；正式导出不应保留测试数字。
- VR 调试 HUD 可用右手柄 A 显隐、B 切页。

### 已实现并有最近代码验证记录

- 正常 Clustered Forward 候选灯路径。
- 固定昂贵 shading 预算的 2 Hero + 2 importance-sampled Tail PointLight shader。
- PointLight 阴影常驻，与相机距离解耦。
- 持久化 static cubemap-array 内容缓存。
- static shadow + dynamic caster overlay 双层合成：`V = Vstatic × Vdynamic`。
- XR 相同 `(light, face)` 阴影提交去重。
- 公平 oldest-first cached projection 调度，默认最多 2 lights/frame。
- 完整 mip chain、trilinear 与 anisotropic sampler。
- PC/VR render debug HUD：FPS、triangles、draw 估算、fragment、pass、材质和 shadow telemetry。

### 已完成设计与文档

- `zevy_engine/docs/VR_Renderring.md` 已增加：
  - 16 灯里程碑与 20 FPS/2.5 Hz 阴影更新数学分析；
  - 突破性方案与保守过渡方案对照；
  - Bevy 0.19、wgpu Multiview/fork 路线；
  - 全帧成本模型；
  - UE LOD/shadow LOD/HLOD/Portal/PVS/实例/量化资产计划；
  - 运行时 PVS、Cyclopean frustum、projected LOD、HZB、shadow caster culling；
  - GPU scene、indirect/MDI、instancing、dirty upload；
  - fragment/overdraw/material tier/render pass；
  - ASTC/KTX2、geometry/texture streaming；
  - CPU、frame pacing、ADPF thermal；
  - Wave A～H 与 R0～R5 验收门槛。
- 根 `AGENTS.md` 已确立“技术路径不设禁区”的项目原则，并新增任务检查点/压缩恢复协议。
- `Docs/Checkpoints/README.md` 已建立检查点模板和证据优先级。

## 当前观测基线

- 用户 VR 实测帧率：约 20 FPS，即约 50 ms/frame。
- 当前目标：72 Hz 为 13.89 ms；90 Hz 为 11.11 ms。
- `Triangles / eye ≈ 208,989`，相机拉近/推远时基本不变，说明当前没有有效 LOD。
- Fragment invocations 曾约从 100,000 变化到 4,000,000，和帧率相关；VR 约为 PC 双眼规模。
- 16 灯、20 FPS、2 lights/frame 时，公平调度的单灯阴影更新上界约 `20×2/16 = 2.5 Hz`，完整轮转约 400 ms。
- 继续维持 16 灯 × 8 Hz 的真实移动 cubemap，在 20 FPS 下约需 7 lights/frame，即 42 faces/frame；不能只靠提高预算解决。

## 当前文件和修改状态

检查点生成前实际状态：

```text
MM AGENTS.md
M  zevy_engine/docs/VR_Renderring.md
?? Docs/Checkpoints/
branch: main
HEAD: e230022
```

解释：

- `AGENTS.md`：已有全栈优先级修改在 index 中；本阶段又在 worktree 增加检查点/压缩恢复协议。禁止恢复时直接 checkout/reset 覆盖任一层。
- `zevy_engine/docs/VR_Renderring.md`：全栈路线修改已暂存。
- `Docs/Checkpoints/`：本阶段新增，包含模板、当前检查点和历史快照，尚未跟踪。
- 当前阶段没有修改 Rust、WGSL、C++、UE 资产或 Level 数据。
- 这些修改都是用户要求的规划/流程文档；恢复时先检查实际 `git status` 和 staged/unstaged diff，不擅自暂存、取消暂存或提交。

## 关键决定与禁止事项

### 已决定

- 项目目标高于框架边界；允许升级、fork、vendor、回移或替换 Bevy/wgpu/Naga/OpenXR/Vulkan renderer。
- 优化顺序首先是“不提交不可见工作”，再消除重复工作，最后才统一降画质。
- 几何、光影、fragment、draw、带宽、streaming、CPU 和 thermal 必须使用同一全帧模型。
- PC 只验证正确性；Android VR capture、P95/P99 和 thermal soak 决定性能方案。
- 每完成一个阶段必须更新 `CURRENT.md` 并新增历史快照；中途暂停至少更新 `CURRENT.md`。

### 禁止事项

- 禁止按相机距离让已启用灯光或阴影突然出现/消失。
- 禁止用扩大 `light.range` 解决相机可见性或剔除问题。
- 禁止左右眼独立选择灯、阴影、LOD 或随机历史。
- 禁止只为了降低三角形数字制造裂缝、明显 LOD popping 或更差 HLOD culling 粒度。
- 禁止把固定参数降级、删灯或统一降分辨率称为核心突破。
- 禁止把未运行测试写成已通过。
- 禁止覆盖、reset、checkout 或重排当前 staged/unstaged 用户状态。
- 禁止上下文压缩后跳过状态复述就直接继续改代码。

## 测试结果

### 最近一次代码验证

在公平阴影调度阶段实际执行过：

- `cargo test --all-targets`：31 passed，0 failed。
- `cargo check --target aarch64-linux-android`：通过。
- 已知非阻塞 warning：第三方 `bevy_mod_openxr` 的 mismatched lifetime syntax。

### 本文档阶段验证

- `git diff --check`：通过；只有 Windows 工作区 LF→CRLF 提示。
- 检查了 Markdown 标题顺序、路线段落和关键术语。
- 未重新运行 Rust/Android/UE 构建，因为本阶段只修改 Markdown 文档。
- 未执行新的 Android GPU capture 或 VR 视觉测试。

### 用户实测仍待解决的问题

- 16 灯场景只有约 20 FPS。
- 阴影公平轮转后仍有约 400 ms 量级的阶梯感。
- 三角形不会随观察距离下降。
- 当前缺少 main/shadow triangles、microtriangle、culling reason、draw/batch、overdraw、upload 和 thermal 的完整分解。

## 未完成步骤和下一步

### 未完成路线

1. Wave A：补齐全帧 telemetry 与固定 camera path 自动 A/B。
2. Wave B：UE 导出 LOD、shadow LOD、bounds、重复资产报告；实现 `ContinuousProxy` 阴影实验。
3. Wave C：Map_S03B Room/Portal/PVS、Cyclopean frustum、稳定 projected-error LOD。
4. Wave D：Bevy 0.19/选择性回移/Zevy fork 比较，打通 Multiview、GPU scene、indirect/MDI。
5. Wave E：HZB、HLOD、instancing、material table。
6. Wave F：Cyclopean tile 选灯、稀疏 shadow pages、GPU-ms scheduler。
7. Wave G：foveation、material tiers、overdraw、tile attachment、ASTC/KTX2、streaming。
8. Wave H：ADPF/OpenXR 热稳定质量控制。

### 风险与依赖

- Android 设备对 GPU timestamp、pipeline statistics、VRS/foveation 和 multiview 的能力需要运行时枚举。
- HZB/compute 可能在移动 tiler 上因 barrier/固定成本不划算，必须保留 PVS-only A/B。
- HLOD/mesh merge 会与 culling 粒度冲突，必须按 room/spatial cell 限制。
- VR 视觉与热稳定测试需要用户在目标设备协助。

### 唯一明确的下一步

在用户授权开始下一阶段后，先实施 **Wave A：可信全帧测量**：

1. 扩展 HUD/telemetry，分开 main triangles、shadow caster triangles、updated faces、draw/batch、culling reason、microtriangle/overdraw 可获得指标、upload 与 thermal。
2. 建立 Map_S03B 固定 camera path/配置矩阵和输出报告格式。
3. 用 PC 正确性检查加 Android/VR 用户实测定位 50 ms 中最大的两个成本模块。
4. Wave A 完成后生成下一份阶段检查点，再决定 Wave B 中几何或阴影代理的先后比例。

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md`，重点第 3.2、20、21 节
4. `G:\zevy_engine\zevy_engine\src\render_debug.rs`
5. `G:\zevy_engine\zevy_engine\src\config.rs`
6. `G:\zevy_engine\zevy_engine\src\scene.rs`
7. `G:\zevy_engine\zevy_engine\src\scalable_lighting.rs`
8. `G:\zevy_engine\zevy_engine\src\shadow_cache.rs`
9. `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
10. `G:\zevy_engine\ue_project\Plugins\ZevyLevelExporter\Source\ZevyLevelExporter\Private\ZevyLevelExportCommandlet.cpp`
11. 实际 `git status --short`、`git diff`、`git diff --cached` 和分支/HEAD
