# 淘宝图片下载

拖入或选择商品导出文件（txt / json，如小强上货导出的 `990498294408.txt`），多线程下载其中的图片。

界面：Tauri 2 + React + Ant Design 6，按 [ui-kit](https://github.com/byttao/ui-kit) 规范（`src/ui-kit/`）。下载：Rust（`src-tauri/src/`）。

## 输出结构

```
<保存位置>/<goodsID>/
├── 主图/     topGallery，主图_01.jpg、主图_02.jpg …
├── SKU图/    skus 的 thumbUrl，SKU图_<规格值>.jpg；多个尺码共用的图只保存一张
└── 详情图/   detailGallery，详情图_01.jpg …
```

- 保存位置：源文件所在文件夹，或自定义文件夹（设置会被记住）。
- SKU 图名称取「决定图片的规格」：同一张图的各 SKU 里取值都相同的规格（通常是颜色分类），尺码不进文件名。
- 文件名里不能用的字符换成外形相近的全角字符，如 `/` → `／`。
- 每张图失败自动重试 3 次；有失败的文件留在列表里并标红，可以再次下载。

## 应用图标

图形来自 iconfont「ehl_图片下载」（ID 16273628，<https://www.iconfont.cn/>），放在 macOS 风格的圆角底板上。源图是 `src-tauri/app-icon.svg` / `app-icon.png`，改完后重新生成各尺寸：

```bash
npm run tauri icon src-tauri/app-icon.png
```

## 打包

GitHub Actions（`.github/workflows/build.yml`）：在 Actions 页面手动运行「打包」，或推送 `v*` 标签（同时发布 Release）。

- macOS：Intel / Apple 芯片通用版 DMG，内含 `.app`、`通用安装-去隔离.command` 与安装说明（`scripts/macos/make-dmg.sh`）。
- Windows：便携版 exe，不需要安装，双击运行。依赖系统自带的 WebView2（Windows 11 自带，Windows 10 通常已随 Edge 安装）。

## 开发

```bash
npm install
npm run tauri dev      # 开发运行
npm run tauri build    # 打包
```

Rust 测试：

```bash
cd src-tauri
cargo test --lib
# 用真实文件打印下载计划 / 真实下载
GOODS_FILE=~/Downloads/990498294408.txt cargo test --lib print_plan -- --ignored --nocapture
GOODS_FILE=~/Downloads/990498294408.txt DL_DIR=/tmp/out cargo test --lib real_download -- --ignored --nocapture
```
