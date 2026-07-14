# Stock Widget

Windows 桌面美股当日分钟 K 线小组件（Tauri 2）。

## 功能

- 默认窗口 `400×300`，无边框圆角卡片，可拖动、可调整大小（最小 `320×220`）
- 收起到系统托盘：工具栏 `−` / 关闭窗口 → 隐藏；托盘左键切换显示；右键菜单「显示/隐藏」「退出」
- 当日 `1m` / `5m` 蜡烛图 + 成交量（lightweight-charts）
- 支持 **美股 / 港股 / 韩股**，顶栏 `US` `HK` `KR` 切换
- 输入代码回车切换（美股 `AAPL`、港股 `00700`、韩股 `005930`）
- 指数别名：`KOSPI` / `KOSDAQ`、`HSI`、`DJI` / `NASDAQ` / `SPX`（映射到 Yahoo `^KS11` 等）
- 盘中约 30s 刷新，休市约 5min 刷新（按时区估算各市场交易时段）

## 数据源

- K 线：Yahoo Finance Chart API（OHLC）
  - 美股 `AAPL`，港股 `0700.HK`，韩股 `005930.KS` / `.KQ`
  - 失败时回退腾讯分时接口合成蜡烛（港股/美股较稳，韩股依赖 Yahoo）
- 报价：腾讯 `qt.gtimg.cn`（`us*` / `hk*` / `kr*`），失败时回退 Yahoo meta

## 开发

前置：Node.js、Rust（rustup）、VS 2022 Build Tools（MSVC）、WebView2。

```bash
npm install
npm run tauri dev
```

## 打包

```bash
npm run tauri build
```

安装包输出目录：

`src-tauri/target/release/bundle/nsis/`
