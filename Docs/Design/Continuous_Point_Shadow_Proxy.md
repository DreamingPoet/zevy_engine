# Zevy 连续 PointLight 阴影投影代理

## 状态

- 阶段：P1 第一版已实现；Rust/Android、PC 与 PICO 静态性能 A/B 通过，等待佩戴运动视觉与 thermal 验收
- 日期：2026-07-22
- 目标场景：Map_S03B，18 个 PointLight（16 Movable、2 Static）
- 目标设备：Android VR 一体机

## 问题与成本模型

当前蜡烛投影通过低频移动 PointLight 并使持久化 cubemap 失效来变化。若可动画灯数为
\(L\)，目标更新频率为 \(f_s\)，PointLight 有 \(F=6\) 个 cubemap face，则每秒静态阴影重画数为：

\[
N_{face/s}=L f_s F
\]

Map_S03B 当前 \(L=16\)、\(f_s=8\text{ Hz}\)，所以：

\[
N_{face/s}=16\times 8\times 6=768
\]

单个 face 的近似成本为：

\[
C_{face}=C_{visibility}+C_{queue}+C_{pass}+N_{caster}c_v+R^2c_f
\]

即使 face 只有 \(128^2\)，96 个常驻 shadow view 的 visibility、queue、render-pass 与 caster
提交仍不会因为分辨率低而消失。固定“每帧最多更新几盏灯”只能重新分配成本，不能改变总成本，
而且会把连续火光变成约数百毫秒一级的投影台阶。

## 连续代理模型

对每个可动画蜡烛灯保留名义光源位置 \(p_l\) 和静态缓存深度 \(D_l\)，不再为 5 mm 火光摇摆
移动真实 PointLight。每帧只计算一个双眼共享、与相机无关的微小虚拟偏移：

\[
\delta_l(t)=\begin{bmatrix}
A_x(0.68\sin(2.1t+\phi_l)+0.32\sin(8.3t+1.73\phi_l))\\
A_y(0.72\sin(3.4t+0.83\phi_l)+0.28\sin(11.7t+1.49\phi_l))\\
A_z(0.64\sin(2.7t+1.21\phi_l)+0.36\sin(9.1t+2.07\phi_l))
\end{bmatrix}
\]

默认 \(A_x=A_y=A_z=5\text{ mm}\)。阴影接收端用虚拟原点
\(p'_l=p_l+\delta_l(t)\) 计算 cubemap lookup direction 与 compare depth，但继续读取在 \(p_l\)
生成的持久化深度。该近似的一阶角误差约为：

\[
\epsilon_\theta\approx\frac{\lVert\delta_l\rVert}{d}
\]

当接收面距灯 \(d=1.5\text{ m}\)、偏移 5 mm 时，\(\epsilon_\theta\approx0.0033\text{ rad}
=0.19^\circ\)。它不是几何精确的移动光源阴影，而是用受限、连续、双眼一致的感知误差换掉高频六面
深度重画。

## GPU 数据与复杂度

偏移由 CPU 每灯每帧计算一次，量化为三个有符号 8-bit 分量，复用 `ClusterableObject.flags`
的空闲位：

- bit 4：连续投影代理存在；
- bits 8..15：X；
- bits 16..23：Y；
- bits 24..31：Z；
- 量化步长：0.25 mm，可表达约 -32.0～+31.75 mm。

这不新增 bind group、storage buffer、纹理或 shadow sample。接收端每次 shadow lookup 只增加常数次
bit decode 和向量加法。稳定静态层的目标成本从

\[
O(L f_s F(C_{visibility}+C_{raster}))
\]

降为 warmup 后近似零重画；主画面只增加既有 shadow lookup 内的固定 \(O(PK)\) 小常数。
动态 caster overlay 不被冻结，继续按真实 caster 变化更新。

## 产品不变量

- 不改变物理 `PointLight.range` 来解决相机可见性。
- 不按相机距离开关灯光或阴影。
- 偏移只由游戏时间、灯的稳定 phase 和关卡配置决定，左右眼读取同一值。
- UE `mobility=static` 不创建代理，不播放强度、range、位置或投影动画。
- 真实几何移动灯、交互 Hero 灯和禁用代理的 A/B 路径仍可使用原 cubemap 重画。
- dynamic caster overlay 始终与静态代理分层，代理不得冻结动态物体阴影。

## 适用边界与通用化要求

连续虚拟原点代理只适用于位移相对遮挡物/接收面距离很小的 bounded micro-motion。它是通用 renderer
可选策略，不是 Map_S03B 或蜡烛的专属优化；当前 `animate_map_s03b_candle_lights` 只是测试 harness。
`point_shadow_proxy_sway_scale` 只用于测量误差曲线，不能由单场景结果决定全局默认。

若真实灯从缓存原点 \(p_0\) 移到 \(p(t)\)，代理位移为 \(\delta=p(t)-p_0\)。一阶方向误差随
\(\lVert\delta\rVert/d\) 线性增长，并且静态深度无法表示新出现的 disocclusion。自由飞行、快速移动、
大位移或 Hero 灯必须升级到真实 shadow update、关键帧/交叉淡化、稀疏 shadow pages 或其他动态可见性路径；
代理最多只能补充两个真实更新之间的小幅高频微动，不能替代移动光源阴影。

产品化必须移除对 Map 名称的依赖，建立通用 `ShadowMotionPolicy` 与 per-light 数据：运动类型/边界、允许
投影误差、shadow priority、最大 stale time 和 fallback。至少用第二个独立或合成场景覆盖 Static、
BoundedMicroMotion、SlowMoving/Keyframed 和 FullyDynamic/Hero 后，才能称为引擎功能。

## A/B 与可证伪标准

配置：

- `continuous_point_shadow_proxy=true`：默认候选路径；
- `continuous_point_shadow_proxy=false`：原 8 Hz / 每帧灯数预算 reference path；
- Android 调试覆盖：`debug.zevy.shadow_proxy`。

固定 Map、固定相机路径和相同设备状态下记录：

- static shadow redraw/reuse/resident；
- dynamic overlay redraw；
- GPU main/shadow pass P50/P95/P99；
- FPS、fragment、温度和频率；
- 阴影边界的连续性、漏光、漂浮、双眼不一致和头部运动稳定性。

实现门槛：

1. 自动化与 Android cross-check 通过；
2. warmup 后、静态场景中当前 RenderWorld 实际相关灯达到 `0 static redraw / resident reuse`；
   18 灯/108 faces 是主世界 eligibility 上限，不冒充每帧实际 resident 数；
3. 16 个 Movable 灯仍有逐帧投影变化，2 个 Static 灯完全不动画；
4. 不出现距离 popping、左右眼 mismatch 或新的规则斑块。

Kill criterion：若 5 mm 默认幅度在 VR 中产生稳定可见的漏光/影子脱离，先降幅并测误差曲线；若达到
可见连续性所需幅度仍不可接受，则保留该路径为低档，转入双快照 `KeyframedCrossFade` 或 shadow-term
重建，不把伪精确结果冒充产品完成。

## 第一版验证记录（2026-07-22）

- `cargo test --all-targets`：46 passed，0 failed；包含默认代理与 real-redraw fallback 的系统测试。
- `cargo check --target aarch64-linux-android`：通过。
- `cargo check --no-default-features --all-targets`：通过，shipping-like 无 HUD 编译路径未被破坏。
- 用户授权后旧包卸载、新 APK 安装和 PICO OpenXR/Vulkan 启动成功；Map_S03B、双眼 1536x1536 与代理
  shader 正常运行，无 panic。
- PC Map_S03B 运行和截图：Naga/wgpu 成功编译 WGSL；18 个 shadow-enabled PointLight 常驻；
  warmup 后 HUD `Resident/Draw/Reuse = 108/0/108`。
- PICO 静态 on-off-on-off 截图 A/B：proxy 两次 P50/P95/P99 分别为 `33.3/36.4/37.7` 与
  `33.5/37.0/39.9 ms`；real-redraw 两次为 `39.1/47.5/49.9` 与 `40.6/48.0/49.5 ms`。
  proxy 均值相对 reference 均值降低约 6.5 ms P50（16%）、11.1 ms P95（23%）、10.9 ms P99（22%）。
  同 resident 的第二组为 `60/0/60` 对 `60/30/30`，且几何与 fragment 计数基本相同。
- Android 当前视图实际 resident 为 10 灯/60 faces；18 灯/108 faces 是主世界 shadow eligibility 上限。
  源码确认 RenderWorld 仍通过 `ViewVisibility` 抽取当前相关灯，二者必须分开记录。
- 静态画面未见新的规则斑块；1.0× 佩戴运动质量已验证，2.0× 幅度标定、AGI capture 与 thermal 尚未验证。
- 最后仅修正 eligibility/resident 日志措辞并重建 APK；设备恢复后 14:48:06 最终 APK 已覆盖安装并
  冷启动，修正后的 eligibility 日志、proxy 1.00x、Map_S03B 与 OpenXR/Vulkan 路径均确认正常。
- [Android/VR 用户验证，1.0×] 阴影连续跳动，无明显运动台阶、漏光或阴影漂浮，左右眼一致；唯一不足是
  感知跳动幅度小于旧真实重画路径。当前把 scale 单变量提高到 2.0×（约 10 mm；1.5 m 处一阶角误差约
  0.38°）等待用户裁决，不改变真实灯位、物理 range、缓存 redraw 或双眼共享状态。
