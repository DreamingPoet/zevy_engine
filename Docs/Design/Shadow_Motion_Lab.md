# Shadow Motion Policy 合成压力实验室

## 目标与非目标

本实验室是独立于 Map_S03B、Actor 名称和导入资产的 renderer 验证夹具，用于主动触发 `SlowMoving` PointLight 的稀疏双快照阴影，测量 16→32→64 灯时的成本斜率、transition 槽超订阅、快照陈旧误差和双眼一致性。

它不是产品场景，也不通过减少内容、扩大 `light.range` 或按相机距离隐藏灯光制造性能结果。实验胜出的调度机制必须留在通用 `shadow_motion_policy` / shadow pipeline；夹具只负责产生可重复输入。

## 固定变量与单变量

标准 profile 为 $N\in\{16,32,64\}$ 个 shadowed PointLight。三档共享：

- 同一 8×8 世界网格及嵌套灯集合，16 灯集合是 32 灯集合的子集，32 是 64 的子集；
- 相同房间、静态 receiver/caster、材质、相机起点和 clear color；
- $D=4$ 个固定为 `DynamicOverlay` 的运动 caster；
- 相同每灯 intensity、物理 range、shadow-map 分辨率和运动方程；
- 相同 `SlowMoving` 手动 policy、快照阈值、cross-fade 时间和 transition pool；
- 相同 XR render scale、MSAA、刷新率、设备状态和采样时长。

主实验唯一变量是 $N$。任何 exact-light threshold、dynamic-overlay、cross-fade、caster 数或 render scale 的改变都必须作为另一组命名 A/B，不能混入 16→32→64 斜率。

## 确定性空间与运动

64 个候选灯中心位于 8×8 网格。标准子集规则：

```text
16: x 为偶数且 z 为偶数
32: (x + z) 为偶数
64: 所有网格单元
```

因此 $L_{16}\subset L_{32}\subset L_{64}$，增加灯不会移动或替换低档已有灯。

第 $i$ 盏灯的物理位置为：

\[
p_i(t)=c_i+
\begin{bmatrix}
a_i\sin(\omega_i t+\phi_i)\\
b_i\sin(0.73\omega_i t+1.7\phi_i)\\
a_i\cos(0.61\omega_i t+0.37\phi_i)
\end{bmatrix}.
\]

$c_i$、$a_i$、$b_i$、$\omega_i$ 和 $\phi_i$ 只由网格 index 决定，不使用随机数和相机状态。运动幅度必须大于默认 4 cm snapshot threshold，同时保持在原始物理 range 内。手动 `SlowMoving` 是实验输入；另开实验才能评估 Automatic 分类。

动态 caster 使用相同原则：固定初始轨道、相位和速度，位置/旋转只由主世界 `Time` 决定，固定为 `ShadowCasterMotionClass::DynamicOverlay`。

## 成本模型

设每盏 PointLight 有 $F=6$ 个 cubemap face，静态 caster/receiver 集为 $S$，动态 caster 数为 $D=4$，transition 槽为 $K$，每次 transition 时间为 $T_b$，shadow face 边长为 $R$。

完整动态参考每帧近似：

\[
C_{full}(N)=O(NF(S+D)).
\]

P2 的静态快照重画、动态 overlay 和过渡采样近似：

\[
C_{P2}(N)=O(U_fFS)+O(NFD)+O(P_t),
\]

其中 $U_f$ 是该帧开始的新 transition 数，满足 $0\le U_f\le K$；$P_t$ 是当前过渡灯覆盖的片元。稳定超订阅时，槽池的理论 transition 吞吐上限为：

\[
Q_{slot}\le \frac{K}{T_b}.
\]

默认 $K=4,T_b=0.12s$ 时上限约 33.3 transitions/s。若所有灯持续请求，单灯平均服务率上界约为 $Q_{slot}/N$，所以 16/32/64 档约为 2.08/1.04/0.52 次每秒。该模型预言：固定槽数会使最大 stale error 随 $N$ 上升；P2b 必须用优先级和 deadline 控制“谁可以更陈旧”，不能假设所有灯都维持 8 Hz。

每次旧 cubemap copy 的字节量（D32）为：

\[
B_{copy}=6R^2\times4.
\]

$R=128$ 时约 0.375 MiB；每秒 copy 带宽上界约为 $Q_{slot}B_{copy}\approx12.5$ MiB/s。实际 GPU 成本还包括同步与 texture-cache 行为，必须用真机 capture 裁决。

当前 dynamic overlay 仍可能按 $O(NFD)$ 增长。若 16→32→64 的 GPU 斜率主要来自每帧 $6N$ 个动态 shadow views，而不是 snapshot redraw/copy，则实验结论应是“下一突破点为 sparse dynamic pages / caster-light overlap culling”，不能错误归因于 cross-fade。

## 启动接口

Desktop：

```powershell
cargo run -- --level=shadow-motion-16
cargo run -- --level=shadow-motion-32
cargo run -- --level=shadow-motion-64
```

Android profiling APK：

```powershell
adb shell setprop debug.zevy.level shadow-motion-16
```

更换 profile 后必须 force-stop 并冷启动。清空属性恢复普通默认 Level：

```powershell
adb shell setprop debug.zevy.level ''
```

该 Android 属性仅存在于带 `render_debug` feature 的 profiling 包；Shipping 不读取测试 Level override。

## 测量协议

1. 冷启动后等待资源、pipeline 和 shadow cache 至少 10 秒。
2. 固定头显/相机路径；每档记录 warm 30 秒，再采样至少 60 秒。
3. 记录 FPS、CPU/GPU frame P50/P95/P99、fragment、draw、shadow face R/D/U、transition active/start/wait/copy、max stale、GPU/CPU 频率和温度。
4. 保存双眼截图；HUD 的 active/effective slots、wait 和 copy 必须左右眼一致。
5. 顺序使用 16→32→64→32→16，检查热状态和顺序偏差；正式裁决再做 20～30 分钟 thermal soak。
6. P2 与完整动态 reference 的画质比较必须使用相同灯位和时刻；保存差异图并检查双影、漏光、漂浮、阶梯和 disocclusion。

## 验收与 kill criteria

- 夹具必须在 PC 和 Android 正确生成请求数量的 shadowed SlowMoving 灯与 4 个 DynamicOverlay caster。
- active transition 必须在 Pico 上真实大于 0，且左右眼共享状态；只看到 `0/K` 不能算主动路径验收。
- 槽超订阅时不能出现灯/影按相机距离突然消失，也不能长期饿死同一实体。
- 16/32/64 的 shadow view、copy、wait 和 stale 必须符合可解释的成本模型；异常斜率必须定位到具体 pass。
- 若 P2 的额外 shadow sample、copy 或视觉误差比完整动态 reference 更差，则回退该类灯到 `FullyDynamic` 并进入 edge-aware/temporal reconstruction，不缩短物理 range 掩盖问题。

## 2026-07-24 PC 实现与首轮结果

独立 `ShadowMotionLab`、16/32/64 profile、确定性灯光/动态 caster 运动、Desktop 参数和 profiling-only Android `debug.zevy.level` 已实现。实验还发现并修复了一处通用分类错误：带显式 `ResolvedShadowCasterMotion::Static` 的运行时 mesh 现在优先服从 policy；只有没有 policy 的运行时 mesh 才走 correctness-first dynamic fallback。

固定时刻的 Materials/Lights HUD 结果如下。截图只证明结构和瞬时状态，不代表真机性能或热稳定结论。

| 灯数 | resident shadow views | DynamicOverlay caster / redraw faces | active / wait | max stale | PC 视觉 |
|---:|---:|---:|---:|---:|---|
| 16 | 96 | 4 / 32 | 4 / 12 | 0.020 m | 正常 |
| 32 | 192 | 4 / 58 | 4 / 28 | 0.042 m | 正常 |
| 64 | 384 | 4 / 113 | 4 / 60 | 0.074 m | 默认 direct-light overflow 出现棋盘斑块 |

结果符合两个模型：dynamic overlay 只重画与 caster 相交的 face，但仍随灯数近似线性增长；固定 4 槽使 wait 和 stale 随灯数增长。64 灯棋盘斑块通过单变量 A/B 定位到已有 direct-light overflow：仅将 exact threshold 从默认 18 临时提高到 64 后斑块完全消失，而 shadow profile、caster、相机和时刻不变。临时值已恢复为 18。该失败不能归因于 P2 cross-fade，也不能作为降低灯数/范围的理由；overflow 后续必须加入双眼共享 Top-K/reservoir 与 edge-aware reconstruction，raw stochastic 结果不得直接输出到眼睛。

## 2026-07-24 Android/VR 结果

同一 release + `render_debug` APK、XR scale 1.0、MSAA 2x，每档冷启动并预热至少 30 秒。32/64 的 PxrMetric CPU/GPU 都达到 66.67 ms 报告上限，表中只能写作下界。

| 灯数 | resident views | dynamic caster / faces | active / wait | max stale | FPS avg | GPU avg / P95 |
|---:|---:|---:|---:|---:|---:|---:|
| 16 | 96 | 4 / 33 | 4 / 12 | 0.026 m | 17.65 | 53.47 / 58.51 ms |
| 32 | 192 | 4 / 60 | 4 / 28 | 0.046 m | 11.10 | ≥66.67 / ≥66.67 ms |
| 64 | 384 | 4 / 121 | 4 / 60 | 0.074 m | 7.75 | ≥66.67 / ≥66.67 ms |

三档均真实触发 transition，左右眼 HUD 状态一致，无 panic、wgpu 或 Vulkan validation failure。64 灯 raw reservoir 棋盘斑块在 Pico 同样复现。默认 XR `HandGizmosPlugin` 产生的彩色手部圆环会污染 benchmark，现已从产品路径移除；hand tracking 与 Map hand harness 没有删除。

16 灯四档成本分解与 Top-K 下界见 `Reduced_Rate_Local_Lighting.md`。结论是稳定 cache/simple geometry 下，shadow submission 不是 53 ms 的同量级主因；全分辨率 direct-light fragment 与 shadow sampling 耦合才是下一结构性目标。`scripts/profile_shadow_motion_lab.ps1` 已把冷启动、固定变量、PxrMetric 解析与 P95 计算固化。

## 状态

- [已实现] 数学模型、独立 Level、嵌套 profile、确定性运动、Desktop/Android profiling 接口、自动测试与重复测量脚本。
- [PC 验证] 16/32/64 均生成正确灯数、policy、resident views、4 个 DynamicOverlay caster，并真实触发 4 个 transition。
- [PC/Android A/B 验证] 64 灯棋盘斑块来自现有 direct-light overflow；全精确参考消除斑块。
- [Android/VR 验证] 16/32/64 主动 transition、左右眼状态、性能斜率和 16 灯四档/K 下界已记录。
- [自动测试] 2026-07-24 当前工作树 74 项 Rust 单测通过；Android profiling/Shipping check 通过。
- [未验证] 固定相机路径 60 秒往返、GPU capture、误差图和 20～30 分钟 thermal soak。
