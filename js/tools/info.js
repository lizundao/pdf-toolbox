import { invoke } from '@tauri-apps/api/core';
import { pdfjsLib } from '../pdfjs-setup.js';
import {
  confirmOverwrite, defaultPdfOutput, fmtSize, handleDroppedPdfs, hideLoading, hideResults, pickPdf, pickPdfSave,
  setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess,
} from '../common.js';

let pdfDoc = null;
let currentPage = 1;
let totalPages = 0;
let sourcePath = '';

function $(id) { return document.getElementById(id); }

function showResult(msg, outputPath) {
  showSuccess({
    title: '保存完成',
    message: msg,
    outputPath,
  });
}

function setMode(next) {
  document.querySelectorAll('[data-info-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.infoMode === next);
  });
  $('infoPreviewPanel').hidden = next !== 'preview';
  $('infoMetaPanel').hidden = next !== 'meta';
  hideResults('infoResult', 'infoError');
}

async function loadMetadata() {
  if (!sourcePath) return;
  try {
    const pwd = $('infoPassword').value.trim() || null;
    const meta = await invoke('get_pdf_metadata', { sourcePath, password: pwd });
    $('infoTitle').value = meta.title || '';
    $('infoAuthor').value = meta.author || '';
    $('infoSubject').value = meta.subject || '';
    $('infoKeywords').value = meta.keywords || '';
    $('infoMetaStatus').textContent = meta.encrypted ? '（已加密，编辑前需密码）' : '';
  } catch (e) {
    showError(e.message || String(e), 'infoError', 'infoErrorText', 'infoResult');
  }
}

async function renderPage(pageNum) {
  if (!pdfDoc) return;
  const page = await pdfDoc.getPage(pageNum);
  const canvas = $('infoCanvas');
  const ctx = canvas.getContext('2d');
  const wrap = $('infoCanvasWrap');
  const maxW = Math.max(wrap.clientWidth, 320) - 24;
  const base = page.getViewport({ scale: 1 });
  const scale = Math.min(maxW / base.width, 1.8) * (window.devicePixelRatio || 1);
  const scaled = page.getViewport({ scale });
  canvas.width = scaled.width;
  canvas.height = scaled.height;
  try {
    await page.render({ canvasContext: ctx, viewport: scaled }).promise;
  } finally {
    page.cleanup();
  }
  $('infoPageLabel').textContent = `第 ${pageNum} / ${totalPages} 页`;
  currentPage = pageNum;
  document.querySelectorAll('.thumb-item').forEach((el) => {
    el.classList.toggle('active', parseInt(el.dataset.page, 10) === pageNum);
  });
}

async function closePdf() {
  if (pdfDoc) {
    try { await pdfDoc.destroy(); } catch { /* ignore */ }
    pdfDoc = null;
  }
}

let thumbsLoaded = 0;
const THUMB_BATCH = 20;

async function renderThumbnails(reset = true) {
  const strip = $('infoThumbStrip');
  if (reset) {
    strip.innerHTML = '';
    thumbsLoaded = 0;
  }
  if (!pdfDoc) return;
  const end = Math.min(totalPages, thumbsLoaded + THUMB_BATCH);
  for (let i = thumbsLoaded + 1; i <= end; i++) {
    const page = await pdfDoc.getPage(i);
    const vp = page.getViewport({ scale: 0.15 });
    const canvas = document.createElement('canvas');
    canvas.width = vp.width;
    canvas.height = vp.height;
    try {
      await page.render({ canvasContext: canvas.getContext('2d'), viewport: vp }).promise;
    } finally {
      page.cleanup();
    }
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'thumb-item';
    item.dataset.page = String(i);
    item.title = `第 ${i} 页`;
    item.appendChild(canvas);
    item.addEventListener('click', () => renderPage(i).catch((e) => showError(e.message || String(e), 'infoError', 'infoErrorText', 'infoResult')));
    strip.appendChild(item);
  }
  thumbsLoaded = end;
  strip.querySelector('.thumb-more')?.remove();
  if (totalPages > thumbsLoaded) {
    const more = document.createElement('button');
    more.type = 'button';
    more.className = 'thumb-more';
    more.textContent = `+${totalPages - thumbsLoaded} · 加载更多`;
    more.addEventListener('click', () => renderThumbnails(false).catch(() => {}));
    strip.appendChild(more);
  }
}

async function openPreview() {
  if (!sourcePath) return;
  $('infoPreviewEmpty').hidden = true;
  $('infoPreviewContent').hidden = false;
  showLoading('加载预览…');
  try {
    await closePdf();
    const bytes = await invoke('read_pdf_file', { sourcePath });
    const pwd = $('infoPassword').value.trim() || undefined;
    const data = new Uint8Array(bytes);
    const task = pdfjsLib.getDocument({ data, password: pwd });
    pdfDoc = await task.promise;
    totalPages = pdfDoc.numPages;
    $('infoPreviewSummary').textContent = `${totalPages} 页`;
    await renderThumbnails();
    await renderPage(1);
  } catch (e) {
    $('infoPreviewEmpty').hidden = false;
    $('infoPreviewContent').hidden = true;
    showError(e.message || String(e), 'infoError', 'infoErrorText', 'infoResult');
  } finally {
    hideLoading();
  }
}

async function openFile(path) {
  sourcePath = path;
  $('infoSource').value = path;
  setPdfOutputField('infoOutput', path, '元数据');
  hideResults('infoResult', 'infoError');
  await closePdf();
  $('infoPreviewEmpty').hidden = false;
  $('infoPreviewContent').hidden = true;
  try {
    const pwd = $('infoPassword').value.trim() || null;
    const summary = await invoke('get_pdf_preview_info', { sourcePath: path, password: pwd });
    $('infoFileSummary').textContent = `${fmtSize(summary.file_size)} · ${summary.page_count} 页${summary.encrypted ? ' · 已加密' : ''}`;
    $('infoFileSummary').hidden = false;
    await loadMetadata();
  } catch (e) {
    $('infoFileSummary').hidden = true;
    showError(e.message || String(e), 'infoError', 'infoErrorText', 'infoResult');
  }
}

export function initInfo() {
  document.querySelectorAll('[data-info-mode]').forEach((tab) => {
    tab.addEventListener('click', () => setMode(tab.dataset.infoMode));
  });

  $('infoPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) await openFile(files[0]);
    } catch (e) { showError(e.message, 'infoError', 'infoErrorText', 'infoResult'); }
  });

  setupDragDrop('infoDropZone', (paths) => {
    handleDroppedPdfs(paths, async (pdfs) => {
      await openFile(pdfs[0]);
    }, (msg) => showError(msg, 'infoError', 'infoErrorText', 'infoResult'));
  });

  $('infoPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput(sourcePath, '元数据'));
      if (p) $('infoOutput').value = p;
    } catch (e) { showError(e.message, 'infoError', 'infoErrorText', 'infoResult'); }
  });

  $('infoLoadPreview').addEventListener('click', openPreview);
  $('infoRefreshMeta').addEventListener('click', loadMetadata);

  const gotoPage = (n) => {
    if (!pdfDoc || n < 1 || n > totalPages) return;
    renderPage(n).catch((e) => showError(e.message || String(e), 'infoError', 'infoErrorText', 'infoResult'));
  };
  $('infoPrevPage').addEventListener('click', () => { if (currentPage > 1) gotoPage(currentPage - 1); });
  $('infoNextPage').addEventListener('click', () => { if (currentPage < totalPages) gotoPage(currentPage + 1); });
  $('infoGotoPage')?.addEventListener('click', () => gotoPage(parseInt($('infoPageInput').value, 10) || 1));
  $('infoPageInput')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') gotoPage(parseInt($('infoPageInput').value, 10) || 1);
  });

  $('infoSaveMeta').addEventListener('click', async () => {
    if (!sourcePath || !$('infoOutput').value) {
      showError('请选择源文件和输出路径', 'infoError', 'infoErrorText', 'infoResult');
      return;
    }
    hideResults('infoResult', 'infoError');
    showLoading('保存元数据…');
    try {
      const output = $('infoOutput').value.trim();
      if (!(await confirmOverwrite(output))) return;
      const result = await invoke('set_pdf_metadata', {
        sourcePath,
        outputPath: output,
        title: $('infoTitle').value.trim() || null,
        author: $('infoAuthor').value.trim() || null,
        subject: $('infoSubject').value.trim() || null,
        keywords: $('infoKeywords').value.trim() || null,
        password: $('infoPassword').value.trim() || null,
      });
      showResult(result.message, result.output_path);
    } catch (e) {
      showError(e.message || String(e), 'infoError', 'infoErrorText', 'infoResult');
    } finally {
      hideLoading();
    }
  });

  setMode('preview');
}
