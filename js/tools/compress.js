import { invoke } from '@tauri-apps/api/core';
import {
  basename, confirmOverwrite, defaultPdfOutput, escapeHtml, fmtSize, handleDroppedPdfs, hideLoading, hideResults,
  pickDirectory, pickPdf, pickPdfSave, setOutputDirField, setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess,
} from '../common.js';

let mode = 'single';
let batchFiles = [];

const els = {};

function $(id) {
  return document.getElementById(id);
}

function showSingleResult(result, outputPath) {
  showSuccess({
    title: '压缩完成',
    message: `节省 ${fmtSize(result.saved_bytes)}（约 ${result.percent_reduced.toFixed(1)}%）`,
    outputPath,
    detailsHtml: [
      '<ul>',
      `<li>原始大小：${fmtSize(result.original_size)}</li>`,
      `<li>压缩后：${fmtSize(result.new_size)}</li>`,
      '</ul>',
    ].join(''),
  });
}

function showBatchResult(batch, outputDir) {
  const lines = [`<li class="summary">共 ${batch.total} 个，成功 ${batch.succeeded}，失败 ${batch.failed}</li>`];
  for (const item of batch.items) {
    const name = escapeHtml(basename(item.source_path));
    if (item.success) {
      const r = item.result;
      lines.push(`<li class="ok">${name}：${fmtSize(r.original_size)} → ${fmtSize(r.new_size)}（-${r.percent_reduced.toFixed(1)}%）</li>`);
    } else {
      lines.push(`<li class="fail">${name}：${escapeHtml(item.error)}</li>`);
    }
  }
  showSuccess({
    title: '批量压缩完成',
    message: `成功 ${batch.succeeded} 个，失败 ${batch.failed} 个`,
    outputPath: outputDir,
    detailsHtml: `<ul>${lines.join('')}</ul>`,
  });
}

function setMode(next) {
  mode = next;
  document.querySelectorAll('[data-compress-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.compressMode === next);
  });
  $('compressSingle').hidden = next !== 'single';
  $('compressBatch').hidden = next !== 'batch';
  hideResults('compressResult', 'compressError');
  updateBtn();
}

function updateBatchList() {
  $('compressFileCount').textContent = batchFiles.length;
  $('compressFileList').innerHTML = batchFiles.map((p) => `<li>${escapeHtml(basename(p))}</li>`).join('');
  updateBtn();
}

function updateBtn() {
  const btn = $('compressRun');
  if (!btn) return;
  if (!$('loadingOverlay').hidden) { btn.disabled = true; return; }
  if (mode === 'single') {
    btn.disabled = !$('compressSource').value || !$('compressOutput').value;
  } else {
    btn.disabled = batchFiles.length === 0 || !$('compressOutputDir').value;
  }
}

export function initCompress() {
  $('compressRatio').addEventListener('input', () => {
    $('compressRatioVal').textContent = $('compressRatio').value;
  });

  document.querySelectorAll('[data-compress-mode]').forEach((tab) => {
    tab.addEventListener('click', () => setMode(tab.dataset.compressMode));
  });

  $('compressPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('compressSource').value = files[0];
        setPdfOutputField('compressOutput', files[0], '压缩');
        updateBtn();
        hideResults('compressResult', 'compressError');
      }
    } catch (e) { showError(e.message || String(e), 'compressError', 'compressErrorText', 'compressResult'); }
  });

  $('compressPickOutput').addEventListener('click', async () => {
    try {
      const path = await pickPdfSave(defaultPdfOutput($('compressSource').value, '压缩'));
      if (path) { $('compressOutput').value = path; updateBtn(); hideResults('compressResult', 'compressError'); }
    } catch (e) { showError(e.message || String(e), 'compressError', 'compressErrorText', 'compressResult'); }
  });

  $('compressPickSources').addEventListener('click', async () => {
    try {
      const files = await pickPdf(true);
      if (files) {
        batchFiles = [...new Set([...batchFiles, ...files])];
        if (batchFiles.length > 0) setOutputDirField('compressOutputDir', batchFiles[0]);
        updateBatchList();
        hideResults('compressResult', 'compressError');
      }
    } catch (e) { showError(e.message || String(e), 'compressError', 'compressErrorText', 'compressResult'); }
  });

  $('compressClearSources').addEventListener('click', () => {
    batchFiles = [];
    updateBatchList();
    hideResults('compressResult', 'compressError');
  });

  $('compressPickOutputDir').addEventListener('click', async () => {
    try {
      const dir = await pickDirectory();
      if (dir) { $('compressOutputDir').value = dir; updateBtn(); hideResults('compressResult', 'compressError'); }
    } catch (e) { showError(e.message || String(e), 'compressError', 'compressErrorText', 'compressResult'); }
  });

  $('compressRun').addEventListener('click', async () => {
    const ratio = Math.max(1, Math.min(100, parseInt($('compressRatio').value, 10) || 50));
    hideResults('compressResult', 'compressError');
    const output = $('compressOutput').value.trim();
    // 单文件写前确认覆盖，避免先弹遮罩再取消造成闪烁
    if (mode === 'single' && !(await confirmOverwrite(output))) return;
    showLoading('正在压缩…');
    updateBtn();
    try {
      if (mode === 'single') {
        const result = await invoke('compress_pdf', {
          sourcePath: $('compressSource').value.trim(),
          outputPath: output,
          ratio,
        });
        showSingleResult(result, output);
      } else {
        const outputDir = $('compressOutputDir').value.trim();
        const batch = await invoke('compress_pdf_batch', {
          sourcePaths: batchFiles,
          outputDir,
          ratio,
        });
        showBatchResult(batch, outputDir);
      }
    } catch (e) {
      showError(e.message || String(e), 'compressError', 'compressErrorText', 'compressResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  setupDragDrop('compressDropZone', (paths) => {
    handleDroppedPdfs(paths, (pdfs) => {
      if (mode === 'single') {
        $('compressSource').value = pdfs[0];
        setPdfOutputField('compressOutput', pdfs[0], '压缩');
        if (pdfs.length > 1) {
          setMode('batch');
          batchFiles = [...new Set([...batchFiles, ...pdfs])];
          setOutputDirField('compressOutputDir', batchFiles[0]);
          updateBatchList();
        }
      } else {
        batchFiles = [...new Set([...batchFiles, ...pdfs])];
        if (batchFiles.length > 0) setOutputDirField('compressOutputDir', batchFiles[0]);
        updateBatchList();
      }
      updateBtn();
      hideResults('compressResult', 'compressError');
    }, (msg) => showError(msg, 'compressError', 'compressErrorText', 'compressResult'));
  });

  setMode('single');
  updateBtn();
}
