import { invoke } from '@tauri-apps/api/core';
import {
  basename, confirmOverwrite, defaultPdfOutput, escapeHtml, fmtSize, handleDroppedPdfs, hideLoading, hideResults,
  pickDirectory, pickPdf, pickPdfSave, setOutputDirField, setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess,
} from '../common.js';

let mode = 'single';
let batchFiles = [];

function $(id) { return document.getElementById(id); }

function showSingleResult(result, outputPath) {
  const skipped = result.skipped
    ? `<li>跳过 ${result.skipped} 张（不支持的色彩空间/格式）</li>`
    : '';
  showSuccess({
    title: '处理完成',
    message: `节省 ${fmtSize(result.saved_bytes)}（约 ${result.percent_reduced.toFixed(1)}%）`,
    outputPath,
    detailsHtml: [
      '<ul>',
      `<li>原始大小：${fmtSize(result.original_size)}</li>`,
      `<li>压缩后：${fmtSize(result.new_size)}</li>`,
      skipped,
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
      const skipped = r.skipped ? `（跳过 ${r.skipped} 张）` : '';
      lines.push(`<li class="ok">${name}：${fmtSize(r.original_size)} → ${fmtSize(r.new_size)}（-${r.percent_reduced.toFixed(1)}%）${skipped}</li>`);
    } else {
      lines.push(`<li class="fail">${name}：${escapeHtml(item.error)}</li>`);
    }
  }
  showSuccess({
    title: '批量处理完成',
    message: `成功 ${batch.succeeded} 个，失败 ${batch.failed} 个`,
    outputPath: outputDir,
    detailsHtml: `<ul>${lines.join('')}</ul>`,
  });
}

function setMode(next) {
  mode = next;
  document.querySelectorAll('[data-scan-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.scanMode === next);
  });
  $('scanSingle').hidden = next !== 'single';
  $('scanBatch').hidden = next !== 'batch';
  hideResults('scanResult', 'scanError');
  updateBtn();
}

function updateBatchList() {
  $('scanFileCount').textContent = batchFiles.length;
  $('scanFileList').innerHTML = batchFiles.map((p) => `<li>${escapeHtml(basename(p))}</li>`).join('');
  updateBtn();
}

function updateBtn() {
  const btn = $('scanRun');
  if (!btn || !$('loadingOverlay').hidden) { if (btn) btn.disabled = true; return; }
  if (mode === 'single') {
    btn.disabled = !$('scanSource').value || !$('scanOutput').value;
  } else {
    btn.disabled = batchFiles.length === 0 || !$('scanOutputDir').value;
  }
}

export function initScan() {
  $('scanQuality').addEventListener('input', () => {
    $('scanQualityVal').textContent = $('scanQuality').value;
  });

  document.querySelectorAll('[data-scan-mode]').forEach((tab) => {
    tab.addEventListener('click', () => setMode(tab.dataset.scanMode));
  });

  $('scanPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) { $('scanSource').value = files[0]; setPdfOutputField('scanOutput', files[0], '扫描'); updateBtn(); hideResults('scanResult', 'scanError'); }
    } catch (e) { showError(e.message, 'scanError', 'scanErrorText', 'scanResult'); }
  });
  $('scanPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('scanSource').value, '扫描'));
      if (p) { $('scanOutput').value = p; updateBtn(); hideResults('scanResult', 'scanError'); }
    } catch (e) { showError(e.message, 'scanError', 'scanErrorText', 'scanResult'); }
  });
  $('scanPickSources').addEventListener('click', async () => {
    try {
      const files = await pickPdf(true);
      if (files) { batchFiles = [...new Set([...batchFiles, ...files])]; if (batchFiles.length > 0) setOutputDirField('scanOutputDir', batchFiles[0]); updateBatchList(); hideResults('scanResult', 'scanError'); }
    } catch (e) { showError(e.message, 'scanError', 'scanErrorText', 'scanResult'); }
  });
  $('scanClearSources').addEventListener('click', () => { batchFiles = []; updateBatchList(); hideResults('scanResult', 'scanError'); });
  $('scanPickOutputDir').addEventListener('click', async () => {
    try {
      const d = await pickDirectory();
      if (d) { $('scanOutputDir').value = d; updateBtn(); hideResults('scanResult', 'scanError'); }
    } catch (e) { showError(e.message, 'scanError', 'scanErrorText', 'scanResult'); }
  });

  $('scanRun').addEventListener('click', async () => {
    const quality = Math.max(10, Math.min(100, parseInt($('scanQuality').value, 10) || 75));
    const maxDimension = Math.max(0, parseInt($('scanMaxDim').value, 10) || 0);
    const grayscale = $('scanGrayscale').checked;
    hideResults('scanResult', 'scanError');
    const output = $('scanOutput').value.trim();
    // 单文件写前确认覆盖，避免先弹遮罩再取消造成闪烁
    if (mode === 'single' && !(await confirmOverwrite(output))) return;
    showLoading('正在处理图片…');
    updateBtn();
    try {
      if (mode === 'single') {
        showSingleResult(await invoke('compress_pdf_images', {
          sourcePath: $('scanSource').value.trim(),
          outputPath: output,
          quality,
          maxDimension,
          grayscale,
        }), output);
      } else {
        const outputDir = $('scanOutputDir').value.trim();
        showBatchResult(await invoke('compress_pdf_images_batch', {
          sourcePaths: batchFiles,
          outputDir,
          quality,
          maxDimension,
          grayscale,
        }), outputDir);
      }
    } catch (e) {
      showError(e.message || String(e), 'scanError', 'scanErrorText', 'scanResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  setupDragDrop('scanDropZone', (paths) => {
    handleDroppedPdfs(paths, (pdfs) => {
      if (mode === 'single') {
        $('scanSource').value = pdfs[0];
        setPdfOutputField('scanOutput', pdfs[0], '扫描');
        if (pdfs.length > 1) {
          setMode('batch');
          batchFiles = [...new Set([...batchFiles, ...pdfs])];
          setOutputDirField('scanOutputDir', batchFiles[0]);
          updateBatchList();
        }
      } else {
        batchFiles = [...new Set([...batchFiles, ...pdfs])];
        if (batchFiles.length > 0) setOutputDirField('scanOutputDir', batchFiles[0]);
        updateBatchList();
      }
      updateBtn();
      hideResults('scanResult', 'scanError');
    }, (msg) => showError(msg, 'scanError', 'scanErrorText', 'scanResult'));
  });

  setMode('single');
  updateBtn();
}
