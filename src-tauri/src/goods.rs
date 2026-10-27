//! 解析商品导出文件（txt / json），生成按「主图 / SKU图 / 详情图」归类的下载任务。

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MAIN_DIR: &str = "主图";
pub const SKU_DIR: &str = "SKU图";
pub const DETAIL_DIR: &str = "详情图";

/// 一张待下载的图片。
#[derive(Debug, Clone)]
pub struct ImageTask {
    pub url: String,
    pub dest: PathBuf,
    /// 用于日志与进度显示，如 `990498294408/主图/主图_01.jpg`
    pub label: String,
    /// 来源文件路径
    pub source: String,
}

/// 从导出文件里读出的商品。
pub struct Goods {
    pub goods_id: String,
    pub goods_name: String,
    pub main: Vec<String>,
    /// (SKU 名称，URL)，已按 URL 去重
    pub skus: Vec<(String, String)>,
    pub detail: Vec<String>,
}

/// 文件解析结果，给前端列表显示。
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
    pub goods_id: String,
    pub goods_name: String,
    pub main_count: usize,
    pub sku_count: usize,
    pub detail_count: usize,
}

impl FileInfo {
    pub fn inspect(path: &str) -> Self {
        match load(Path::new(path)) {
            Ok(g) => FileInfo {
                path: path.to_string(),
                ok: true,
                error: None,
                goods_id: g.goods_id,
                goods_name: g.goods_name,
                main_count: g.main.len(),
                sku_count: g.skus.len(),
                detail_count: g.detail.len(),
            },
            Err(e) => FileInfo {
                path: path.to_string(),
                ok: false,
                error: Some(e),
                goods_id: String::new(),
                goods_name: String::new(),
                main_count: 0,
                sku_count: 0,
                detail_count: 0,
            },
        }
    }
}

pub fn load(path: &Path) -> Result<Goods, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取文件失败：{e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("不是有效的 JSON：{e}"))?;
    let goods = find_goods(&root).ok_or("没有找到 goodsID 字段")?;

    let goods_id = match &goods["goodsID"] {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    if goods_id.is_empty() {
        return Err("goodsID 为空".into());
    }
    let goods_name = goods["goodsName"].as_str().unwrap_or_default().to_string();

    let main = url_list(&goods["topGallery"]);
    let detail = url_list(&goods["detailGallery"]);
    let skus = sku_images(&goods["skus"]);
    if main.is_empty() && skus.is_empty() && detail.is_empty() {
        return Err("文件里没有图片链接".into());
    }
    Ok(Goods { goods_id, goods_name, main, skus, detail })
}

/// 生成下载任务：`<base>/<goodsID>/主图/主图_01.jpg` 等。
pub fn plan(goods: &Goods, base: &Path, source: &str) -> Vec<ImageTask> {
    let id = sanitize(&goods.goods_id);
    let root = base.join(&id);
    let mut tasks = Vec::new();
    let mut push = |dir: &str, stem: String, url: &str| {
        let file = format!("{stem}.{}", extension(url));
        tasks.push(ImageTask {
            url: url.to_string(),
            dest: root.join(dir).join(&file),
            label: format!("{id}/{dir}/{file}"),
            source: source.to_string(),
        });
    };

    let width = |n: usize| n.to_string().len().max(2);
    let w = width(goods.main.len());
    for (i, url) in goods.main.iter().enumerate() {
        push(MAIN_DIR, format!("主图_{:0w$}", i + 1), url);
    }
    for (name, url) in &goods.skus {
        push(SKU_DIR, format!("SKU图_{name}"), url);
    }
    let w = width(goods.detail.len());
    for (i, url) in goods.detail.iter().enumerate() {
        push(DETAIL_DIR, format!("详情图_{:0w$}", i + 1), url);
    }
    tasks
}

/// 找商品对象：通常在 `store.initDataObj.goods`，其他导出格式按含 goodsID 的对象兜底。
fn find_goods(root: &Value) -> Option<&Value> {
    let direct = &root["store"]["initDataObj"]["goods"];
    if direct.get("goodsID").is_some() {
        return Some(direct);
    }
    let mut fallback = None;
    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(map) => {
                if map.contains_key("goodsID") {
                    if ["topGallery", "skus", "detailGallery"].iter().any(|k| map.contains_key(*k)) {
                        return Some(v);
                    }
                    fallback.get_or_insert(v);
                }
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    fallback
}

/// 数组元素可以是字符串，或带 url / src 的对象。
fn url_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(s) => normalize_url(s),
                    Value::Object(o) => ["url", "src", "imgUrl"]
                        .iter()
                        .find_map(|k| o.get(*k).and_then(Value::as_str).and_then(normalize_url)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_url(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with("//") {
        Some(format!("https:{s}"))
    } else if s.starts_with("http://") || s.starts_with("https://") {
        Some(s.to_string())
    } else {
        None
    }
}

/// SKU 图按 URL 去重（同一颜色的多个尺码共用一张图），名称取决定图片的规格值。
///
/// 「决定图片的规格」= 在每组同图 SKU 里取值都相同的规格（如颜色分类），
/// 尺码这类在同图 SKU 里变化的规格不进文件名。
fn sku_images(v: &Value) -> Vec<(String, String)> {
    struct Sku {
        url: String,
        specs: Vec<(String, String)>,
    }
    let skus: Vec<Sku> = v
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|s| {
                    let url = s["thumbUrl"].as_str().and_then(normalize_url)?;
                    let specs = s["specs"]
                        .as_array()
                        .map(|specs| {
                            specs
                                .iter()
                                .map(|p| {
                                    let key = p["spec_key"].as_str().unwrap_or_default();
                                    let value = p["spec_value"].as_str().unwrap_or_default();
                                    (key.trim().to_string(), value.trim().to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(Sku { url, specs })
                })
                .collect()
        })
        .unwrap_or_default();

    // 按 URL 分组，保持首次出现的顺序
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, Vec<&Sku>> = HashMap::new();
    for sku in &skus {
        groups
            .entry(&sku.url)
            .or_insert_with(|| {
                order.push(&sku.url);
                Vec::new()
            })
            .push(sku);
    }

    let mut keys: Vec<&str> = Vec::new();
    for sku in &skus {
        for (k, _) in &sku.specs {
            if !keys.contains(&k.as_str()) {
                keys.push(k);
            }
        }
    }
    let value_of = |sku: &Sku, key: &str| {
        sku.specs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default()
    };
    let mut defining: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|k| {
            groups.values().all(|g| {
                let first = value_of(g[0], k);
                g.iter().all(|s| value_of(s, k) == first)
            })
        })
        .collect();
    // 多个候选时，去掉全部 SKU 都同值的规格（对区分图片没有帮助）
    if defining.len() > 1 {
        let varying: Vec<&str> = defining
            .iter()
            .copied()
            .filter(|k| skus.iter().map(|s| value_of(s, k)).collect::<HashSet<_>>().len() > 1)
            .collect();
        if !varying.is_empty() {
            defining = varying;
        }
    }
    if defining.is_empty() {
        defining = keys.clone();
    }

    let mut used = HashSet::new();
    order
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let first = groups[url][0];
            let parts: Vec<String> = defining
                .iter()
                .map(|k| value_of(first, k))
                .filter(|v| !v.is_empty())
                .collect();
            let base = if parts.is_empty() {
                format!("{:02}", i + 1)
            } else {
                truncate(&sanitize(&parts.join("_")), 80)
            };
            let mut name = base.clone();
            let mut n = 2;
            while !used.insert(name.clone()) {
                name = format!("{base}_{n}");
                n += 1;
            }
            (name, url.to_string())
        })
        .collect()
}

/// 文件名里不能出现的字符换成外形相近的全角字符，规格值仍然好读。
pub fn sanitize(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| match c {
            '/' => '／',
            '\\' => '＼',
            ':' => '：',
            '*' => '＊',
            '?' => '？',
            '"' => '＂',
            '<' => '＜',
            '>' => '＞',
            '|' => '｜',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out.trim().trim_end_matches('.').to_string()
}

fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// 从 URL 取扩展名（`xxx.jpg_.webp` 取 webp），认不出时用 jpg。
fn extension(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let last = path.rsplit('/').next().unwrap_or(path);
    let ext = last.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "avif" | "heic" => ext,
        _ => "jpg".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sku_dedup_uses_color_not_size() {
        let v = json!([
            {"thumbUrl": "https://a/1.jpg", "specs": [{"spec_key":"颜色","spec_value":"橘色【洋气/欧货】"},{"spec_key":"尺码","spec_value":"S"}]},
            {"thumbUrl": "https://a/1.jpg", "specs": [{"spec_key":"颜色","spec_value":"橘色【洋气/欧货】"},{"spec_key":"尺码","spec_value":"M"}]},
            {"thumbUrl": "https://a/2.jpg", "specs": [{"spec_key":"颜色","spec_value":"黑色"},{"spec_key":"尺码","spec_value":"S"}]}
        ]);
        let skus = sku_images(&v);
        assert_eq!(
            skus,
            vec![
                ("橘色【洋气／欧货】".to_string(), "https://a/1.jpg".to_string()),
                ("黑色".to_string(), "https://a/2.jpg".to_string()),
            ]
        );
    }

    /// 打印真实导出文件的下载计划：`GOODS_FILE=… cargo test print_plan -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_plan() {
        let path = std::env::var("GOODS_FILE").expect("GOODS_FILE");
        let g = load(Path::new(&path)).unwrap();
        for t in plan(&g, Path::new("/out"), &path) {
            println!("{}", t.label);
        }
    }

    #[test]
    fn extension_from_url() {
        assert_eq!(extension("https://x/O1CN01_!!1756.jpg"), "jpg");
        assert_eq!(extension("https://x/a.jpg_.webp?x=1"), "webp");
        assert_eq!(extension("https://x/a"), "jpg");
    }
}
