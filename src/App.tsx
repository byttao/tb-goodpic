import { useState } from "react";
import { DownloadOutlined } from "@ant-design/icons";
import { AppShell, ThemeMenu, ThemeToggle } from "./ui-kit";
import DownloadPage from "./pages/DownloadPage";

type PageKey = "download";

export default function App() {
  const [page, setPage] = useState<PageKey>("download");
  return (
    <AppShell
      brand={{ mark: "图", name: "淘宝图片下载", subtitle: "商品图片批量下载" }}
      nav={[{ key: "download", label: "图片下载", icon: <DownloadOutlined />, section: "商品图片" }]}
      active={page}
      onNavigate={setPage}
      sidebarFooter={
        <>
          <span>版本 V0.1.0</span>
          <span className="flex items-center gap-1">
            <ThemeMenu />
            <ThemeToggle />
          </span>
        </>
      }
    >
      <DownloadPage />
    </AppShell>
  );
}
