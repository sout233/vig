// Real HTTP transfers against a local fixture exercise the production downloader.
use super::*;
use sha2::Digest;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

const REVISION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("config.json".into(), br#"{"thinker_config":{"model_type":"qwen3_asr","audio_config":{"encoder_layers":18}}}"#.to_vec()),
        ("tokenizer_config.json".into(), b"{}".to_vec()), ("tokenizer.json".into(), b"{}".to_vec()),
        ("model.safetensors".into(), b"fixture weight bytes".to_vec()),
    ])
}

fn server(
    files: BTreeMap<String, Vec<u8>>,
    corrupt: bool,
    ignore_range: bool,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let thread = std::thread::spawn(move || {
        // The client closes its connections when the test sends the shutdown request.
        for incoming in listener.incoming() {
            let mut stream = incoming.unwrap();
            let mut raw = Vec::new();
            let mut buffer = [0; 4096];
            while !raw.windows(4).any(|x| x == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                raw.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(raw).unwrap();
            let url = request.split_whitespace().nth(1).unwrap();
            if url == "/shutdown" {
                break;
            }
            captured.lock().unwrap().push(request.clone());
            let (body, status, extra) = if url.starts_with("/api/") {
                let siblings: Vec<_> = files
                    .iter()
                    .map(|(name, data)| {
                        let mut hash = sha1::Sha1::new();
                        hash.update(format!("blob {}\0", data.len()));
                        hash.update(data);

                        serde_json::json!({
                            "rfilename": name,
                            "size": data.len(),
                            "blobId": format!("{:x}", hash.finalize()),
                            "lfs": if name.ends_with(".safetensors") {
                                Some(serde_json::json!({
                                    "sha256": format!("{:x}", sha2::Sha256::digest(data))
                                }))
                            } else {
                                None
                            }
                        })
                    })
                    .collect();
                (
                    serde_json::to_vec(&serde_json::json!({"sha":REVISION,"siblings":siblings}))
                        .unwrap(),
                    "200 OK",
                    String::new(),
                )
            } else {
                assert!(url.contains(REVISION), "文件下载必须固定到清单版本");
                let name = url.rsplit('/').next().unwrap();
                let mut data = files[name].clone();
                if corrupt && name.ends_with(".safetensors") {
                    data[0] ^= 1;
                }
                let range = request.lines().find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("range: bytes=")
                        .map(|value| value.trim_end_matches('-').parse::<usize>().unwrap())
                });
                if let Some(offset) = range.filter(|_| !ignore_range) {
                    let extra = format!(
                        "Content-Range: bytes {offset}-{}/{}\r\n",
                        data.len() - 1,
                        data.len()
                    );
                    (data[offset..].to_vec(), "206 Partial Content", extra)
                } else {
                    (data, "200 OK", String::new())
                }
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{extra}\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (endpoint, requests, thread)
}

fn shutdown(endpoint: &str, thread: std::thread::JoinHandle<()>) {
    let mut connection =
        std::net::TcpStream::connect(endpoint.trim_start_matches("http://")).unwrap();
    connection
        .write_all(b"GET /shutdown HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    thread.join().unwrap();
}

#[test]
fn sensevoice_download_selects_only_int8_bundle_and_vad_files() {
    for (target, names) in [
        (
            ModelDownload::Recognition(ModelSize::SenseVoice),
            vec!["model.int8.onnx", "tokens.txt", "LICENSE"],
        ),
        (ModelDownload::Vad, vec!["silero_vad.onnx", "LICENSE.txt"]),
    ] {
        let mut files: BTreeMap<String, Vec<u8>> = names
            .iter()
            .map(|name| (name.to_string(), b"fixture".to_vec()))
            .collect();
        files.insert(
            "model.onnx".into(),
            b"unwanted full precision model".to_vec(),
        );
        if files.contains_key("tokens.txt") {
            files.insert(
                "tokens.txt".into(),
                b"<unk> 0\n<|zh|> 1\n<|en|> 2\n<|Speech|> 3\n<|withitn|> 4\n".to_vec(),
            );
        }
        let (endpoint, requests, thread) = server(files, false, false);
        let root = tempfile::tempdir().unwrap();
        let destination = download_from(
            target,
            &endpoint,
            root.path(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        for name in names {
            assert!(destination.join(name).is_file());
        }
        assert!(!destination.join("model.onnx").exists());
        assert!(
            !requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.contains("/model.onnx "))
        );
        shutdown(&endpoint, thread);
    }
}

#[test]
fn downloads_resume_verify_and_reuse_complete_models() {
    let files = fixture();
    let (endpoint, requests, thread) = server(files.clone(), false, false);
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("Qwen/Qwen3-ASR-0.6B").join(REVISION);
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(
        destination.join("model.safetensors.part"),
        &files["model.safetensors"][..6],
    )
    .unwrap();
    let mut progress = Vec::new();
    let result = download_from(
        ModelDownload::Recognition(ModelSize::Small),
        &endpoint,
        root.path(),
        &AtomicBool::new(false),
        &mut |p| progress.push(p),
    )
    .unwrap();
    assert_eq!(result, destination);
    for (name, data) in &files {
        assert_eq!(std::fs::read(destination.join(name)).unwrap(), *data);
    }
    assert_eq!(progress.last().unwrap().percentage(), 100.);
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.to_ascii_lowercase().contains("range: bytes=6-"))
    );
    let before = requests.lock().unwrap().len();
    download_from(
        ModelDownload::Recognition(ModelSize::Small),
        &endpoint,
        root.path(),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        requests.lock().unwrap().len(),
        before + 1,
        "完整文件只校验，不重新下载"
    );
    shutdown(&endpoint, thread);
}

#[test]
fn corrupt_downloads_and_invalid_resume_responses_never_install() {
    for (corrupt, ignore_range) in [(true, false), (false, true)] {
        let (endpoint, _, thread) = server(fixture(), corrupt, ignore_range);
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("Qwen/Qwen3-ASR-0.6B").join(REVISION);
        if ignore_range {
            std::fs::create_dir_all(&destination).unwrap();
            std::fs::write(destination.join("model.safetensors.part"), b"fix").unwrap();
        }
        let error = download_from(
            ModelDownload::Recognition(ModelSize::Small),
            &endpoint,
            root.path(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(
            error.contains(if corrupt { "哈希" } else { "断点续传" }),
            "{error}"
        );
        assert!(!destination.join("model.safetensors").exists());
        if ignore_range {
            assert_eq!(
                std::fs::read(destination.join("model.safetensors.part")).unwrap(),
                b"fix"
            );
        }
        shutdown(&endpoint, thread);
    }
}

#[test]
fn cancellation_preserves_partial_files_for_the_next_attempt() {
    let (endpoint, _, thread) = server(fixture(), false, false);
    let root = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(false);
    let error = download_from(
        ModelDownload::Recognition(ModelSize::Small),
        &endpoint,
        root.path(),
        &cancel,
        &mut |p| {
            if p.file == "model.safetensors" {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap_err();
    assert!(error.contains("已取消"));
    let destination = root.path().join("Qwen/Qwen3-ASR-0.6B").join(REVISION);
    assert!(destination.join("model.safetensors.part").exists());
    download_from(
        ModelDownload::Recognition(ModelSize::Small),
        &endpoint,
        root.path(),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert!(destination.join("model.safetensors").exists());
    shutdown(&endpoint, thread);
}

#[test]
fn older_model_preferences_default_to_the_official_source() {
    let preferences: super::super::Preferences =
        serde_json::from_str(r#"{"small":null,"large":null,"aligner":null,"selected":"Small"}"#)
            .unwrap();
    assert_eq!(preferences.source, DownloadSource::HuggingFace);
    assert_eq!(DownloadSource::Mirror.endpoint(), "https://hf-mirror.com");
}

#[test]
fn cancellation_interrupts_a_pending_network_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0; 4096];
        assert!(stream.read(&mut buffer).unwrap() > 0);
        flag.store(true, Ordering::Relaxed);
        // Keep the response pending until cancellation closes the connection.
        assert_eq!(stream.read(&mut buffer).unwrap(), 0);
    });
    let root = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let error = download_from(
        ModelDownload::Recognition(ModelSize::Small),
        &endpoint,
        root.path(),
        &cancel,
        &mut |_| {},
    )
    .unwrap_err();
    assert!(error.contains("已取消"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(2));
    server.join().unwrap();
}

#[test]
#[ignore = "需要访问两个公共下载源；仅下载配置文件，不下载权重"]
fn public_sources_provide_verified_config_files() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        for source in [DownloadSource::HuggingFace, DownloadSource::Mirror] {
            let endpoint = source.endpoint();
            let cancel = AtomicBool::new(false);
            let response = request(
                client
                    .get(format!(
                        "{endpoint}/api/models/Qwen/Qwen3-ASR-0.6B/revision/main?blobs=true"
                    ))
                    .send(),
                &cancel,
            )
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
            let repository = request(response.json::<manifest::Repository>(), &cancel)
                .await
                .unwrap();
            let (revision, files) = repository.files().unwrap();
            let file = files
                .into_iter()
                .find(|file| file.name == "config.json")
                .unwrap();
            let directory = tempfile::tempdir().unwrap();
            let mut callback = |_| {};
            let mut progress = Progress {
                completed: 0,
                total: file.size,
                transferred: 0,
                started: Instant::now(),
                last_report: Instant::now(),
                callback: &mut callback,
            };
            transfer::download_file(
                &client,
                endpoint,
                "Qwen/Qwen3-ASR-0.6B",
                &revision,
                directory.path(),
                &file,
                &cancel,
                &mut progress,
            )
            .await
            .unwrap();
            let text = std::fs::read_to_string(directory.path().join("config.json")).unwrap();
            let config: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(config["thinker_config"]["model_type"], "qwen3_asr");
            eprintln!("{endpoint}: verified config.json ({})", file.size);
        }
    });
}
