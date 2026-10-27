mod download;
mod goods;

use goods::FileInfo;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

const PROGRESS_EVENT: &str = "download-progress";

/// 与 ui-kit 的 `useBatchProgress` 约定一致，另带本张图片的结果。
#[derive(Clone, Serialize)]
struct ProgressEvent {
    current: usize,
    total: usize,
    file_name: String,
    percent: f64,
    stage: String,
    ok: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FailedImage {
    source: String,
    label: String,
    url: String,
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadResult {
    total: usize,
    succeeded: usize,
    failed: Vec<FailedImage>,
    /// 有图片下载失败（或无法解析）的源文件，前端留在列表里便于重试
    failed_sources: Vec<String>,
    /// 每个商品的输出文件夹
    folders: Vec<String>,
}

/// 解析文件，返回每个文件的商品信息与图片数量。
#[tauri::command]
async fn inspect_files(paths: Vec<String>) -> Vec<FileInfo> {
    tauri::async_runtime::spawn_blocking(move || paths.iter().map(|p| FileInfo::inspect(p)).collect())
        .await
        .unwrap_or_default()
}

/// 下载全部文件里的图片。`out_dir` 为空时保存到各源文件所在文件夹。
#[tauri::command]
async fn download_images(
    app: AppHandle,
    files: Vec<String>,
    out_dir: Option<String>,
    threads: usize,
) -> Result<DownloadResult, String> {
    tauri::async_runtime::spawn_blocking(move || run_download(&app, &files, out_dir.as_deref(), threads))
        .await
        .map_err(|e| e.to_string())?
}

fn run_download(
    app: &AppHandle,
    files: &[String],
    out_dir: Option<&str>,
    threads: usize,
) -> Result<DownloadResult, String> {
    let custom = out_dir.map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from);
    let mut tasks = Vec::new();
    let mut folders = Vec::new();
    let mut failed = Vec::new();
    let mut failed_sources = Vec::new();

    for file in files {
        let path = Path::new(file);
        match goods::load(path) {
            Ok(g) => {
                let base = custom
                    .clone()
                    .or_else(|| path.parent().map(Path::to_path_buf))
                    .ok_or_else(|| format!("无法确定 {file} 的保存位置"))?;
                folders.push(base.join(goods::sanitize(&g.goods_id)).to_string_lossy().into_owned());
                tasks.extend(goods::plan(&g, &base, file));
            }
            Err(e) => {
                failed.push(FailedImage { source: file.clone(), label: file.clone(), url: String::new(), error: e });
                failed_sources.push(file.clone());
            }
        }
    }
    if tasks.is_empty() {
        return Err("没有可下载的图片".into());
    }

    let total = tasks.len();
    let client = download::client()?;
    let errors = Mutex::new(Vec::new());
    download::run(&tasks, threads, &client, |finished, task, result| {
        let error = result.err();
        if let Some(e) = &error {
            errors.lock().unwrap().push(FailedImage {
                source: task.source.clone(),
                label: task.label.clone(),
                url: task.url.clone(),
                error: e.clone(),
            });
        }
        let _ = app.emit(
            PROGRESS_EVENT,
            ProgressEvent {
                current: finished,
                total,
                file_name: task.label.clone(),
                percent: finished as f64 * 100.0 / total as f64,
                stage: "下载中".into(),
                ok: error.is_none(),
                error,
            },
        );
    });

    let errors = errors.into_inner().unwrap();
    for e in &errors {
        if !failed_sources.contains(&e.source) {
            failed_sources.push(e.source.clone());
        }
    }
    let succeeded = total - errors.len();
    failed.extend(errors);
    Ok(DownloadResult { total, succeeded, failed, failed_sources, folders })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![inspect_files, download_images])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
