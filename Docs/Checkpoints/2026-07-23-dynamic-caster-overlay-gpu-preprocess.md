# 任务检查点：动态 caster overlay GPU preprocessing 修复

## 元数据

- 更新时间：2026-07-23 16:55，Asia/Shanghai
- 状态：实现与 PC 视觉验证完成；Android 构建/安装/启动完成；VR 动态视觉待用户验证；未提交
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ ed9f4647c9389f114176c2c9fa3fb2fa6bbe5817`

## 最终目标和本阶段完成标准

最终目标是面向 VR 一体机建立高性能、现代、大量动态灯光/动态阴影渲染器。当前阶段要求运动球通过通用 dynamic caster overlay 对静态墙面和地面投影，同时保持静态 shadow atlas 可复用，不以扩大灯光范围、提高亮度或 Map 特例掩盖错误。

本阶段代码和 PC 正确性已经完成；PICO 中的地面/墙面动态运动、旧影清除和双眼一致仍需用户佩戴验证。

## 根因与证据

1. overlay off 的完整 cubemap 重画能看到球影，overlay on 看不到，排除了球体几何、PBR 材质、灯光范围和接收面。
2. 动态 atlas clear-value 探针影响最终照明，证明 shader 已采样动态 atlas。
3. 动态 shadow workload 仍报告约 6.0K VS、1.6K primitives 和 0.45 ms，证明 draw 已提交。
4. 更换 atlas layer 或复用 native view 均未恢复球影，排除动态半区索引和单一 view uniform。
5. Bevy `EarlyGpuPreprocessNode` 只处理 main view 和 `ViewLightEntities` 登记的 shadow views；Zevy synthetic dynamic views 未登记，导致 draw 读取未正确生成的 GPU instance output，提交了 primitives 却没有球体真实变换的深度。

## 实现

- 在 `zevy_engine/src/shadow_overlay.rs` 中把 synthetic dynamic shadow views 注册到各 main view 的 `ViewLightEntities`。
- 新增去重 helper `register_dynamic_preprocess_views` 及单元测试。
- 把 dynamic `Shadow` phase 的 `prepare_for_new_frame` 移至 `ManageViews`，匹配 Bevy native shadow 生命周期。
- 保留正常 mesh batching；所有 atlas/view/unbatchable 临时探针已撤回。
- debug 日志新增 caster、face、visible caster-face reference 与 queued draw 计数，只在变化时输出。
- 机制完全位于通用 renderer；Map_S03B 只提供两个飞行灯、两个运动 PBR 球和灰色 floor/wall calibration receivers。

## PC 与 Android 结果

- PC 最终截图：`zevy_engine/target/render_debug/Map_S03B_dynamic_ball_shadow_fixed_clean.png`。
- 灰色地面接收板上可见运动球的紫黑色投影，证明 synthetic dynamic view 现在取得有效实例预处理结果。
- `cargo check --lib`、格式检查、5 个 shadow overlay 测试、4 个 motion harness 测试和 Android target check 均通过。
- 全部 lib 测试为 50 passed / 1 failed；唯一失败是前序配置中 exact threshold 实际 18、旧测试期望 8，与本修复无关，未静默更改。
- release/render-debug APK 已构建、签名、安装并冷启动于 PICO 设备 `PA9410MGJ9260457G`，进程 PID 10022，无 panic/shader error。
- 启动日志采样时 `should_render=false`、`LayerCnt=0`，所以没有有效 VR 视觉或性能结论。

## 文件和工作区状态

- 本阶段核心代码：`zevy_engine/src/shadow_overlay.rs`。
- 本阶段状态文件：`Docs/Checkpoints/CURRENT.md` 与本快照。
- `AGENTS.md` 是用户改动；config/HUD/scalable lighting/PBR fork/shader/continuous proxy/Map harness 等其余 dirty 文件属于前序未提交工作，全部必须保留。
- 无暂存、无提交；ignored `target` 截图和 APK 不提交。

## 关键决定和禁止事项

- 不扩大物理 `light.range`，不靠亮度掩盖缺影。
- 动态 caster 不使静态 atlas 整层每帧失效。
- cubemap 六面走同一通用注册机制，不写 floor/wall 特例。
- 保留完整 cubemap redraw 作为 correctness oracle/fallback。
- 左右眼共享灯光与阴影状态；后续需要 capture 判断 synthetic view preprocessing 是否被双眼重复触发，并演进为一次共享/Cyclopean 预处理。
- Map_S03B 只能作为测试 harness，不能决定产品算法。

## 唯一下一步

用户佩戴当前 PICO，在 Map_S03B 中确认：地面和墙面都有运动球影、影子连续跟随且旧影清除、左右眼一致。通过后进入 overlay GPU capture 与双眼 preprocessing 去重；若失败，按缺失 cubemap face/caster culling/动态 layer 清除定向修复。

## 恢复入口

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
4. `G:\zevy_engine\zevy_engine\src\scene\map_s03b_motion_test.rs`
5. 实际 Git 状态与最新测试结果
