# Tag 构建

推送任意 tag 会运行 `.github/workflows/build-tags.yml`，也可在 Actions 页面手动运行。

| 平台 | 架构 | Runner |
| --- | --- | --- |
| Windows | x64 | windows-2022 |
| Linux | x64 | ubuntu-24.04 |
| Linux | ARM64 | ubuntu-24.04-arm |
| macOS | x64 | macos-15-intel |
| macOS | ARM64 | macos-15 |

各架构使用原生 runner 编译，固定 Rust `nightly-2026-05-05`，使用仓库内的 Cargo.lock。每个任务独立上传压缩包和 SHA-256 校验文件到 Actions Artifacts；某个平台失败不会中断其他平台的构建。

压缩包包含程序和 sherpa-onnx / ONNX Runtime 动态库，不包含模型权重及 FFmpeg。Linux 包修正同目录动态库的 RPATH；macOS 包修正动态库引用后进行 ad-hoc 签名，未进行 Apple 开发者签名或公证。该流程不自动创建 GitHub Release。

运行前需将 `ffmpeg`、`ffprobe` 加入 PATH。Linux 需要图形桌面、Vulkan 驱动及相应系统库，Ubuntu 24.04 可安装 `libasound2t64 libfontconfig1 libxkbcommon-x11-0 libwayland-client0 libgtk-3-0t64 libwebkit2gtk-4.1-0 libvulkan1 fonts-noto-cjk`。macOS 最低版本为 15。

应用数据目录由操作系统确定：Windows 使用 LocalAppData，macOS 使用 `~/Library/Application Support/Vig`，Linux 使用 `$XDG_DATA_HOME/Vig`（未设置时为 `~/.local/share/Vig`）。识别设置位于 `asr.json`，模型位于 `models` 子目录。

字幕使用 Windows 的微软雅黑、macOS 的苹方、Linux 的 Noto Sans CJK SC；非 Windows 平台通过系统字体服务查询字体文件路径。缺少字体会明确报错。Linux 请安装 `fonts-noto-cjk`。

Windows 的数字拖动保留隐藏光标和重定位；Linux/macOS 使用 GPUI 的鼠标位置、Shift 和释放事件进行普通拖动，失去窗口焦点时结束拖动，不支持无限重定位。
