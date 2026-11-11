import { invoke } from '@tauri-apps/api/core';
import {
  confirmOverwrite, defaultPdfOutput, handleDroppedPdfs, hideLoading, hideResults, pickFontFile, pickImages,
  pickPdf, pickPdfSave, setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess,
} from '../common.js';

let mode = 'text';

function $(id) { return document.getElementById(id); }

// 数值回退：保留合法的 0（如 0° 旋转、0 透明度），仅在非有限数时取默认值
function numOr(raw, fallback) {
  const n = parseFloat(raw);
  return Number.isFinite(n) ? n : fallback;
}

function fontOptionValue(path, faceIndex) {
  return `${path}\t${faceIndex}`;
}

function parseFontValue(value) {
  if (!value) return { fontPath: null, fontFaceIndex: 0 };
  const tab = value.indexOf('\t');
  if (tab < 0) return { fontPath: value, fontFaceIndex: 0 };
  return {
    fontPath: value.slice(0, tab),
    fontFaceIndex: parseInt(value.slice(tab + 1), 10) || 0,
  };
}

function selectedFont() {
  return parseFontValue($('wmFontSelect').value);
}

async function loadSystemFonts() {
  const sel = $('wmFontSelect');
  try {
    const fonts = await invoke('list_system_fonts');
    sel.replaceChildren();
    if (!fonts?.length) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '未找到系统字体';
      sel.append(opt);
      return;
    }
    // 字体路径/名称来自系统扫描，使用 DOM API 赋值避免 innerHTML 注入
    const fragment = document.createDocumentFragment();
    for (const f of fonts) {
      const opt = document.createElement('option');
      opt.value = fontOptionValue(f.path, f.face_index);
      opt.textContent = f.label;
      fragment.append(opt);
    }
    sel.append(fragment);
  } catch (e) {
    sel.replaceChildren();
    const opt = document.createElement('option');
    opt.value = '';
    opt.textContent = '加载字体失败';
    sel.append(opt);
    showError(e.message || String(e), 'wmError', 'wmErrorText', 'wmResult');
  }
}

async function addCustomFont(path) {
  try {
    const entry = await invoke('validate_font_file', { path, faceIndex: 0 });
    const sel = $('wmFontSelect');
    const val = fontOptionValue(entry.path, entry.face_index);
    const exists = [...sel.options].some((o) => o.value === val);
    if (!exists) {
      const opt = document.createElement('option');
      opt.value = val;
      opt.textContent = `${entry.label}（自定义）`;
      sel.prepend(opt);
    }
    sel.value = val;
  } catch (e) {
    showError(e.message || String(e), 'wmError', 'wmErrorText', 'wmResult');
  }
}

function setMode(next) {
  mode = next;
  document.querySelectorAll('[data-wm-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.wmMode === next);
  });
  $('wmTextPanel').hidden = next !== 'text';
  $('wmImagePanel').hidden = next !== 'image';
  $('wmHeaderPanel').hidden = next !== 'header';
  $('wmFontField').hidden = next === 'image';
  hideResults('wmResult', 'wmError');
  updateBtn();
}

function updateBtn() {
  const btn = $('wmRun');
  if (!btn || !$('loadingOverlay').hidden) { if (btn) btn.disabled = true; return; }
  const hasSource = $('wmSource').value;
  const hasOutput = $('wmOutput').value;
  if (!hasSource || !hasOutput) { btn.disabled = true; return; }
  if (mode === 'text') {
    btn.disabled = !$('wmText').value.trim();
  } else if (mode === 'image') {
    btn.disabled = !$('wmImage').value;
  } else {
    btn.disabled = false;
  }
}

export function initWatermark() {
  loadSystemFonts();

  document.querySelectorAll('[data-wm-mode]').forEach((tab) => {
    tab.addEventListener('click', () => setMode(tab.dataset.wmMode));
  });

  $('wmOpacity').addEventListener('input', () => {
    $('wmOpacityVal').textContent = $('wmOpacity').value;
  });
  $('wmFontSize').addEventListener('input', () => {
    $('wmFontSizeVal').textContent = $('wmFontSize').value;
  });
  $('wmImgScale').addEventListener('input', () => {
    $('wmImgScaleVal').textContent = $('wmImgScale').value;
  });
  $('wmText').addEventListener('input', updateBtn);

  $('wmPickFont').addEventListener('click', async () => {
    try {
      const path = await pickFontFile();
      if (path) await addCustomFont(path);
    } catch (e) { showError(e.message, 'wmError', 'wmErrorText', 'wmResult'); }
  });

  $('wmPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) { $('wmSource').value = files[0]; setPdfOutputField('wmOutput', files[0], '水印'); updateBtn(); hideResults('wmResult', 'wmError'); }
    } catch (e) { showError(e.message, 'wmError', 'wmErrorText', 'wmResult'); }
  });
  $('wmPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('wmSource').value, '水印'));
      if (p) { $('wmOutput').value = p; updateBtn(); hideResults('wmResult', 'wmError'); }
    } catch (e) { showError(e.message, 'wmError', 'wmErrorText', 'wmResult'); }
  });
  $('wmPickImage').addEventListener('click', async () => {
    try {
      const files = await pickImages(false);
      if (files?.[0]) { $('wmImage').value = files[0]; updateBtn(); hideResults('wmResult', 'wmError'); }
    } catch (e) { showError(e.message, 'wmError', 'wmErrorText', 'wmResult'); }
  });

  setupDragDrop('wmDropZone', (paths) => {
    handleDroppedPdfs(paths, (pdfs) => {
      $('wmSource').value = pdfs[0];
      setPdfOutputField('wmOutput', pdfs[0], '水印');
      updateBtn();
      hideResults('wmResult', 'wmError');
    }, (msg) => showError(msg, 'wmError', 'wmErrorText', 'wmResult'));
  });

  $('wmRun').addEventListener('click', async () => {
    const sourcePath = $('wmSource').value.trim();
    const outputPath = $('wmOutput').value.trim();
    const pageRange = $('wmPageRange').value.trim() || null;
    const { fontPath, fontFaceIndex } = selectedFont();
    hideResults('wmResult', 'wmError');
    if (!(await confirmOverwrite(outputPath))) return;
    showLoading('正在添加水印…');
    updateBtn();
    try {
      let result;
      const fontArgs = {
        fontPath: fontPath || null,
        fontFaceIndex,
      };
      if (mode === 'text') {
        result = await invoke('add_text_watermark', {
          sourcePath,
          outputPath,
          text: $('wmText').value.trim(),
          fontSize: numOr($('wmFontSize').value, 48),
          opacity: numOr($('wmOpacity').value, 25) / 100,
          angle: numOr($('wmAngle').value, 45),
          tiled: $('wmTiled').checked,
          pageRange,
          ...fontArgs,
        });
      } else if (mode === 'image') {
        result = await invoke('add_image_watermark', {
          sourcePath,
          outputPath,
          imagePath: $('wmImage').value.trim(),
          opacity: numOr($('wmImgOpacity').value, 50) / 100,
          scalePercent: numOr($('wmImgScale').value, 30),
          position: $('wmImgPosition').value,
          pageRange,
        });
      } else {
        result = await invoke('add_page_headers', {
          sourcePath,
          outputPath,
          headerText: $('wmHeaderText').value.trim() || null,
          footerTemplate: $('wmFooterTemplate').value.trim() || '{page} / {total}',
          showDate: $('wmShowDate').checked,
          pageRange,
          ...fontArgs,
        });
      }
      showSuccess({
        title: '处理完成',
        message: result.message,
        outputPath: result.output_path,
      });
    } catch (e) {
      showError(e.message || String(e), 'wmError', 'wmErrorText', 'wmResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  setMode('text');
  updateBtn();
}
