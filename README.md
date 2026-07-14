# Stock Widget

Windows 桌面股票小组件：当日分钟 K 线 + 实时报价，无边框悬浮窗，可收起到系统托盘。

技术栈：**Tauri 2** · **TypeScript** · **Vite** · **lightweight-charts** · **Rust**。

## 功能

- 无边框圆角卡片窗口（默认 `400×300`，最小 `320×220`），可拖动、可调整大小
- 系统托盘：工具栏 `−` / 关闭窗口 → 隐藏；托盘左键切换显示；右键「显示/隐藏」「退出」
- 当日 `1m` / `5m` 蜡烛图 + 成交量
- 市场切换：美股 `US` / 港股 `HK` / 韩股 `KR`
- 输入代码回车切换（例：`AAPL`、`00700`、`005930`）
- 指数别名：`DJI` / `NASDAQ` / `SPX`、`HSI`、`KOSPI` / `KOSDAQ`
- 设置面板：深色/浅色主题、绿涨红跌 / 红涨绿跌、窗口置顶
- 窗口位置、大小与偏好持久化
- 盘中约 30s 刷新，休市约 5min（按各市场时区估算交易时段）

## 数据源

| 用途 | 主源 | 回退 |
| --- | --- | --- |
| K 线 | Yahoo Finance Chart API | 腾讯分时接口合成蜡烛 |
| 报价 | 腾讯 `qt.gtimg.cn` | Yahoo meta |

代码映射示例：美股 `AAPL`，港股 `0700.HK`，韩股 `005930.KS` / `.KQ`。韩股 K 线主要依赖 Yahoo。

## 环境要求

- Node.js 18+
- Rust（[rustup](https://rustup.rs/)）
- Windows：VS 2022 Build Tools（MSVC）+ WebView2

## 开发

```bash
npm install
npm run tauri dev
```

## 打包

```bash
npm run tauri build
```

安装包输出：

```
src-tauri/target/release/bundle/nsis/
```

## 目录结构

```
src/                 前端（Vite + TypeScript）
  main.ts            UI、设置、轮询刷新
  chart.ts           lightweight-charts 封装
  styles.css         主题与布局
src-tauri/           Tauri / Rust 后端
  src/lib.rs         命令、托盘、设置读写
  src/models/        行情模型与代码解析
  src/providers/     Yahoo / 腾讯数据源与回退
```

## 免责声明

本工具仅供个人研究参考，不构成投资建议。行情数据可能延迟、缺失或不准确，请自行判断投资风险。
