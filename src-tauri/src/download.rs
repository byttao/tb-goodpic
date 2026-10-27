//! 多线程下载：固定数量的工作线程从同一个任务队列取图片，各自下载、写盘，完成一张回调一次。

use crate::goods::ImageTask;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/128.0 Safari/537.36";
const RETRIES: usize = 3;

pub fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("创建网络客户端失败：{e}"))
}

/// 用 `threads` 个线程下载全部任务。`on_done(已完成数, 任务, 结果)` 在工作线程里调用。
pub fn run<F>(tasks: &[ImageTask], threads: usize, client: &reqwest::blocking::Client, on_done: F)
where
    F: Fn(usize, &ImageTask, Result<(), String>) + Sync,
{
    let next = AtomicUsize::new(0);
    // 计数与回调放在同一把锁里，进度事件按完成数递增的顺序发出，进度条不会回退
    let done = Mutex::new(0usize);
    let threads = threads.clamp(1, 32).min(tasks.len().max(1));

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(task) = tasks.get(i) else { break };
                let result = fetch_with_retry(client, task);
                let mut finished = done.lock().unwrap_or_else(|e| e.into_inner());
                *finished += 1;
                on_done(*finished, task, result);
            });
        }
    });
}

fn fetch_with_retry(client: &reqwest::blocking::Client, task: &ImageTask) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..RETRIES {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(600 * attempt as u64));
        }
        match fetch(client, task) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn fetch(client: &reqwest::blocking::Client, task: &ImageTask) -> Result<(), String> {
    let mut resp = client
        .get(&task.url)
        .header(reqwest::header::REFERER, "https://item.taobao.com/")
        .send()
        .map_err(|e| format!("网络错误：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("服务器返回 {}", resp.status()));
    }
    let mut bytes = Vec::new();
    resp.read_to_end(&mut bytes).map_err(|e| format!("读取数据失败：{e}"))?;
    if bytes.is_empty() {
        return Err("下载到的文件为空".into());
    }

    if let Some(dir) = task.dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建文件夹失败：{e}"))?;
    }
    // 先写临时文件再改名，中途失败不会留下半张图
    let mut part = task.dest.clone().into_os_string();
    part.push(".part");
    std::fs::write(&part, &bytes).map_err(|e| format!("写入文件失败：{e}"))?;
    std::fs::rename(&part, &task.dest).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("保存文件失败：{e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goods;
    use std::path::Path;

    /// 真实下载：`GOODS_FILE=… DL_DIR=… cargo test real_download -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_download() {
        let file = std::env::var("GOODS_FILE").expect("GOODS_FILE");
        let out = std::env::var("DL_DIR").expect("DL_DIR");
        let g = goods::load(Path::new(&file)).unwrap();
        let tasks = goods::plan(&g, Path::new(&out), &file);
        let failed = Mutex::new(Vec::new());
        let start = std::time::Instant::now();
        run(&tasks, 8, &client().unwrap(), |n, t, r| {
            if let Err(e) = r {
                failed.lock().unwrap().push(format!("{} {e}", t.label));
            }
            if n % 20 == 0 || n == tasks.len() {
                println!("{n}/{}", tasks.len());
            }
        });
        let failed = failed.into_inner().unwrap();
        println!("用时 {:?}，失败 {}：{failed:?}", start.elapsed(), failed.len());
        assert!(failed.is_empty());
    }
}
