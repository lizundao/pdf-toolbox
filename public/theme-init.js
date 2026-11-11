// 预绘制主题：以经典脚本在 <head> 内同步执行，避免深浅色切换时的闪烁（FOUC）。
// 提取为外部文件而非内联，是为了在严格 CSP（script-src 'self'）下仍可执行。
(function () {
  try {
    if (localStorage.getItem('pdf-toolbox-theme') === 'light') {
      document.documentElement.dataset.theme = 'light';
    }
  } catch (e) {
    // 隐私模式 / 受限 WebView 下 localStorage 可能抛错，忽略并使用默认深色主题
  }
})();
