# 第三方组件与许可证声明

PDF 工具箱（以下简称"本软件"）依赖以下开源组件。本软件自身采用 [MIT License](LICENSE)，下列组件的版权与许可归各自作者所有，其许可证文本可在对应项目中查阅。

## 运行时依赖

| 组件 | 版本 | 许可证 | 用途 | 来源 |
|---|---|---|---|---|
| PDF.js (pdfjs-dist) | 4.10.38 | Apache-2.0 | PDF 预览与导出图片 | https://www.npmjs.com/package/pdfjs-dist |
| Tauri | 2.x | Apache-2.0 OR MIT | 桌面应用框架 | https://crates.io/crates/tauri |
| tauri-plugin-dialog | 2.x | Apache-2.0 OR MIT | 系统文件对话框 | https://crates.io/crates/tauri-plugin-dialog |
| lopdf | 0.39 | MIT | PDF 解析与编辑内核 | https://crates.io/crates/lopdf |
| image | 0.25 | MIT OR Apache-2.0 | 图片编码 / 解码 | https://crates.io/crates/image |
| serde / serde_json | 1.x | MIT OR Apache-2.0 | 数据序列化 | https://crates.io/crates/serde |
| chrono | 0.4 | MIT OR Apache-2.0 | 日期时间处理 | https://crates.io/crates/chrono |
| ttf-parser | 0.25 | MIT OR Apache-2.0 | TrueType 字体解析 | https://crates.io/crates/ttf-parser |
| subsetter | 0.2.6 | MIT OR Apache-2.0 | 字体子集化（中文水印嵌入） | https://crates.io/crates/subsetter |
| encoding_rs | 0.8 | (Apache-2.0 OR MIT) AND BSD-3-Clause | UTF-8 / GBK / BIG5 编码解码 | https://crates.io/crates/encoding_rs |
| rand | 0.9 | MIT OR Apache-2.0 | 密码学随机数（加密密钥） | https://crates.io/crates/rand |

## 构建与开发依赖

| 组件 | 版本 | 许可证 | 用途 | 来源 |
|---|---|---|---|---|
| Vite | 6.x | MIT | 前端构建 | https://www.npmjs.com/package/vite |
| @tauri-apps/cli | 2.x | Apache-2.0 OR MIT | Tauri 构建命令行 | https://www.npmjs.com/package/@tauri-apps/cli |

## Apache-2.0 合规说明

PDF.js 采用 Apache License 2.0。根据该许可证第 4 条，分发时须保留版权、许可与免责声明。PDF.js 的许可证全文随 npm 包提供（`node_modules/pdfjs-dist/LICENSE`），亦可在 https://github.com/mozilla/pdf.js/blob/master/LICENSE 查阅。以 Apache-2.0 OR MIT 双授权的组件，本软件按 MIT 条款使用。

## 品牌素材

本软件内 "墨点 / inkdot" 品牌标识与图标由项目作者提供，不属于第三方组件。

---

生成日期：2026-11-11
