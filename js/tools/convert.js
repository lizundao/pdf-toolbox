import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { pdfjsLib } from '../pdfjs-setup.js';
import {
  confirmOverwrite, defaultFileOutput, defaultPdfOutput, basename, escapeHtml, fileStem, handleDroppedImages,
  handleDroppedOffice, handleDroppedPdfs, hideInlineProgress, hideLoading, hideResults, joinPath, pickDirectory,
  pickImages, pickOfficeFile, pickPdf, pickPdfSave, pickTextFile, pickTextSave, setFileOutputField, setOutputDirField,
  setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess, updateInlineProgress, updateLoadingProgress,
} from '../common.js';

let mode = 'img2pdf';
let pdf2imgMode = 'single';
let imageFiles = [];
let batchPdfFiles = [];
let officeStatus = null;
let officeDetecting = false;
let progressListenerReady = false;

function $(id) { return document.getElementById(id); }

function refreshImg2pdfOutput() {
  if (imageFiles.length > 0) setPdfOutputField('convertImgOutput', imageFiles[0], '合成');
}

function showResult(detail, outputPath) {
  showSuccess({
    title: '转换完成',
    message: detail,
    outputPath,
  });
}

function officeReadyForPath(path) {
  if (!officeStatus?.platform_supported) return false;
  if (!path) {
    return officeStatus.word || officeStatus.excel || officeStatus.powerpoint;
  }
  const ext = path.split('.').pop()?.toLowerCase() || '';
  if (['doc', 'docx', 'docm', 'rtf'].includes(ext)) return officeStatus.word;
  if (['xls', 'xlsx', 'xlsm', 'csv'].includes(ext)) return officeStatus.excel;
  if (['ppt', 'pptx', 'pptm'].includes(ext)) return officeStatus.powerpoint;
  return false;
}

async function refreshOfficeStatus() {
  const notice = $('convertOfficeNotice');
  const statusEl = $('convertOfficeStatus');
  if (!notice || officeDetecting) return;

  officeDetecting = true;
  notice.className = 'info-bar';
  notice.textContent = '正在检测本机 Office…';
  statusEl.hidden = true;
  updateInlineProgress('convertOfficeDetect', 0, '准备检测');
  updateBtn();

  try {
    officeStatus = await invoke('get_office_status');
  } catch {
    officeStatus = { platform_supported: false, word: false, excel: false, powerpoint: false };
  } finally {
    officeDetecting = false;
    hideInlineProgress('convertOfficeDetect');
  }

  if (!officeStatus.platform_supported) {
    notice.className = 'info-bar error';
    notice.textContent = 'Office 转 PDF 仅支持 Windows。当前系统无法使用此功能。';
    statusEl.hidden = true;
  } else if (!officeStatus.word && !officeStatus.excel && !officeStatus.powerpoint) {
    notice.className = 'info-bar error';
    notice.textContent = '未检测到 Microsoft Office。此功能需要本机已安装 Office（Word / Excel / PowerPoint），未安装 Office 无法使用。';
    statusEl.hidden = true;
  } else {
    notice.className = 'info-bar warn';
    notice.textContent = '此功能依赖本机已安装的 Microsoft Office 进行转换。未安装 Office 无法使用；转换时会短暂启动对应的 Office 程序。';
    const parts = [];
    if (officeStatus.word) parts.push('Word');
    if (officeStatus.excel) parts.push('Excel');
    if (officeStatus.powerpoint) parts.push('PowerPoint');
    statusEl.textContent = `已检测到：${parts.join('、')}`;
    statusEl.hidden = false;
  }
  updateBtn();
}

async function ensureProgressListener() {
  if (progressListenerReady) return;
  try {
    await listen('task-progress', (event) => {
      const { percent, message } = event.payload ?? {};
      if (officeDetecting) {
        updateInlineProgress('convertOfficeDetect', percent, message);
        $('convertOfficeNotice').textContent = message;
        return;
      }
      if (!$('loadingOverlay').hidden) {
        updateLoadingProgress(percent, message);
      }
    });
    progressListenerReady = true;
  } catch (e) {
    // 监听注册失败不应阻断转换本身，仅降级为无进度提示
    console.warn('进度监听注册失败：', e);
  }
}

function setMode(next) {
  mode = next;
  document.querySelectorAll('[data-convert-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.convertMode === next);
  });
  $('convertImg2pdf').hidden = next !== 'img2pdf';
  $('convertPdf2img').hidden = next !== 'pdf2img';
  $('convertPdf2text').hidden = next !== 'pdf2text';
  $('convertText2pdf').hidden = next !== 'text2pdf';
  $('convertOffice2pdf').hidden = next !== 'office2pdf';
  $('convertDropZone').hidden = next === 'text2pdf';
  const dropSub = $('convertDropSub');
  if (dropSub) {
    if (next === 'office2pdf') dropSub.textContent = 'Office 文档（Word / Excel / PowerPoint）';
    else if (next === 'img2pdf') dropSub.textContent = '图片文件';
    else if (next === 'pdf2text' || next === 'pdf2img') dropSub.textContent = 'PDF 文件';
    else dropSub.textContent = '图片 / PDF，依当前模式自动识别';
  }
  if (next === 'office2pdf') refreshOfficeStatus();
  hideResults('convertResult', 'convertError');
  updateBtn();
}

function setPdf2imgMode(next) {
  pdf2imgMode = next;
  document.querySelectorAll('[data-pdf2img-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.pdf2imgMode === next);
  });
  $('convertPdf2imgSingle').hidden = next !== 'single';
  $('convertPdf2imgBatch').hidden = next !== 'batch';
  updateBtn();
}

function updateImageList() {
  $('convertImageCount').textContent = imageFiles.length;
  $('convertImageList').innerHTML = imageFiles.map((p) => `<li>${escapeHtml(basename(p))}</li>`).join('');
  refreshImg2pdfOutput();
  updateBtn();
}

function updateBatchPdfList() {
  $('convertPdf2imgBatchCount').textContent = batchPdfFiles.length;
  $('convertPdf2imgBatchList').innerHTML = batchPdfFiles.map((p) => `<li>${escapeHtml(basename(p))}</li>`).join('');
  if (batchPdfFiles.length > 0) setOutputDirField('convertImgOutputDirBatch', batchPdfFiles[0]);
  updateBtn();
}

function updateBtn() {
  const btn = $('convertRun');
  if (!btn || !$('loadingOverlay').hidden) { if (btn) btn.disabled = true; return; }
  if (mode === 'img2pdf') {
    btn.disabled = imageFiles.length === 0 || !$('convertImgOutput').value;
  } else if (mode === 'pdf2img') {
    if (pdf2imgMode === 'single') {
      btn.disabled = !$('convertPdf2imgSource').value || !$('convertImgOutputDir').value;
    } else {
      btn.disabled = batchPdfFiles.length === 0 || !$('convertImgOutputDirBatch').value;
    }
  } else if (mode === 'pdf2text') {
    btn.disabled = !$('convertPdfSource').value || !$('convertTextOutput').value;
  } else if (mode === 'office2pdf') {
    if (officeDetecting) { btn.disabled = true; return; }
    const source = $('convertOfficeSource').value.trim();
    btn.disabled = !officeReadyForPath(source) || !$('convertOfficeOutput').value;
  } else {
    const hasFile = $('convertTextSource').value;
    const hasText = $('convertTextInput').value.trim();
    btn.disabled = (!$('convertTextPdfOutput').value) || (!hasFile && !hasText);
  }
}

function exportOptions() {
  return {
    format: $('convertImgFormat').value,
    scale: parseFloat($('convertImgScale').value) || 2,
    quality: Math.max(0.1, Math.min(1, (parseInt($('convertImgQuality').value, 10) || 85) / 100)),
  };
}

async function exportOnePdfToImages(sourcePath, outputDir, pageRange, password) {
  const { format, scale, quality } = exportOptions();
  const pages = await invoke('list_pdf_export_pages', { sourcePath, pageRange, password });
  const bytes = await invoke('read_pdf_file', { sourcePath });
  const pdf = await pdfjsLib.getDocument({
    data: new Uint8Array(bytes),
    password: password || undefined,
  }).promise;

  const stem = fileStem(sourcePath);
  const ext = format === 'jpeg' ? 'jpg' : 'png';
  const mime = format === 'jpeg' ? 'image/jpeg' : 'image/png';
  let count = 0;

  try {
    for (let i = 0; i < pages.length; i++) {
      const pageNum = pages[i];
      const pct = Math.round(((i + 1) / pages.length) * 100);
      const msg = `${basename(sourcePath)}：第 ${i + 1}/${pages.length} 页…`;
      updateLoadingProgress(pct, msg);
      const page = await pdf.getPage(pageNum);
      const viewport = page.getViewport({ scale });
      const canvas = document.createElement('canvas');
      canvas.width = viewport.width;
      canvas.height = viewport.height;
      try {
        await page.render({ canvasContext: canvas.getContext('2d'), viewport }).promise;

        const blob = await new Promise((resolve, reject) => {
          canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('图片编码失败'))), mime, quality);
        });
        const data = Array.from(new Uint8Array(await blob.arrayBuffer()));
        const filename = `${stem}_page_${String(pageNum).padStart(3, '0')}.${ext}`;
        const outPath = joinPath(outputDir, filename);
        await invoke('write_binary_file', { path: outPath, data });
        count += 1;
      } finally {
        if (typeof page.cleanup === 'function') page.cleanup();
        // 主动释放画布占用，批量导出大文件时避免显存/内存堆积
        canvas.width = 0;
        canvas.height = 0;
      }
    }
    return count;
  } finally {
    try { await pdf.destroy(); } catch { /* ignore */ }
  }
}

async function exportPdfToImages() {
  const sourcePath = $('convertPdf2imgSource').value.trim();
  const outputDir = $('convertImgOutputDir').value.trim();
  const pageRange = $('convertPdf2imgRange').value.trim() || null;
  const password = $('convertPdf2imgPassword').value.trim() || null;
  const { format } = exportOptions();
  const count = await exportOnePdfToImages(sourcePath, outputDir, pageRange, password);
  return {
    detail: `已导出 ${count} 张 ${format === 'jpeg' ? 'JPEG' : 'PNG'} 图片`,
    output_path: outputDir,
  };
}

async function exportPdfToImagesBatch() {
  const outputDir = $('convertImgOutputDirBatch').value.trim();
  const pageRange = $('convertPdf2imgBatchRange').value.trim() || null;
  const password = $('convertPdf2imgBatchPassword').value.trim() || null;
  let total = 0;
  let ok = 0;

  for (const sourcePath of batchPdfFiles) {
    const subDir = joinPath(outputDir, fileStem(sourcePath));
    try {
      total += await exportOnePdfToImages(sourcePath, subDir, pageRange, password);
      ok += 1;
    } catch (e) {
      throw new Error(`${basename(sourcePath)}：${e.message || e}`);
    }
  }

  return {
    detail: `批量完成：${ok} 个 PDF，共 ${total} 张图片`,
    output_path: outputDir,
  };
}

export function initConvert() {
  ensureProgressListener();

  document.querySelectorAll('[data-convert-mode]').forEach((tab) => {
    tab.addEventListener('click', () => setMode(tab.dataset.convertMode));
  });
  document.querySelectorAll('[data-pdf2img-mode]').forEach((tab) => {
    tab.addEventListener('click', () => setPdf2imgMode(tab.dataset.pdf2imgMode));
  });

  $('convertImgQuality').addEventListener('input', () => {
    $('convertImgQualityVal').textContent = $('convertImgQuality').value;
  });

  $('convertPickImages').addEventListener('click', async () => {
    try {
      const files = await pickImages(true);
      if (files) { imageFiles = [...new Set([...imageFiles, ...files])]; updateImageList(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertClearImages').addEventListener('click', () => { imageFiles = []; updateImageList(); hideResults('convertResult', 'convertError'); });
  $('convertPickImgOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(imageFiles[0] ? defaultPdfOutput(imageFiles[0], '合成') : undefined);
      if (p) { $('convertImgOutput').value = p; updateBtn(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertPickImgOutputDir').addEventListener('click', async () => {
    try {
      const d = await pickDirectory();
      if (d) { $('convertImgOutputDir').value = d; updateBtn(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertPickImgOutputDirBatch').addEventListener('click', async () => {
    try {
      const d = await pickDirectory();
      if (d) { $('convertImgOutputDirBatch').value = d; updateBtn(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });

  $('convertPickPdf2imgSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('convertPdf2imgSource').value = files[0];
        setOutputDirField('convertImgOutputDir', files[0]);
        updateBtn();
        hideResults('convertResult', 'convertError');
      }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertPickPdf2imgBatch').addEventListener('click', async () => {
    try {
      const files = await pickPdf(true);
      if (files) { batchPdfFiles = [...new Set([...batchPdfFiles, ...files])]; updateBatchPdfList(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertClearPdf2imgBatch').addEventListener('click', () => { batchPdfFiles = []; updateBatchPdfList(); hideResults('convertResult', 'convertError'); });

  $('convertPickPdfSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('convertPdfSource').value = files[0];
        setFileOutputField('convertTextOutput', files[0], '文本', 'txt');
        updateBtn();
        hideResults('convertResult', 'convertError');
      }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertPickTextOutput').addEventListener('click', async () => {
    try {
      const p = await pickTextSave(defaultFileOutput($('convertPdfSource').value, '文本', 'txt'));
      if (p) { $('convertTextOutput').value = p; updateBtn(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertPickTextSource').addEventListener('click', async () => {
    try {
      const p = await pickTextFile();
      if (p) {
        $('convertTextSource').value = p;
        setPdfOutputField('convertTextPdfOutput', p, '转PDF');
        updateBtn();
        hideResults('convertResult', 'convertError');
      }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertTextInput').addEventListener('input', updateBtn);
  $('convertPickTextPdfOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('convertTextSource').value, '转PDF'));
      if (p) { $('convertTextPdfOutput').value = p; updateBtn(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });

  $('convertPickOfficeSource').addEventListener('click', async () => {
    try {
      const path = await pickOfficeFile();
      if (path) {
        $('convertOfficeSource').value = path;
        setPdfOutputField('convertOfficeOutput', path, '转PDF');
        updateBtn();
        hideResults('convertResult', 'convertError');
      }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });
  $('convertPickOfficeOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('convertOfficeSource').value, '转PDF'));
      if (p) { $('convertOfficeOutput').value = p; updateBtn(); hideResults('convertResult', 'convertError'); }
    } catch (e) { showError(e.message, 'convertError', 'convertErrorText', 'convertResult'); }
  });

  setupDragDrop('convertDropZone', (paths) => {
    if (mode === 'img2pdf') {
      handleDroppedImages(paths, (imgs) => {
        imageFiles = [...new Set([...imageFiles, ...imgs])];
        updateImageList();
        hideResults('convertResult', 'convertError');
      }, (msg) => showError(msg, 'convertError', 'convertErrorText', 'convertResult'));
    } else if (mode === 'pdf2img') {
      handleDroppedPdfs(paths, (pdfs) => {
        if (pdf2imgMode === 'single') {
          $('convertPdf2imgSource').value = pdfs[0];
          setOutputDirField('convertImgOutputDir', pdfs[0]);
          if (pdfs.length > 1) {
            setPdf2imgMode('batch');
            batchPdfFiles = [...new Set([...batchPdfFiles, ...pdfs])];
            updateBatchPdfList();
          }
        } else {
          batchPdfFiles = [...new Set([...batchPdfFiles, ...pdfs])];
          updateBatchPdfList();
        }
        updateBtn();
        hideResults('convertResult', 'convertError');
      }, (msg) => showError(msg, 'convertError', 'convertErrorText', 'convertResult'));
    } else if (mode === 'pdf2text') {
      handleDroppedPdfs(paths, (pdfs) => {
        $('convertPdfSource').value = pdfs[0];
        setFileOutputField('convertTextOutput', pdfs[0], '文本', 'txt');
        updateBtn();
        hideResults('convertResult', 'convertError');
      }, (msg) => showError(msg, 'convertError', 'convertErrorText', 'convertResult'));
    } else if (mode === 'office2pdf') {
      handleDroppedOffice(paths, (files) => {
        const path = files[0];
        $('convertOfficeSource').value = path;
        setPdfOutputField('convertOfficeOutput', path, '转PDF');
        updateBtn();
        hideResults('convertResult', 'convertError');
      }, (msg) => showError(msg, 'convertError', 'convertErrorText', 'convertResult'));
    }
  });

  $('convertRun').addEventListener('click', async () => {
    hideResults('convertResult', 'convertError');
    await ensureProgressListener();

    // 单文件输出：写前确认覆盖；图片导出到目录：提示同名文件会被覆盖
    let overwriteTarget = null;
    if (mode === 'img2pdf') overwriteTarget = $('convertImgOutput').value.trim();
    else if (mode === 'pdf2text') overwriteTarget = $('convertTextOutput').value.trim();
    else if (mode === 'office2pdf') overwriteTarget = $('convertOfficeOutput').value.trim();
    else if (mode === 'text2pdf') overwriteTarget = $('convertTextPdfOutput').value.trim();
    if (overwriteTarget && !(await confirmOverwrite(overwriteTarget))) return;
    if (mode === 'pdf2img' && !window.confirm('导出到目标目录时，同名图片会被覆盖，确定继续吗？')) return;

    const withProgress = mode === 'office2pdf' || mode === 'pdf2img';
    showLoading('正在转换…', { percent: withProgress ? 0 : null });
    updateBtn();
    try {
      let result;
      if (mode === 'img2pdf') {
        result = await invoke('images_to_pdf', {
          imagePaths: imageFiles,
          outputPath: $('convertImgOutput').value.trim(),
        });
        showResult(result.detail, result.output_path);
      } else if (mode === 'pdf2img') {
        result = pdf2imgMode === 'single'
          ? await exportPdfToImages()
          : await exportPdfToImagesBatch();
        showResult(result.detail, result.output_path);
      } else if (mode === 'pdf2text') {
        result = await invoke('extract_pdf_text', {
          sourcePath: $('convertPdfSource').value.trim(),
          outputPath: $('convertTextOutput').value.trim(),
          pageRange: $('convertPageRange').value.trim() || null,
        });
        showResult(result.detail, result.output_path);
      } else if (mode === 'office2pdf') {
        result = await invoke('office_to_pdf', {
          sourcePath: $('convertOfficeSource').value.trim(),
          outputPath: $('convertOfficeOutput').value.trim(),
        });
        showResult(result.detail, result.output_path);
      } else {
        const sourcePath = $('convertTextSource').value.trim();
        const outputPath = $('convertTextPdfOutput').value.trim();
        const text = $('convertTextInput').value;
        if (sourcePath) {
          result = await invoke('text_file_to_pdf', { sourcePath, outputPath });
        } else {
          result = await invoke('text_to_pdf', { text, outputPath });
        }
        showResult(result.detail, result.output_path);
      }
    } catch (e) {
      showError(e.message || String(e), 'convertError', 'convertErrorText', 'convertResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  setMode('img2pdf');
  setPdf2imgMode('single');
  updateBtn();
}
