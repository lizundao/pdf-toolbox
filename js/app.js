import { listen } from '@tauri-apps/api/event';
import { initTheme } from './theme.js';
import { initGlobalProgress, initNotifications } from './common.js';
import { initInfo } from './tools/info.js';
import { initSecurity } from './tools/security.js';
import { initRedact } from './tools/redact.js';
import { initWatermark } from './tools/watermark.js';
import { initCompress } from './tools/compress.js';
import { initConvert } from './tools/convert.js';
import { initPages } from './tools/pages.js';
import { initScan } from './tools/scan.js';
import { initMenubar } from './menubar.js';

const views = ['home', 'compress', 'scan', 'convert', 'pages', 'watermark', 'security', 'redact', 'info', 'about', 'license'];
let inited = { compress: false, scan: false, convert: false, pages: false, watermark: false, security: false, redact: false, info: false };

export function navigate(view) {
  if (!views.includes(view)) return;

  document.querySelectorAll('[data-view]').forEach((btn) => {
    if (btn.dataset.view) {
      btn.classList.toggle('active', btn.dataset.view === view);
    }
  });
  document.querySelectorAll('.view').forEach((el) => {
    el.classList.toggle('active', el.id === `view-${view}`);
  });

  if (view === 'compress' && !inited.compress) {
    initCompress();
    inited.compress = true;
  }
  if (view === 'scan' && !inited.scan) {
    initScan();
    inited.scan = true;
  }
  if (view === 'convert' && !inited.convert) {
    initConvert();
    inited.convert = true;
  }
  if (view === 'pages' && !inited.pages) {
    initPages();
    inited.pages = true;
  }
  if (view === 'watermark' && !inited.watermark) {
    initWatermark();
    inited.watermark = true;
  }
  if (view === 'security' && !inited.security) {
    initSecurity();
    inited.security = true;
  }
  if (view === 'redact' && !inited.redact) {
    initRedact();
    inited.redact = true;
  }
  if (view === 'info' && !inited.info) {
    initInfo();
    inited.info = true;
  }
}

document.querySelectorAll('[data-view]').forEach((btn) => {
  btn.addEventListener('click', () => navigate(btn.dataset.view));
});

document.querySelectorAll('[data-goto]').forEach((card) => {
  const activate = () => {
    if (!card.classList.contains('soon')) navigate(card.dataset.goto);
  };
  card.addEventListener('click', activate);
  // 卡片语义为按钮，补键盘操作（Enter / 空格）以支持纯键盘导航
  card.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
      e.preventDefault();
      activate();
    }
  });
});

listen('navigate', (event) => {
  navigate(event.payload);
}).catch(() => {});

// 版本号单一来源（vite define 注入 package.json.version），回退到 HTML 中的静态值
if (typeof __APP_VERSION__ !== 'undefined') {
  document.querySelectorAll('[data-app-version]').forEach((el) => { el.textContent = __APP_VERSION__; });
}

initTheme();
initNotifications();
initGlobalProgress();
initMenubar(navigate);
navigate('home');
