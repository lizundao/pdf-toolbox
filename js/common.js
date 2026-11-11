import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { open as dialogOpen, save as dialogSave } from '@tauri-apps/plugin-dialog';

/** HTML 转义：所有插入 innerHTML 的动态文本都必须先经过此函数 */
export function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function fmtSize(n) {
  if (n >= 1024 * 1024) return (n / 1024 / 1024).toFixed(2) + ' MB';
  if (n >= 1024) return (n / 1024).toFixed(2) + ' KB';
  return n + ' B';
}

export function isPdf(path) {
  return path.toLowerCase().endsWith('.pdf');
}

/**
 * 执行前检查输出文件是否已存在，存在则请求用户确认（避免静默覆盖）。
 * 返回 true=可继续，false=用户取消。检查失败时保守放行交由后端处理。
 */
export async function confirmOverwrite(outputPath) {
  if (!outputPath) return true;
  try {
    const exists = await invoke('check_path_exists', { path: outputPath });
    if (exists) {
      const name = basename(outputPath);
      return window.confirm(`输出文件「${name}」已存在，继续将覆盖它。是否继续？`);
    }
  } catch {
    // 检查失败不阻断流程，后端仍会做源/目标保护
  }
  return true;
}

export function basename(path) {
  return path.split(/[/\\]/).pop();
}

export function parentDir(path) {
  const i = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return i >= 0 ? path.slice(0, i) : '';
}

export function fileStem(path) {
  const name = basename(path);
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name;
}

export function joinPath(dir, name) {
  const sep = dir.includes('\\') ? '\\' : '/';
  return dir.replace(/[/\\]+$/, '') + sep + name;
}

/** 与源文件同目录，文件名：{原名}-{suffix}.{ext} */
export function defaultFileOutput(sourcePath, suffix, ext) {
  if (!sourcePath) return '';
  return joinPath(parentDir(sourcePath), `${fileStem(sourcePath)}-${suffix}.${ext}`);
}

export function defaultPdfOutput(sourcePath, suffix) {
  return defaultFileOutput(sourcePath, suffix, 'pdf');
}

export function defaultOutputDir(sourcePath) {
  return parentDir(sourcePath);
}

export function setPdfOutputField(elementId, sourcePath, suffix) {
  const el = document.getElementById(elementId);
  if (el && sourcePath) el.value = defaultPdfOutput(sourcePath, suffix);
}

export function setOutputDirField(elementId, sourcePath) {
  const el = document.getElementById(elementId);
  if (el && sourcePath) el.value = defaultOutputDir(sourcePath);
}

export function setFileOutputField(elementId, sourcePath, suffix, ext) {
  const el = document.getElementById(elementId);
  if (el && sourcePath) el.value = defaultFileOutput(sourcePath, suffix, ext);
}

const loadingOverlay = () => document.getElementById('loadingOverlay');
const loadingText = () => document.getElementById('loadingText');
const loadingProgress = () => document.getElementById('loadingProgress');
const loadingProgressBar = () => document.getElementById('loadingProgressBar');
const loadingPercent = () => document.getElementById('loadingPercent');

export function showLoading(text = '处理中…', { percent = null } = {}) {
  loadingText().textContent = text;
  const showBar = percent != null;
  loadingProgress().hidden = !showBar;
  loadingPercent().hidden = !showBar;
  if (showBar) {
    loadingProgressBar().style.width = `${percent}%`;
    loadingPercent().textContent = `${percent}%`;
  }
  loadingOverlay().hidden = false;
}

export function updateLoadingProgress(percent, text) {
  if (text) loadingText().textContent = text;
  loadingProgress().hidden = false;
  loadingPercent().hidden = false;
  const pct = Math.max(0, Math.min(100, percent ?? 0));
  loadingProgressBar().style.width = `${pct}%`;
  loadingPercent().textContent = `${pct}%`;
}

export function hideLoading() {
  loadingOverlay().hidden = true;
  loadingProgress().hidden = true;
  loadingPercent().hidden = true;
  loadingProgressBar().style.width = '0%';
}

export function updateInlineProgress(prefix, percent, text) {
  const bar = document.getElementById(`${prefix}Bar`);
  const label = document.getElementById(`${prefix}Label`);
  const wrap = document.getElementById(`${prefix}Progress`);
  if (!wrap || !bar || !label) return;
  wrap.hidden = false;
  const pct = Math.max(0, Math.min(100, percent ?? 0));
  bar.style.width = `${pct}%`;
  label.textContent = text ? `${pct}% · ${text}` : `${pct}%`;
}

export function hideInlineProgress(prefix) {
  const wrap = document.getElementById(`${prefix}Progress`);
  const bar = document.getElementById(`${prefix}Bar`);
  if (wrap) wrap.hidden = true;
  if (bar) bar.style.width = '0%';
}

export function hideResults(resultId = 'resultSection', errorId = 'errorSection') {
  const r = document.getElementById(resultId);
  const e = document.getElementById(errorId);
  if (r) r.hidden = true;
  if (e) e.hidden = true;
}

let resultDialogOutputPath = null;
let resultDialogLastFocus = null;

function closeResultDialog() {
  const dialog = document.getElementById('resultDialog');
  if (dialog) dialog.hidden = true;
  resultDialogOutputPath = null;
  // 焦点归还触发元素，避免键盘用户丢失位置
  if (resultDialogLastFocus && typeof resultDialogLastFocus.focus === 'function') {
    resultDialogLastFocus.focus();
  }
  resultDialogLastFocus = null;
}

let globalProgressBound = false;

/** 全局监听后端 task-progress 事件，批量任务进度统一在遮罩中呈现。 */
export function initGlobalProgress() {
  if (globalProgressBound) return;
  globalProgressBound = true;
  listen('task-progress', (event) => {
    const { percent, message } = event.payload ?? {};
    if (!$('loadingOverlay')?.hidden) {
      updateLoadingProgress(percent, message);
    }
  }).catch(() => {});
  // 兜底：捕获未处理的 Promise 拒绝，避免静默失败
  window.addEventListener('unhandledrejection', (e) => {
    try { showToast(e.reason?.message || String(e.reason || '未知错误'), { type: 'error' }); } catch { /* ignore */ }
  });
}

const $ = (id) => document.getElementById(id);

export function initNotifications() {
  const dialog = document.getElementById('resultDialog');
  if (!dialog) return;

  document.getElementById('resultDialogOk')?.addEventListener('click', closeResultDialog);
  dialog.addEventListener('click', (e) => {
    if (e.target === dialog) closeResultDialog();
  });
  document.addEventListener('keydown', (e) => {
    if (dialog.hidden) return;
    if (e.key === 'Escape') { closeResultDialog(); return; }
    // 焦点约束在弹窗内（Tab 循环），符合 aria-modal 语义
    if (e.key === 'Tab') {
      const focusable = [...dialog.querySelectorAll('button:not([hidden]):not(:disabled)')]
        .filter((el) => el.offsetParent !== null);
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  });

  document.getElementById('resultDialogReveal')?.addEventListener('click', async () => {
    if (!resultDialogOutputPath) return;
    try {
      await invoke('reveal_path_in_folder', { path: resultDialogOutputPath });
    } catch (err) {
      showToast(err.message || String(err), { type: 'error' });
    }
  });

  document.getElementById('resultDialogCopy')?.addEventListener('click', async () => {
    if (!resultDialogOutputPath) return;
    try {
      await navigator.clipboard.writeText(resultDialogOutputPath);
      showToast('路径已复制', { type: 'info', duration: 2000 });
    } catch {
      showToast('复制失败', { type: 'error' });
    }
  });
}

export function showToast(message, { type = 'info', duration = 5000 } = {}) {
  const stack = document.getElementById('toastStack');
  if (!stack) return;
  const el = document.createElement('div');
  el.className = `toast toast-${type}`;
  el.textContent = message;
  stack.appendChild(el);
  requestAnimationFrame(() => el.classList.add('show'));
  setTimeout(() => {
    el.classList.remove('show');
    setTimeout(() => el.remove(), 280);
  }, duration);
}

/**
 * 成功结果弹窗
 * @param {{ title?: string, message?: string, outputPath?: string, detailsHtml?: string }} opts
 */
export function showSuccess({ title = '处理完成', message = '', outputPath, detailsHtml } = {}) {
  resultDialogLastFocus = document.activeElement;
  resultDialogOutputPath = outputPath?.trim() || null;

  document.getElementById('resultDialogTitle').textContent = title;
  document.getElementById('resultDialogMessage').textContent = message;
  document.getElementById('resultDialogMessage').hidden = !message;

  const pathWrap = document.getElementById('resultDialogPathWrap');
  const pathEl = document.getElementById('resultDialogPath');
  if (resultDialogOutputPath) {
    pathEl.textContent = resultDialogOutputPath;
    pathEl.title = resultDialogOutputPath;
    pathWrap.hidden = false;
  } else {
    pathWrap.hidden = true;
  }

  const detailsEl = document.getElementById('resultDialogDetails');
  if (detailsHtml) {
    detailsEl.innerHTML = detailsHtml;
    detailsEl.hidden = false;
  } else {
    detailsEl.innerHTML = '';
    detailsEl.hidden = true;
  }

  const hasPath = !!resultDialogOutputPath;
  document.getElementById('resultDialogReveal').hidden = !hasPath;
  document.getElementById('resultDialogCopy').hidden = !hasPath;
  document.getElementById('resultDialog').hidden = false;
  // 打开后把焦点移入弹窗，供键盘用户直接操作
  document.getElementById('resultDialogOk')?.focus();
}

export function showError(msg, errorId = 'errorSection', errorTextId = 'errorText', resultId = 'resultSection') {
  const r = document.getElementById(resultId);
  const e = document.getElementById(errorId);
  if (r) r.hidden = true;
  // 真正渲染错误卡片（此前 errorTextId 从未使用，错误只在 toast 闪现后无法回看）
  const text = document.getElementById(errorTextId);
  if (text) {
    text.textContent = msg;
    if (e) e.hidden = false;
  }
  showToast(msg, { type: 'error' });
}

export async function pickPdf(multiple = false) {
  const selected = await dialogOpen({
    multiple,
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  });
  if (!selected) return null;
  return Array.isArray(selected) ? selected : [selected];
}

export async function pickPdfSave(defaultPath) {
  const opts = { filters: [{ name: 'PDF', extensions: ['pdf'] }] };
  if (defaultPath) opts.defaultPath = defaultPath;
  return dialogSave(opts);
}

export async function pickDirectory() {
  return dialogOpen({ directory: true, multiple: false });
}

export async function pickImages(multiple = true) {
  const selected = await dialogOpen({
    multiple,
    filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp', 'gif'] }],
  });
  if (!selected) return null;
  return Array.isArray(selected) ? selected : [selected];
}

export async function pickTextFile() {
  return dialogOpen({
    multiple: false,
    filters: [{ name: '文本', extensions: ['txt', 'md', 'log'] }],
  });
}

export async function pickTextSave(defaultPath) {
  const opts = { filters: [{ name: '文本', extensions: ['txt'] }] };
  if (defaultPath) opts.defaultPath = defaultPath;
  return dialogSave(opts);
}

export async function pickFontFile() {
  return dialogOpen({
    multiple: false,
    filters: [{
      name: '字体',
      extensions: ['ttf', 'ttc', 'otf'],
    }],
  });
}

export async function pickOfficeFile() {
  const selected = await dialogOpen({
    multiple: false,
    filters: [{
      name: 'Office 文档',
      extensions: ['doc', 'docx', 'docm', 'xls', 'xlsx', 'xlsm', 'ppt', 'pptx', 'pptm', 'rtf', 'csv'],
    }],
  });
  return selected || null;
}

const OFFICE_EXT = /\.(docx?|docm|xlsx?|xlsm|pptx?|pptm|rtf|csv)$/i;

export function isOffice(path) {
  return OFFICE_EXT.test(path);
}

export function handleDroppedOffice(paths, onValid, onReject) {
  const files = paths.filter(isOffice);
  const rejected = paths.length - files.length;
  if (files.length === 0) {
    onReject?.('请拖放 Office 文档（Word / Excel / PowerPoint）');
    return;
  }
  onValid(files);
  if (rejected > 0) onReject?.(`已忽略 ${rejected} 个非 Office 文件`);
}

// 窗口级拖放只注册一次，按当前激活视图分发，避免每个模块各注册一个全局监听导致串扰/泄漏。
const dropHandlers = new Map(); // dropZoneId -> onFiles
let windowDragBound = false;

function bindWindowDragOnce() {
  if (windowDragBound) return;
  windowDragBound = true;
  getCurrentWindow().onDragDropEvent((event) => {
    const { type, paths } = event.payload;
    // 当前可见的拖放区（即当前激活视图内那个）
    const activeView = document.querySelector('.view.active');
    const zone = activeView?.querySelector('.drop-zone');
    const over = () => zone?.classList.add('drag-over');
    const leave = () => document.querySelectorAll('.drag-over').forEach((el) => el.classList.remove('drag-over'));
    if (type === 'enter' || type === 'over') over();
    else if (type === 'leave') leave();
    else if (type === 'drop' && paths?.length) {
      leave();
      const id = zone?.id;
      const handler = id ? dropHandlers.get(id) : null;
      if (handler) handler(paths);
    }
  });
}

/** 绑定拖放区域：DOM 级仅做视觉反馈，窗口级监听全局只注册一次并按视图分发 */
export function setupDragDrop(dropZoneId, onFiles) {
  const zone = document.getElementById(dropZoneId);
  if (!zone) return;
  dropHandlers.set(dropZoneId, onFiles);

  const over = () => zone.classList.add('drag-over');
  const leave = () => zone.classList.remove('drag-over');
  zone.addEventListener('dragenter', (e) => { e.preventDefault(); over(); });
  zone.addEventListener('dragover', (e) => { e.preventDefault(); over(); });
  zone.addEventListener('dragleave', leave);
  zone.addEventListener('drop', (e) => { e.preventDefault(); leave(); });

  bindWindowDragOnce();
}

export function handleDroppedPdfs(paths, onValid, onReject) {
  const pdfs = paths.filter(isPdf);
  const rejected = paths.length - pdfs.length;
  if (pdfs.length === 0) {
    onReject?.('请拖放 PDF 文件（.pdf）');
    return;
  }
  onValid(pdfs);
  if (rejected > 0) onReject?.(`已忽略 ${rejected} 个非 PDF 文件`);
}

const IMAGE_EXT = /\.(png|jpe?g|webp|bmp|gif)$/i;

export function isImage(path) {
  return IMAGE_EXT.test(path);
}

export function handleDroppedImages(paths, onValid, onReject) {
  const images = paths.filter(isImage);
  const rejected = paths.length - images.length;
  if (images.length === 0) {
    onReject?.('请拖放图片文件（png/jpg/webp 等）');
    return;
  }
  onValid(images);
  if (rejected > 0) onReject?.(`已忽略 ${rejected} 个非图片文件`);
}
