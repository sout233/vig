# Tag 构建

推送任意 tag 会运行 `.github/workflows/build-tags.yml`，也可在 Actions 页面手动运行。

| 平台 | 架构 | Runner |
| --- | --- | --- |
| Windows | x64 | windows-2022 |
| Linux | x64 | ubuntu-24.04 |
| Linux | ARM64 | ubuntu-24.04-arm |
| macOS | x64 | macos-15-intel |
| macOS | ARM64 | macos-15 |

各架构使用原生 runner 编译，固定 Rust `nightly-2026-05-05`，使用仓库内的 Cargo.lock。每个平台分别构建 `lite` 和 `full`，共十项构建，独立上传程序包和 SHA-256 校验文件到 Actions Artifacts；某项失败不会中断其他构建。

tag 构建的十项任务全部成功后，会自动创建或更新对应 GitHub Release，上传十个程序包和十个校验文件。包含 `-` 的 tag 标记为预发布版。发布前会核对 tag 与构建 commit、十项构建的成功状态、产物完整性及 SHA-256 校验值，失败时不执行发布。

补发已有双版本构建时，在 Actions 选择 `Publish built release` → `Run workflow`，选择最新 `main`，填写已有 tag 和十项任务均成功的 run ID。该操作复用已有 Artifacts，无需重新编译；不会移动 tag。手动在分支上运行构建流程时仅保存 Artifacts。

## 版本区别

| 版本 | 功能 | Windows 下载形式 |
| --- | --- | --- |
| lite 精简版 | 保留剪辑、手动文本字幕、预览和渲染；不含自动字幕识别或模型下载 | 单个 `.exe`，不附带 DLL |
| full 完整版 | 全部剪辑功能，以及 Qwen、SenseVoice、VAD、模型下载和自动字幕 | `.zip`，含 EXE 与所需的 ASR 动态库 |

精简版使用 `cargo build --release --no-default-features`，默认 `cargo build --release` 构建完整版。`asr` feature 同时控制识别引擎、模型下载、自动字幕界面和 C++ 桥接的编译，不是单纯隐藏菜单。精简版保留手动创建、修改文本字幕的功能。

CI 对 Windows 精简版开启 `-C target-feature=+crt-static`，并检查普通及延迟加载 PE 导入表，只允许 Windows 系统 DLL；若仍依赖 ASR DLL 或 Visual C++ 动态运行库，会使打包失败。单 EXE 指无附带第三方 DLL，并不包含 FFmpeg、语音模型或 Windows 自带的图形组件。Boxicons 的许可嵌在“关于 Vig”中。

完整版保留 `sherpa-onnx-c-api.dll`、`onnxruntime.dll`、`onnxruntime_providers_shared.dll`；不打包没有被当前程序使用的 `sherpa-onnx-cxx-api.dll`、`cargs.dll`。Windows 完整版需要 Microsoft Visual C++ 2015–2022 运行库，不能直接删除 ASR 所需的 DLL。

两种版本都不包含 FFmpeg。完整版的模型权重仍在软件中单独下载。Linux/macOS 的精简版也移除了 ASR 库，但依然依赖对应平台的系统图形库，并非静态链接的独立操作系统环境。Linux 包修正同目录动态库的 RPATH；macOS 包修正动态库引用后进行 ad-hoc 签名，未进行 Apple 开发者签名或公证。

运行前需将 `ffmpeg`、`ffprobe` 加入 PATH。Linux 需要图形桌面、Vulkan 驱动及相应系统库，Ubuntu 24.04 可安装 `libasound2t64 libfontconfig1 libxkbcommon-x11-0 libwayland-client0 libgtk-3-0t64 libwebkit2gtk-4.1-0 libvulkan1 fonts-noto-cjk`。macOS 最低版本为 15。

应用数据目录由操作系统确定：Windows 使用 LocalAppData，macOS 使用 `~/Library/Application Support/Vig`，Linux 使用 `$XDG_DATA_HOME/Vig`（未设置时为 `~/.local/share/Vig`）。识别设置位于 `asr.json`，模型位于 `models` 子目录。

字幕使用 Windows 的微软雅黑、macOS 的苹方、Linux 的 Noto Sans CJK SC；非 Windows 平台通过系统字体服务查询字体文件路径。缺少字体会明确报错。Linux 请安装 `fonts-noto-cjk`。

Windows 的数字拖动保留隐藏光标和重定位；Linux/macOS 使用 GPUI 的鼠标位置、Shift 和释放事件进行普通拖动，失去窗口焦点时结束拖动，不支持无限重定位。
