use super::{Progress, check_cancel, manifest::File, request};
use sha2::Digest;
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::AtomicBool,
};

fn verify(path: &Path, metadata: &File, cancel: &AtomicBool) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != metadata.size {
        return Err(format!(
            "文件长度校验失败：{}；请删除该文件后重新下载",
            path.display()
        ));
    }
    let mut sha256 = sha2::Sha256::new();
    let mut sha1 = sha1::Sha1::new();
    sha1.update(format!("blob {}\0", metadata.size).as_bytes());
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        if metadata.lfs.is_some() {
            sha256.update(&buffer[..count]);
        } else {
            sha1.update(&buffer[..count]);
        }
    }
    let (actual, expected) = if let Some(lfs) = &metadata.lfs {
        (format!("{:x}", sha256.finalize()), &lfs.sha256)
    } else {
        (format!("{:x}", sha1.finalize()), &metadata.blob)
    };
    if actual != expected.to_ascii_lowercase() {
        return Err(format!(
            "文件哈希校验失败：{}；请删除该文件后重新下载",
            path.display()
        ));
    }
    Ok(())
}

pub(super) async fn download_file(
    client: &reqwest::Client,
    endpoint: &str,
    repository: &str,
    revision: &str,
    directory: &Path,
    file: &File,
    cancel: &AtomicBool,
    progress: &mut Progress<'_>,
) -> Result<(), String> {
    check_cancel(cancel)?;
    let destination = directory.join(&file.name);
    if destination.exists() {
        progress.report(&file.name, true, true);
        verify(&destination, file, cancel)?;
        progress.completed += file.size;
        progress.report(&file.name, true, true);
        return Ok(());
    }
    let partial = directory.join(format!("{}.part", file.name));
    let mut output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial)
        .map_err(|e| format!("无法创建下载文件：{e}"))?;
    let mut offset = output.metadata().map_err(|e| e.to_string())?.len();
    if offset > file.size {
        return Err(format!(
            "未完成文件长度异常：{}；请删除该文件后重新下载",
            partial.display()
        ));
    }
    progress.completed += offset;
    progress.report(&file.name, false, true);
    if offset < file.size {
        let mut url = reqwest::Url::parse(&format!("{endpoint}/{repository}/resolve/{revision}/"))
            .map_err(|e| e.to_string())?;
        url.path_segments_mut()
            .map_err(|_| "下载源 URL 无效")?
            .pop_if_empty()
            .push(&file.name);
        let mut download = client
            .get(url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity");
        if offset > 0 {
            download = download.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let mut response = request(download.send(), cancel)
            .await?
            .error_for_status()
            .map_err(|e| format!("下载 {} 失败：{e}", file.name))?;
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            let expected = format!("bytes {offset}-{}/{}", file.size - 1, file.size);
            if response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                != Some(expected.as_str())
            {
                return Err(format!(
                    "{} 的断点续传响应区间不正确，未追加文件",
                    file.name
                ));
            }
        } else if offset > 0 || response.status() != reqwest::StatusCode::OK {
            return Err(format!(
                "下载源没有正确响应 {} 的断点续传请求，未追加文件",
                file.name
            ));
        }
        while let Some(chunk) = request(response.chunk(), cancel).await? {
            if chunk.len() as u64 > file.size - offset {
                return Err(format!("{} 的下载内容超过清单长度", file.name));
            }
            output
                .write_all(&chunk)
                .map_err(|e| format!("写入模型失败：{e}"))?;
            offset += chunk.len() as u64;
            progress.completed += chunk.len() as u64;
            progress.transferred += chunk.len() as u64;
            progress.report(&file.name, false, false);
        }
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    progress.report(&file.name, true, true);
    verify(&partial, file, cancel)?;
    check_cancel(cancel)?;
    std::fs::rename(&partial, &destination).map_err(|e| format!("完成模型文件失败：{e}"))?;
    progress.report(&file.name, true, true);
    Ok(())
}
