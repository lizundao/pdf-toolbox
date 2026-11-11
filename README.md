# PDF 工具箱

基于 Tauri 2 的 PDF 多功能桌面工具：左侧边栏 + 窗口菜单双导航，Rust 后端处理 PDF 逻辑。

> **免费 · 离线 · 开源（MIT）** · 官网：[inkdot.cn](https://inkdot.cn)
> 文件全程在本地处理，无账号、无遥测、不收集、不上传任何数据。

## 已上线功能

| 工具 | 功能 |
|------|------|
| **压缩** | 单文件 / 批量结构流压缩，拖放上传 |
| **扫描增强** | 内嵌图片 JPEG 重压缩、降分辨率、转灰度 |
| **格式转换** | 图片↔PDF、PDF↔文本、Office→PDF（Windows） |
| **页面工具** | 合并、拆分、重排、插入、裁剪、旋转、去空白页等 |
| **水印版式** | 文字/图片水印、页眉页脚、页码 |
| **安全隐私** | 加密、解密、权限控制、清除元数据 |
| **脱敏涂黑** | 预览框选敏感区域，输出涂黑 PDF |
| **预览信息** | 文档预览、缩略图、元数据编辑 |

## 技术栈

- **前端**：原生 JS 模块化 + Vite 6 + PDF.js，侧边栏 Shell 布局
- **后端**：Rust + Tauri 2 + [lopdf](https://docs.rs/lopdf) + [image](https://docs.rs/image)
- **模块**：`compress` / `scan` / `convert` / `pages` / `watermark` / `security` / `info` / `pdf_util`

## 环境要求

- Node.js 18+
- Rust 1.70+
- [Tauri 2 前置要求](https://v2.tauri.app/start/prerequisites/)

## 安装与运行

```bash
npm install
npm run tauri dev    # 开发
npm run release:portable  # 编译 + 便携版 zip（推荐发布）
npm run release:zip       # 仅打 zip（需先 release）
```

打包产物：

- **便携版**：`release/PDF工具箱-1.0.0-便携版.zip`（含 exe + 用户手册）
- 安装程序：`src-tauri/target/release/bundle/nsis/PDF Toolbox_1.0.0_x64-setup.exe`

用户文档 [docs/用户手册.md](docs/用户手册.md) · 发布说明 [docs/发布说明.md](docs/发布说明.md)

## 项目结构

```
pdf/
├── index.html              # 应用壳（侧边栏 + 多视图）
├── css/app.css             # 全局样式
├── js/
│   ├── app.js              # 路由与视图切换
│   ├── menubar.js          # 自定义标题栏与窗口控制
│   ├── theme.js            # 主题切换
│   ├── pdfjs-setup.js      # PDF.js 共用初始化
│   ├── common.js           # 对话框、loading、拖放、默认输出路径
│   └── tools/
│       ├── compress.js     # 压缩工具
│       ├── scan.js         # 扫描增强
│       ├── convert.js      # 格式转换
│       ├── pages.js        # 页面工具
│       ├── watermark.js    # 水印版式
│       ├── security.js     # 安全隐私
│       └── info.js         # 预览信息
├── docs/
│   ├── 开发计划.md
│   ├── 功能全景.md
│   └── 发布说明.md
└── src-tauri/src/
    ├── lib.rs              # Tauri 入口
    ├── fonts.rs            # 系统字体枚举与嵌入
    ├── shell_util.rs       # 打开文件所在位置
    ├── compress.rs         # 压缩命令
    ├── scan.rs             # 扫描增强命令
    ├── convert.rs          # 格式转换命令
    ├── pages.rs            # 页面操作命令
    ├── watermark.rs        # 水印版式命令
    ├── security.rs       # 安全隐私命令
    ├── info.rs           # 预览与元数据命令
    └── pdf_util.rs         # PDF 共用逻辑
```

## 文档

- [开发计划](docs/开发计划.md) — 分阶段路线图
- [功能全景](docs/功能全景.md) — 全部规划功能清单
- [发布说明](docs/发布说明.md) — 用户安装与使用指南

## 许可证与合规

- **本项目**：[MIT License](LICENSE) © inkdot
- **第三方组件**：PDF.js（Apache-2.0）、lopdf（MIT）、Tauri（Apache-2.0 OR MIT）等，详见 [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)
- **隐私承诺**：完全离线运行，不收集、不上传、不追踪任何数据
