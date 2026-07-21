# Zevy 项目 Agent 工作准则

本文件适用于 `G:\zevy_engine` 下的全部代码、文档、UE 导出器、Bevy/wgpu 渲染器、OpenXR 集成、Android 工具与实验分支。

## 项目使命：唯一生存法则

Zevy 的核心目标是：

> **建立一个面向 VR 一体机的、高性能的、现代的、支持大量动态灯光与动态阴影的渲染引擎。**

这是项目生存的唯一法则：不以守住现有框架、插件边界或传统做法为成功，而以在目标移动硬件上持续突破性能、画质、稳定性与灯光规模的边界为成功。必须同时在数学模型、渲染算法、GPU 架构和工程实现上寻求突破。

## 技术路径不设禁区

- Bevy、wgpu、Naga、`bevy_mod_openxr`、`bevy_mod_xr`、Vulkan backend、shader、render graph、PBR、shadow pipeline、资产格式和 UE 导出流程都不是不可修改的黑盒。
- 允许 fork、vendor、patch、选择性回移上游代码、升级依赖、替换模块，或建立 Zevy 专用 renderer。若插件层限制目标，就修改引擎层；若引擎层限制目标，就修改 backend 或绕过它。
- 不得因为“Bevy 原来不是这样设计的”“社区通常不这样做”“迁移工作量大”或“现有 API 没暴露”而提前否决一个高收益方向。
- 允许研究和原型化 Multiview、Cyclopean rendering、GPU-driven culling、tile/froxel light selection、stochastic/reservoir lighting、稀疏 shadow pages、dual-paraboloid/octahedral shadow、software/hardware ray visibility、tile-local deferred、foveation、重建和其他非传统方案。
- 同时维护保守参考路径和激进实验路径；兼容性 fallback 是验证工具，不是束缚主架构的理由。

“不择手段”在本项目中指**技术思想和代码层级不设禁区**，不是忽略安全、用户数据、平台规则、许可证、可回退性或目标设备稳定性。破坏性操作、外部系统变更和许可证风险仍必须遵守授权与工程规范。

## 每次优化必须遵守的方法

1. **先写数学模型。** 明确像素数、view 数、灯数、shadow face 数、更新频率、带宽和帧预算如何缩放；区分 \(O(N)\)、\(O(PN)\)、\(O(TN+PK)\) 与固定成本。
2. **再建立可证伪实验。** 在固定 Map、固定相机路径、固定设备状态下做单变量 A/B，记录 CPU、GPU、P50/P95/P99、fragment、draw、shadow redraw、温度和频率。
3. **以目标 VR 一体机裁决。** PC 结果只用于调试正确性；Android 真机 GPU capture 与 20～30 分钟 thermal soak 才能决定方案是否胜出。
4. **区分事实和假设。** 文档必须标记已实现、已在 PC 验证、已在 Android 验证、设计假设和失败实验，不能把理论收益写成实测收益。
5. **优先结构性收益。** 先做到“不提交不可见工作”：room/PVS、frustum、LOD/HLOD、occlusion、shadow-caster culling；再消除双眼重复、逐片元全扫描、每灯六面重复、过多 draw/bind、无效 pass/attachment 往返和静态内容重画，最后才微调常数或统一降低画质。
6. **实验必须可比较、可回退。** 使用 feature/config/独立分支保留 reference path；保留截图、GPU capture、误差图、benchmark 与 kill criterion。
7. **失败也要沉淀。** 记录失败原因、设备、驱动、成本曲线与画质问题，防止后续 Agent 重复同一条无效路线。

## 产品正确性不变量

除非任务明确要求做 A/B，优化不得偷偷破坏以下行为：

- 灯光的物理照射范围与相机可见距离分离；不得靠扩大 `light.range` 掩盖剔除问题。
- 已启用的灯光和阴影不得因相机靠近/远离而突然出现或消失。
- 左右眼必须共享可共享的灯光、阴影、LOD、随机样本和历史状态，避免 binocular mismatch。
- Hero/交互灯的响应优先于装饰 Tail 灯；低优先级只能平滑降低频率、分辨率或近似质量。
- 静态内容不应无理由每帧重画；动态内容不应无理由使整个静态缓存失效。
- 几何优化必须由 projected error、silhouette、屏幕面积和实机成本驱动；不得只为降低三角形数字而制造双眼 LOD 不一致、裂缝、明显 popping 或更差的 HLOD culling 粒度。
- 平均 FPS 不能掩盖 P95/P99、reprojection、热降频或瞬时视觉跳变。

## 判断“突破”的标准

一个改动至少满足以下一项，才可称为突破：

- 改变成本阶数或使灯数/物体数增长时成本斜率显著变平；
- 消除双眼、灯间、帧间或静态/动态之间的大块重复工作；
- 在相同画质与灯数下给目标机带来可重复的显著 GPU/CPU 收益；
- 在相同帧预算内显著增加动态灯数、阴影质量或热稳定时间；
- 用可控、双眼稳定且无突变的感知误差替代昂贵的物理精确计算。

单纯把参数调低、隐藏远灯、减少灯数或让阴影突然关闭，只能是诊断/应急 trade-off，不能作为项目核心创新。

## 任务检查点与上下文恢复（强制）

编程、架构、资产管线和跨文件开发任务必须使用 `Docs/Checkpoints` 保存可从磁盘恢复的状态，不能只依赖聊天上下文。

### 权威文件

- `Docs/Checkpoints/CURRENT.md`：当前任务唯一的最新恢复入口。开始或恢复开发前必须读取。
- `Docs/Checkpoints/YYYY-MM-DD-<stage>.md`：阶段完成后的历史快照，完成后原则上不再改写。
- `Docs/Checkpoints/README.md`：格式、更新时机和模板。

### 何时必须更新

- 一个计划阶段完成并准备进入下一阶段时；
- 长阶段中完成了会影响后续判断的中间里程碑时；
- 即将暂停、阻塞、交接、切换对话或进行大规模迁移/实验前；
- 测试结论、关键约束、工作区文件状态或下一步发生实质变化时。

阶段完成时同时更新 `CURRENT.md` 并新增历史快照；阶段中途只需更新 `CURRENT.md`。检查点不是逐工具调用日志，应简洁但足以让完全没有聊天历史的 Agent 继续工作。

### 检查点必备内容

1. 最终目标和完成标准；
2. 已完成内容，并标明实现、PC 验证、Android/VR 验证或仅设计；
3. 当前分支、HEAD、文件和修改/暂存状态，明确区分已有用户改动与本阶段改动；
4. 关键决定、产品不变量和禁止事项；
5. 实际执行过的测试、结果、设备与未执行项；不得把“预计通过”写成“已通过”；
6. 未完成步骤、依赖、风险和唯一明确的下一步；
7. 恢复时首先应读取的相关文件。

### 压缩、重启或新对话后的恢复协议

1. 先完整读取根 `AGENTS.md` 与 `Docs/Checkpoints/CURRENT.md`；
2. 再读取检查点引用的规格/代码，并执行 `git status`、必要的 `git diff` 和当前分支/HEAD 检查；
3. 在第一次 commentary 中先复述一次：最终目标、已完成阶段、当前文件状态、关键约束、测试状态和下一步；
4. 对照磁盘检查是否遗漏关键约束或出现冲突，再开始修改；
5. 若信息冲突，优先级为：实际文件与 Git/测试结果 > `CURRENT.md` > 历史检查点 > 压缩后的聊天摘要；不得静默猜测；
6. 不重复已经完成并验证的工作，不因上下文缩短而擅自缩小目标、丢弃约束或退回保守架构。

## 当前最高优先级（2026-07-20）

1. 把 Map_S03B 的约 50 ms/frame 分解为 main/shadow geometry、direct lighting、shadow update/sampling、Multi-Pass、fragment/bandwidth、draw/CPU submission、streaming 与 thermal 成本。
2. 补齐 main/shadow triangles、microtriangle、draw/batch、PVS/frustum/HZB culling、overdraw、vertex/index bytes、upload 与 thermal telemetry。
3. 升级 UE 导出管线：LOD、shadow LOD、room/portal/PVS、HLOD/spatial chunk、实例引用、GPU 友好索引/顶点顺序、量化与离线审计报告。
4. 在运行时实现 Cyclopean PVS/frustum、稳定 projected-error LOD、shadow-caster culling，并验证远近场景的三角形和 draw 确实下降。
5. 为 16 盏蜡烛实现不依赖频繁 cubemap redraw 的连续阴影代理，消除约 400 ms 的投影阶梯。
6. 并行比较 Bevy 0.19 升级、上游优化回移和 Zevy renderer fork；实现 XR Multiview、GPU scene、HZB、GPU culling、indirect/MDI、instancing 和 dirty uploads。
7. Zevy `bevy_pbr` fork 已证明修改引擎层可显著降低选灯成本，但 2×2 screen supercluster 被 VR 运动测试证伪为转头亮度块，未经重建的 world reservoir 又被证伪为阴影斑块；当前 `exact_lights=8` 是 Map_S03B 已验证视觉基线。下一步必须为 8 灯以上 overflow 实现双眼共享 reservoir/Top-K、edge-aware reconstruction 与固定路径 16→32→64 灯斜率验证，不能把 raw stochastic shadow 直接输出到眼睛。并行把 96 个固定 shadow views 演进为提前 cache reject、稀疏 dynamic pages 和 GPU-ms/误差驱动调度。
8. 接入 foveation/VRS、移动 tile attachment 优化、material tier、ASTC/KTX2、资源 streaming 与 ADPF 热控制，最终达到热稳定 72 Hz，并向 90 Hz 扩展。

详细架构复盘、公式、实验顺序和验收门槛见 `zevy_engine/docs/VR_Renderring.md`。
