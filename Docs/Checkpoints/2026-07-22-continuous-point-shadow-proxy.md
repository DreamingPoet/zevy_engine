# 阶段检查点：连续 PointLight 阴影投影代理 P1

## 元数据

- 完成时间：2026-07-22 14:52，Asia/Shanghai
- 分支 / 起始 HEAD：`main @ ed9f4647c9389f114176c2c9fa3fb2fa6bbe5817`
- 阶段状态：第一版实现、自动化、PC/Android 运行、PICO 静态 A/B、最终 APK 安装与启动确认完成；佩戴运动视觉和 thermal 待验
- 提交状态：未暂存、未提交

## 最终目标与本阶段标准

目标是让 Map_S03B 的 16 个 Movable 蜡烛投影逐帧连续变化，却不再以 8 Hz 真实移动每盏 PointLight 并
重画六个 cubemap face；2 个 Static 灯继续完全不动画。必须保持 18 灯的距离无关 shadow eligibility、动态 caster
overlay、双眼一致性、exact-8 视觉基线和可回退 reference path。

## 已完成

- [设计] 建立 \(N_{face/s}=L f_s F\) 成本模型；当前旧路径为 768 face/s。
- [实现] fork 增加 `PointLightShadowMapJitter`，CPU 计算 5 mm offset，0.25 mm 精度压缩进既有 flags。
- [实现] PBR 从虚拟 origin 重投影 nominal cubemap；无新 buffer/binding/texture/sample/fragment sin。
- [实现] 默认 Config 开启代理，scale 可调；关闭后恢复真实重画。
- [实现] Static 灯无代理；dynamic overlay 保持独立；HUD 显示当前模式。
- [PC] Map_S03B 18 灯运行成功，Naga/wgpu shader 编译成功，warmup 后 `108 resident / 0 draw / 108 reuse`。
- [Android/VR 静态] PICO on-off-on-off A/B 可逆；proxy 平均相对 real-redraw 降低约 6.5 ms P50（16%）、
  11.1 ms P95（23%）和 10.9 ms P99（22%）。同 resident 样本为 `60/0/60` 对 `60/30/30`。
- [遥测] 18 灯是主世界 shadow eligibility；当前构图实际 resident 为 10 灯/60 faces，由 RenderWorld
  `ViewVisibility` 抽取决定，二者已在日志和文档中分开。
- [文档] 数学、ABI、A/B、kill criterion 和未验证项已写入设计与 VR 路线文档。

## 修改文件

- `Docs/Design/Continuous_Point_Shadow_Proxy.md`
- `Docs/Checkpoints/CURRENT.md`
- 本文件
- `zevy_engine/docs/VR_Renderring.md`
- `zevy_engine/src/config.rs`
- `zevy_engine/src/render_debug.rs`
- `zevy_engine/src/scalable_lighting.rs`
- `zevy_engine/src/scene.rs`
- `zevy_engine/src/shaders/zevy_pbr_functions.wgsl`
- `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/lib.rs`
- `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/light/mod.rs`
- `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/light/point_light.rs`
- `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs`
- `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/mesh_view_types.wgsl`

任务开始时工作树干净，以上均为本阶段改动；无已知用户独立修改。截图在 ignored `target` 下。

## 测试

- `cargo fmt --all`：通过。
- `cargo test --all-targets`：46 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`：通过。
- `cargo check --no-default-features --all-targets --message-format=short`：通过。
- release+RenderDebug APK：构建、zipalign、v3 debug 签名验证通过；737,200,181 bytes，2026-07-22 14:48:06。
- PC Map_S03B 自动截图：通过；18 灯，`108/0/108` cache evidence，静态画面无新规则斑块。
- `git diff --check`：通过，仅 LF→CRLF 提示。
- 初次签名冲突已立即通知；用户授权后完整卸载旧包，新 APK 安装和 OpenXR/Vulkan 启动成功。
- 最终遥测措辞修正版 APK 已覆盖安装并冷启动；日志确认 18/18 eligibility、proxy 1.00x、Map_S03B 和
  PICO 4 Ultra Enterprise HMD 正常，无 panic。尚未做佩戴运动视觉或 thermal soak。

## 关键决定与禁止事项

- 这是误差受限的感知代理，不是几何精确移动光源；Hero 保留真实重画。
- 不扩大物理 range，不按相机距离开关灯/阴影，不制造左右眼不同 offset。
- Static 永不动画；dynamic caster overlay 不被冻结。
- Android VR 若发现漏光，先做 scale 单变量 A/B；失败则转双快照/重建，不隐藏问题。

## 下一步

用户佩戴头显沿灯光交界处转头和移动，确认代理投影连续、无明显漏光/漂浮/双眼 mismatch。视觉通过后再做
固定路径 AGI capture 与 20～30 分钟 thermal soak，并为 current-view resident 集合设计 hysteresis/prewarm。
