# Vig

使用 Rust 和 GPUI Kit 构建的本地视频剪辑软件，当前主要在 Windows 上开发和验证。

## 功能

- 视频轨和音频轨可添加、删除、排序及调整高度，片段支持跨轨移动。
- 视频和音频默认绑定，支持裁剪、分割、多选、复制粘贴及撤销／重做。
- 支持片段淡入淡出、重叠交叉渐变，以及位置、缩放、旋转、不透明度变换。
- 提供文本、纯色生成式素材和音频波形预览。
- 预览使用异步加载与缓存，支持 Full、1/2、1/4 分辨率；工程可设置分辨率和帧率。
- 支持本地语音识别生成字幕，以及通过 FFmpeg 导出 MP4。

## 构建与运行

需要 Rust 工具链、Visual Studio C++ Build Tools（含 Windows SDK），以及可通过 `PATH` 访问的 `ffmpeg` 和 `ffprobe`。当前验证使用 Rust nightly 工具链。文本渲染要求 FFmpeg 包含 `drawtext` 滤镜，默认使用 Windows 的微软雅黑字体。

```powershell
cargo run --locked
```

发布构建：

```powershell
cargo build --locked --release
```

首次构建需要联网获取依赖和 sherpa-onnx 原生运行库。构建脚本会将运行库复制到输出目录；分发时应保留同目录下的 DLL，不能只复制 `vig.exe`。模型权重通过软件单独下载，不包含在仓库中。

## 自动生成字幕

打开「工具 → 自动生成字幕…」，选择源音频轨道、识别模型和下载源。

| 识别模型 | 时间定位方式 | 所需模型 |
| --- | --- | --- |
| SenseVoice Small INT8 | 按 VAD 检测到的语音段计时 | 约 239 MB 的识别权重及约 2.3 MB 的 Silero VAD |
| Qwen3-ASR 0.6B / 1.7B | 通过强制对齐获得词语时间戳 | 识别模型及 Qwen3-ForcedAligner-0.6B |

下载源可选择 Hugging Face 或 `hf-mirror.com`。支持进度、取消、断点续传、文件校验及下载后自动导入，也可复制下载链接或手动导入已有模型目录。窗口内的「如何导入」提供文件要求。

模型与设置保存在 `%LOCALAPPDATA%\Vig`，不会写入工程文件。识别在本机 CPU 上运行，音频不会上传。字幕使用音频轨道实际的裁剪、速度和混音结果，生成后是普通视频轨上的可编辑文本素材。

SenseVoice 提供语音段时间，不能将长段文本按字数均分后视作逐字对齐。Qwen 的当前接入使用独立的强制对齐模型；两种 Qwen 识别模型共用同一个对齐模型。

## 常用操作

| 操作 | 快捷键或鼠标操作 |
| --- | --- |
| 导入素材 | Ctrl+O |
| 播放／暂停 | Space |
| 在播放头处分割 | S |
| 复制／粘贴 | Ctrl+C / Ctrl+V |
| 复制拖动 | Ctrl+拖动 |
| 多选 | Ctrl+左键，或在时间线空白处框选 |
| 时间线纵向滚动 | 滚轮 |
| 时间线横向缩放 | Ctrl+滚轮 |
| 轨道高度缩放 | Alt+滚轮 |
| 时间线横向滚动 | Shift+滚轮 |

输入框获得焦点时，编辑快捷键不会抢占文字输入。

## 验证

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets --features ui-verify
cargo test --locked --features ui-verify -- --test-threads=2
.\scripts\check-ui-stack.ps1
```

媒体测试使用 FFmpeg 生成测试素材，需要 `ffmpeg` 和 `ffprobe`。`ui-verify` 启用界面交互测试；`check-ui-stack.ps1` 使用 1 MB 线程栈验证时间线界面。

真实语音模型测试默认跳过，不会自动下载权重。SenseVoice 测试需要设置 `VIG_SENSEVOICE_MODEL`、`VIG_VAD_MODEL` 和 `VIG_ASR_AUDIO`；Qwen 测试需要设置 `VIG_ASR_MODEL`、`VIG_ASR_ALIGNER` 和 `VIG_ASR_AUDIO`。通过对应测试名和 `--ignored` 显式运行。

## 目录

- `src/model/`：工程、轨道、片段及编辑规则。
- `src/media/`：FFmpeg 处理、预览缓存、播放同步和波形。
- `src/editor/`：编辑器状态、用户操作及后台任务。
- `src/views/`：界面、时间线、属性与菜单。
- `src/asr/`：语音模型下载、识别和字幕时间定位。
- `src/theme/`：主题、材质和按钮交互。
- `src/verify/`：可选的原生界面验证流程。
- `assets/icons/`：本地图标及其许可说明。

依赖库、FFmpeg 和模型分别保留各自的许可证。Boxicons 图标的许可位于 `assets/icons/LICENSE`。
