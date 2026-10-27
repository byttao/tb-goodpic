import { useEffect, useState } from "react";
import { FileTextOutlined, FolderOpenOutlined } from "@ant-design/icons";
import { App as AntApp, Button, Empty, Form, InputNumber, Radio, Typography } from "antd";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  DropZone,
  FileRow,
  ListWell,
  LogViewer,
  Page,
  Panel,
  ProgressPanel,
  Workspace,
  fileName,
  useBatchProgress,
  useFileDrop,
  useLogger,
  type BatchProgress,
} from "../ui-kit";

const EXTENSIONS = ["txt", "json"];
const SETTINGS_KEY = "tb-goodpic:settings";

interface FileInfo {
  path: string;
  ok: boolean;
  error: string | null;
  goodsId: string;
  goodsName: string;
  mainCount: number;
  skuCount: number;
  detailCount: number;
}

interface DownloadResult {
  total: number;
  succeeded: number;
  failed: { source: string; label: string; url: string; error: string }[];
  failedSources: string[];
  folders: string[];
}

type ProgressEvent = BatchProgress & { ok: boolean; error: string | null };
type SaveMode = "source" | "custom";

interface Settings {
  mode: SaveMode;
  outDir: string;
  threads: number;
}

function loadSettings(): Settings {
  const fallback: Settings = { mode: "source", outDir: "", threads: 8 };
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") };
  } catch {
    return fallback;
  }
}

const imageCount = (f: FileInfo) => f.mainCount + f.skuCount + f.detailCount;

export default function DownloadPage() {
  const [files, setFiles] = useState<FileInfo[]>([]);
  const [failed, setFailed] = useState<Set<string>>(new Set());
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [folders, setFolders] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const { logs, log } = useLogger();
  const { message } = AntApp.useApp();

  useEffect(() => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  }, [settings]);

  const { progress, start, finish } = useBatchProgress("download-progress", (e) => {
    const event = e as ProgressEvent;
    if (event.ok) log(`已保存 ${event.file_name}`, "success");
    else log(`${event.file_name} 下载失败：${event.error}`, "error");
  });

  const add = async (paths: string[]) => {
    const known = new Set(files.map((f) => f.path));
    const fresh = paths.filter((p) => !known.has(p));
    if (!fresh.length) return;
    const infos = await invoke<FileInfo[]>("inspect_files", { paths: fresh });
    const valid = infos.filter((f) => f.ok);
    for (const f of infos.filter((f) => !f.ok)) log(`${fileName(f.path)} 无法读取：${f.error}`, "warning");
    if (!valid.length) {
      message.warning("没有可用的商品文件");
      return;
    }
    setFiles((prev) => [...prev, ...valid]);
    setFailed(new Set());
    for (const f of valid) {
      log(`已添加 ${fileName(f.path)}：商品 ${f.goodsId}，主图 ${f.mainCount} · SKU 图 ${f.skuCount} · 详情图 ${f.detailCount}`);
    }
  };

  const { hovering } = useFileDrop(add, { extensions: EXTENSIONS, enabled: !running });

  const browse = async () => {
    const picked = await open({ multiple: true, filters: [{ name: "商品导出文件", extensions: EXTENSIONS }] });
    if (picked) await add(Array.isArray(picked) ? picked : [picked]);
  };

  const pickDir = async () => {
    const dir = await open({ directory: true, defaultPath: settings.outDir || undefined });
    if (typeof dir === "string") setSettings((s) => ({ ...s, mode: "custom", outDir: dir }));
  };

  const clear = () => {
    setFiles([]);
    setFailed(new Set());
  };

  const total = files.reduce((sum, f) => sum + imageCount(f), 0);
  const needsDir = settings.mode === "custom" && !settings.outDir;

  const download = async () => {
    setRunning(true);
    start(total);
    log(`开始下载 ${files.length} 个商品，共 ${total} 张图片，同时下载 ${settings.threads} 张`, "processing");
    try {
      const result = await invoke<DownloadResult>("download_images", {
        files: files.map((f) => f.path),
        outDir: settings.mode === "custom" ? settings.outDir : null,
        threads: settings.threads,
      });
      const failedSources = new Set(result.failedSources);
      setFiles((prev) => prev.filter((f) => failedSources.has(f.path)));
      setFailed(failedSources);
      setFolders(result.folders);
      for (const folder of result.folders) log(`保存位置：${folder}`);
      if (result.failed.length) {
        log(`下载结束：成功 ${result.succeeded} 张，失败 ${result.failed.length} 张。失败的文件留在列表里，可以再次下载。`, "warning");
        message.warning(`${result.failed.length} 张图片下载失败`);
        finish("部分失败");
      } else {
        log(`下载完成：共 ${result.succeeded} 张`, "success");
        message.success(`已下载 ${result.succeeded} 张图片`);
        finish("完成");
      }
    } catch (err) {
      log(`下载失败：${err}`, "error");
      finish("失败", false);
    } finally {
      setRunning(false);
    }
  };

  return (
    <Page>
      <Workspace
        aside={
          <>
            <Panel title="保存位置">
              <Radio.Group
                orientation="vertical"
                value={settings.mode}
                disabled={running}
                onChange={(e) => setSettings((s) => ({ ...s, mode: e.target.value }))}
                options={[
                  { value: "source", label: "源文件所在文件夹" },
                  { value: "custom", label: "自定义文件夹" },
                ]}
              />
              {settings.mode === "custom" && (
                <div className="flex flex-col gap-2">
                  <Button icon={<FolderOpenOutlined />} disabled={running} onClick={pickDir}>
                    {settings.outDir ? "更改文件夹…" : "选择文件夹…"}
                  </Button>
                  {settings.outDir && (
                    <Typography.Text type="secondary" className="break-all text-sm">
                      {settings.outDir}
                    </Typography.Text>
                  )}
                </div>
              )}
              <Typography.Text type="secondary" className="text-sm">
                每个商品新建「goodsID」文件夹，内含主图、SKU图、详情图三个子文件夹。
              </Typography.Text>
            </Panel>

            <Panel title="下载设置">
              <Form layout="vertical" className="m-0">
                <Form.Item label="同时下载" extra="1–32 张，网络慢时调小" className="m-0">
                  <InputNumber
                    min={1}
                    max={32}
                    value={settings.threads}
                    disabled={running}
                    suffix="张"
                    className="w-full"
                    onChange={(v) => setSettings((s) => ({ ...s, threads: v ?? 8 }))}
                  />
                </Form.Item>
              </Form>
            </Panel>

            <div className="mt-auto flex flex-col gap-3">
              <ProgressPanel progress={progress} />
              {folders.length > 0 && !running && (
                <Button block icon={<FolderOpenOutlined />} onClick={() => revealItemInDir(folders)}>
                  打开保存位置
                </Button>
              )}
              <Button
                type="primary"
                block
                loading={running}
                disabled={!files.length || needsDir}
                onClick={download}
              >
                {running ? "正在下载" : `开始下载 · ${total} 张`}
              </Button>
            </div>
          </>
        }
      >
        <Panel
          fill
          className="flex-1"
          title="商品文件"
          extra={
            <Button type="link" size="small" disabled={!files.length || running} onClick={clear}>
              清空列表
            </Button>
          }
        >
          <DropZone
            active={hovering}
            compact={files.length > 0}
            disabled={running}
            onBrowse={browse}
            icon={<FileTextOutlined />}
            title="拖入 txt / json 文件，或点击选择"
            hint="小强上货等工具导出的商品数据，可一次添加多个"
          />
          <ListWell>
            {files.length === 0 ? (
              <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="添加文件后显示在这里" className="py-9" />
            ) : (
              <ul className="m-0 list-none p-0">
                {files.map((f, i) => (
                  <FileRow
                    key={f.path}
                    index={i + 1}
                    title={f.path}
                    status={failed.has(f.path) ? "error" : undefined}
                    trailing={
                      <span className="shrink-0 text-sm tabular-nums text-text-secondary">
                        主图 {f.mainCount} · SKU {f.skuCount} · 详情 {f.detailCount}
                      </span>
                    }
                  >
                    <span className="tabular-nums">{f.goodsId}</span>
                    <span className="ml-2 text-text-secondary">{f.goodsName || fileName(f.path)}</span>
                  </FileRow>
                ))}
              </ul>
            )}
          </ListWell>
        </Panel>
      </Workspace>

      <Panel fill title="下载日志" size="small" className="h-[170px] shrink-0">
        <LogViewer logs={logs} />
      </Panel>
    </Page>
  );
}
