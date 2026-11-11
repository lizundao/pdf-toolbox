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
let loadedPassword = '';
let viewport = null;
/** @type {{ page: number, x: number, y: number, width: number, height: number }[]} */
let regions = [];
let dragging = false;
let dragStart = null;
let dragCurrent = null;

function $(id) { return document.getElementById(id); }

function updateBtn() {
  const btn = $('redactRun');
  if (!btn || !$('loadingOverlay').hidden) { if (btn) btn.disabled = true; return; }
  const hasSource = $('redactSource').value.trim();
  const hasOutput = $('redactOutput').value.trim();
  btn.disabled = !(hasSource && hasOutput && regions.length > 0);
}

function updateRegionList() {
  const list = $('redactRegionList');
  const pageRegions = regions.filter((r) => r.page === currentPage);
  $('redactRegionSummary').textContent = regions.length
    ? `共 ${regions.length} 处涂黑（当前页 ${pageRegions.length} 处）`
    : '在预览上拖拽框选敏感区域';
  if (!regions.length) {
    list.innerHTML = '<li class="hint">暂无涂黑区域</li>';
    return;
  }
  list.innerHTML = regions
    .map((r, i) => `<li>第 ${r.page} 页 · ${Math.round(r.width)}×${Math.round(r.height)} pt <button type="button" class="btn-link" data-remove="${i}">删除</button></li>`)
    .join('');
  list.querySelectorAll('[data-remove]').forEach((btn) => {
    btn.addEventListener('click', () => {
      regions.splice(parseInt(btn.dataset.remove, 10), 1);
      updateRegionList();
      drawOverlay();
      updateBtn();
    });
  });
}

function pdfRectToCanvas(rect) {
  if (!viewport) return null;
  const [vx1, vy1, vx2, vy2] = viewport.convertToViewportRectangle([
    rect.x, rect.y, rect.x + rect.width, rect.y + rect.height,
  ]);
  return { left: vx1, top: vy1, width: vx2 - vx1, height: vy2 - vy1 };
}

function canvasToPdfRect(x1, y1, x2, y2) {
  const [px1, py1] = viewport.convertToPdfPoint(x1, y1);
  const [px2, py2] = viewport.convertToPdfPoint(x2, y2);
  return {
    x: Math.min(px1, px2),
    y: Math.min(py1, py2),
    width: Math.abs(px2 - px1),
    height: Math.abs(py2 - py1),
  };
}

function drawOverlay() {
  const overlay = $('redactOverlay');
  if (!overlay || !viewport) return;
  const ctx = overlay.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, overlay.width, overlay.height);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

  regions.filter((r) => r.page === currentPage).forEach((r) => {
    const box = pdfRectToCanvas(r);
    if (!box) return;
    ctx.fillStyle = 'rgba(0, 0, 0, 0.85)';
    ctx.fillRect(box.left, box.top, box.width, box.height);
    ctx.strokeStyle = 'rgba(220, 50, 50, 0.9)';
    ctx.lineWidth = 2;
    ctx.strokeRect(box.left, box.top, box.width, box.height);
  });

  if (dragging && dragStart && dragCurrent) {
    const x = Math.min(dragStart.x, dragCurrent.x);
    const y = Math.min(dragStart.y, dragCurrent.y);
    const w = Math.abs(dragCurrent.x - dragStart.x);
    const h = Math.abs(dragCurrent.y - dragStart.y);
    ctx.fillStyle = 'rgba(0, 0, 0, 0.35)';
    ctx.fillRect(x, y, w, h);
    ctx.strokeStyle = 'rgba(220, 50, 50, 0.9)';
    ctx.lineWidth = 2;
    ctx.setLineDash([6, 4]);
    ctx.strokeRect(x, y, w, h);
    ctx.setLineDash([]);
  }
}

async function closePdf() {
  if (pdfDoc) {
    try { await pdfDoc.destroy(); } catch { /* ignore */ }
    pdfDoc = null;
  }
  viewport = null;
}

async function renderPage(pageNum) {
  if (!pdfDoc) return;
  const page = await pdfDoc.getPage(pageNum);
  try {
    const canvas = $('redactCanvas');
    const overlay = $('redactOverlay');
    const ctx = canvas.getContext('2d');
    const wrap = $('redactCanvasWrap');
    const maxW = Math.max((wrap?.clientWidth || 0) - 24, 320);
    const dpr = window.devicePixelRatio || 1;
    const baseVp = page.getViewport({ scale: 1 });
    const scale = Math.min(maxW / baseVp.width, 1.8);
    viewport = page.getViewport({ scale });
    canvas.width = Math.floor(viewport.width * dpr);
    canvas.height = Math.floor(viewport.height * dpr);
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;
    overlay.width = Math.floor(viewport.width * dpr);
    overlay.height = Math.floor(viewport.height * dpr);
    overlay.style.width = `${viewport.width}px`;
    overlay.style.height = `${viewport.height}px`;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    await page.render({ canvasContext: ctx, viewport }).promise;
    $('redactPageLabel').textContent = `第 ${pageNum} / ${totalPages} 页`;
    currentPage = pageNum;
    drawOverlay();
  } finally {
    if (typeof page.cleanup === 'function') page.cleanup();
  }
}

async function loadPreview() {
  if (!sourcePath) return;
  $('redactPreviewEmpty').hidden = true;
  $('redactPreviewContent').hidden = false;
  showLoading('加载预览…');
  try {
    await closePdf();
    const bytes = await invoke('read_pdf_file', { sourcePath });
    const pwd = $('redactPassword').value.trim() || undefined;
    pdfDoc = await pdfjsLib.getDocument({ data: new Uint8Array(bytes), password: pwd }).promise;
    loadedPassword = pwd || '';
    totalPages = pdfDoc.numPages;
    currentPage = 1;
    regions = [];
    $('redactPreviewSummary').textContent = `${totalPages} 页 · 拖拽框选涂黑区域`;
    await renderPage(1);
    updateRegionList();
    updateBtn();
  } catch (e) {
    $('redactPreviewEmpty').hidden = false;
    $('redactPreviewContent').hidden = true;
    showError(e.message || String(e), 'redactError', 'redactErrorText', 'redactResult');
  } finally {
    hideLoading();
  }
}

async function openFile(path) {
  await closePdf();
  sourcePath = path;
  $('redactSource').value = path;
  setPdfOutputField('redactOutput', path, '脱敏');
  hideResults('redactResult', 'redactError');
  regions = [];
  $('redactPreviewEmpty').hidden = false;
  $('redactPreviewContent').hidden = true;
  updateRegionList();
  try {
    const pwd = $('redactPassword').value.trim() || null;
    const summary = await invoke('get_pdf_preview_info', { sourcePath: path, password: pwd });
    $('redactFileSummary').textContent = `${fmtSize(summary.file_size)} · ${summary.page_count} 页${summary.encrypted ? ' · 已加密' : ''}`;
    $('redactFileSummary').hidden = false;
  } catch (e) {
    $('redactFileSummary').hidden = true;
    showError(e.message || String(e), 'redactError', 'redactErrorText', 'redactResult');
  }
  updateBtn();
}

function overlayCoords(evt) {
  const overlay = $('redactOverlay');
  const rect = overlay.getBoundingClientRect();
  // 返回的是「视口(CSS)坐标」，与 viewport.convertToPdfPoint 的坐标系一致；
  // 画布物理像素已按 dpr 放大，这里用 CSS 宽度换算，避免 HiDPI 下框选偏移。
  const scaleX = viewport.width / rect.width;
  const scaleY = viewport.height / rect.height;
  return {
    x: (evt.clientX - rect.left) * scaleX,
    y: (evt.clientY - rect.top) * scaleY,
  };
}

export function initRedact() {
  $('redactPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) await openFile(files[0]);
    } catch (e) { showError(e.message, 'redactError', 'redactErrorText', 'redactResult'); }
  });

  $('redactPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('redactSource').value, '脱敏'));
      if (p) { $('redactOutput').value = p; updateBtn(); }
    } catch (e) { showError(e.message, 'redactError', 'redactErrorText', 'redactResult'); }
  });

  $('redactLoadPreview').addEventListener('click', () => {
    loadPreview().catch((e) => showError(e.message || String(e), 'redactError', 'redactErrorText', 'redactResult'));
  });

  $('redactPassword').addEventListener('change', async () => {
    const nextPwd = $('redactPassword').value.trim();
    if (nextPwd === loadedPassword) return;
    if (regions.length && !window.confirm('更改密码会清除已框选的涂黑区域，确定继续吗？')) {
      $('redactPassword').value = loadedPassword;
      return;
    }
    await closePdf();
    regions = [];
    updateRegionList();
    updateBtn();
    $('redactPreviewContent').hidden = true;
    $('redactPreviewEmpty').hidden = false;
  });

  $('redactPrevPage').addEventListener('click', () => {
    if (currentPage > 1) {
      renderPage(currentPage - 1).catch((e) => showError(e.message || String(e), 'redactError', 'redactErrorText', 'redactResult'));
    }
  });
  $('redactNextPage').addEventListener('click', () => {
    if (currentPage < totalPages) {
      renderPage(currentPage + 1).catch((e) => showError(e.message || String(e), 'redactError', 'redactErrorText', 'redactResult'));
    }
  });

  $('redactClearPage').addEventListener('click', () => {
    regions = regions.filter((r) => r.page !== currentPage);
    drawOverlay();
    updateRegionList();
    updateBtn();
  });

  $('redactClearAll').addEventListener('click', () => {
    regions = [];
    drawOverlay();
    updateRegionList();
    updateBtn();
  });

  const overlay = $('redactOverlay');
  overlay.addEventListener('mousedown', (evt) => {
    if (!pdfDoc || evt.button !== 0) return;
    dragging = true;
    dragStart = overlayCoords(evt);
    dragCurrent = dragStart;
    drawOverlay();
  });
  overlay.addEventListener('mousemove', (evt) => {
    if (!dragging) return;
    dragCurrent = overlayCoords(evt);
    drawOverlay();
  });
  const finishDrag = () => {
    if (!dragging || !dragStart || !dragCurrent || !viewport) {
      dragging = false;
      dragStart = null;
      dragCurrent = null;
      return;
    }
    const dist = Math.hypot(dragCurrent.x - dragStart.x, dragCurrent.y - dragStart.y);
    if (dist >= 6) {
      const pdfRect = canvasToPdfRect(dragStart.x, dragStart.y, dragCurrent.x, dragCurrent.y);
      if (pdfRect.width >= 2 && pdfRect.height >= 2) {
        regions.push({ page: currentPage, ...pdfRect });
        updateRegionList();
        updateBtn();
      }
    }
    dragging = false;
    dragStart = null;
    dragCurrent = null;
    drawOverlay();
  };
  overlay.addEventListener('mouseup', finishDrag);
  overlay.addEventListener('mouseleave', finishDrag);

  setupDragDrop('redactDropZone', (paths) => {
    handleDroppedPdfs(paths, async (pdfs) => {
      await openFile(pdfs[0]);
    }, (msg) => showError(msg, 'redactError', 'redactErrorText', 'redactResult'));
  });

  $('redactRun').addEventListener('click', async () => {
    const source = $('redactSource').value.trim();
    const output = $('redactOutput').value.trim();
    hideResults('redactResult', 'redactError');
    if (!(await confirmOverwrite(output))) return;
    showLoading('正在涂黑…');
    updateBtn();
    try {
      const result = await invoke('apply_pdf_redaction', {
        sourcePath: source,
        outputPath: output,
        regions,
        password: $('redactPassword').value.trim() || null,
      });
      showSuccess({
        title: '涂黑完成',
        message: result.message,
        outputPath: result.output_path,
      });
    } catch (e) {
      showError(e.message || String(e), 'redactError', 'redactErrorText', 'redactResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  updateRegionList();
  updateBtn();
}
