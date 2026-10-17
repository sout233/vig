use std::path::PathBuf;

pub fn data_directory() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .map(|directory| directory.join("Vig"))
        .ok_or_else(|| "无法确定当前用户的应用数据目录".into())
}

pub fn subtitle_font() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let path = PathBuf::from(std::env::var_os("WINDIR").ok_or("无法确定系统字体目录")?)
        .join("Fonts/msyh.ttc");

    #[cfg(not(windows))]
    let path = {
        use zed_font_kit::{
            family_name::FamilyName, handle::Handle, properties::Properties, source::SystemSource,
        };
        let family = if cfg!(target_os = "macos") {
            "PingFang SC"
        } else {
            "Noto Sans CJK SC"
        };
        let handle = SystemSource::new()
            .select_best_match(&[FamilyName::Title(family.into())], &Properties::new())
            .map_err(|error| format!("无法找到字幕字体 {family}，请先安装该字体：{error}"))?;
        match handle {
            Handle::Path { path, .. } => path,
            _ => return Err(format!("字幕字体 {family} 没有可供 FFmpeg 读取的文件路径")),
        }
    };
    if !path.is_file() {
        return Err(format!("字幕字体不存在：{}", path.display()));
    }
    Ok(path)
}
