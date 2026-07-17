结论：你现在实现的“导出阶段生成完整 Mip 链，再由 Bevy 上传全部 Mip Level”是正确方向，尤其针对当前项目使用的 Bevy 0.16.1。它补上了 Bevy 默认加载普通 PNG 时缺失的环节，并不是重复实现已有功能。
Bevy 0.16.1 默认怎么处理纹理
我核对了当前工程和本机 Bevy 0.16.1 源码：
ImagePlugin 默认使用 Linear 采样。
普通 PNG 创建的 Image 默认只有 mip_level_count = 1。
mipmap_filter = Linear 只规定“已有 Mip Level 之间如何插值”，不会自动生成 Mipmap。
因此原来只有 PNG 原图时，即使开启 Linear 和各向异性过滤，GPU 仍然只能一直采样第 0 级。
这也解释了此前“设置了 mipmap_filter，但数字 Mip 没出现”的现象。glTF 规范实际上建议引擎在纹理使用 Mipmap 过滤方式时运行时生成 Mip，但如果引擎没有实现，只能退回普通 Linear/Nearest；Bevy 0.16.1 的普通 PNG 路径没有替我们完成生成。
GPU 选择 Mip Level 并不是简单按照“相机距离”，而是根据纹理在屏幕上的投影尺寸、UV 密度、模型缩放和观察角度计算 LOD。Trilinear Filtering 会在相邻两个 Mip Level 之间混合，所以数字测试中会出现两个数字同时模糊叠加，这是正常现象。
Mipmap 和抗锯齿不是同一种东西
画面问题	主要解决方案
远处纹理闪烁、摩尔纹、细线跳动	Mipmap + Trilinear
地面、墙面斜视时模糊或闪烁	各向异性过滤
模型轮廓、三角形边缘锯齿	MSAA、SMAA、FXAA、TAA
高光、细小发光点随移动闪烁	正确 Mip + TAA/材质级抗高光闪烁
阴影边缘锯齿	阴影贴图尺寸、过滤和 Bias

Bevy 默认 MSAA 是 4 倍采样，但 MSAA 主要处理几何轮廓，对纹理内部和高光闪烁帮助有限。TAA 能处理纹理和高光等更多类型的时间锯齿，但可能产生拖影、细节闪烁，并要求关闭 MSAA、提供正确的运动矢量。SMAA 比 FXAA 通常质量更高，并比 TAA 稳定、没有历史帧拖影，但同样建议关闭 MSAA。
社区和正式项目通常怎么做
静态资产：离线生成 Mip，保存为 KTX2/Basis Universal
这是更标准的生产方案。KTX2 可以在一个纹理文件里保存完整 Mip 链，并支持适合 GPU 的压缩。Bevy 的 KTX2 Loader 会读取文件的 level_count，把所有级别直接设置到 mip_level_count。

动态纹理：运行时 GPU 生成
适合反射纹理、Render Target、程序生成纹理。对于磁盘上的普通场景纹理，通常仍然推荐离线生成，避免每次启动重新计算。

旧版 Bevy 使用第三方或自定义插件
社区存在 bevy_mod_mipmap_generator 这类方案，提供 CPU 生成、压缩和缓存功能。但它会带来版本兼容和加载流程复杂度，因此不如离线 KTX2 稳定。

对 zevy_engine 的建议
当前阶段可以继续保留现有方案：
导出时生成完整 Mip 链。
正式导出关闭数字叠加。
min/mag/mipmap_filter 使用 Linear。
当前各向异性过滤为 16x，PC 合适；Android XR 可以测试 8x 和 16x 的性能差异。
PC 和 Android XR 明确设置 Msaa::Sample4，不要完全依赖相机默认值。
下一阶段建议改成：
原始可编辑纹理仍保存为单独的 PNG/TGA。
构建或导出时为每张纹理生成独立的 KTX2。
Bevy 加载 KTX2，不再需要 .zevy-mips Sidecar。
模型、材质、纹理、Level JSON 仍然保持独立，不会影响手动修改材质和 Actor Transform。
BaseColor、Normal、ORM、Emissive 应按各自纹理语义生成 Mip，不能永远使用同一种颜色缩小算法。
截至 2026 年 7 月 17 日，Bevy 最新稳定版已经是 0.19，而当前 zevy_engine 是 0.16.1，因此查看最新文档时要注意 API 和默认行为存在版本差异。
综合而言，我推荐 zevy 当前的质量基线为：
PC：完整 Mipmap + Trilinear + 16x AF + MSAA 4x
Android XR：完整 Mipmap + Trilinear + 8x/16x AF + MSAA 4x
如果之后仍有明显高光或细线时间闪烁，再单独评估 TAA，而不是用 TAA 替代 Mipmap。