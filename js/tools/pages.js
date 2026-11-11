import { invoke } from '@tauri-apps/api/core';
import {
  basename, confirmOverwrite, defaultPdfOutput, escapeHtml, fmtSize, handleDroppedPdfs, hideLoading, hideResults,
  pickDirectory, pickPdf, pickPdfSave, setOutputDirField, setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess,
} from '../common.js';

let op = 'merge';
let mergeFiles = [];

const SHARED_OPS = ['extract', 'delete', 'rotate', 'crop', 'resize', 'blank'];

const PAGE_OP_SUFFIX = {
  extract: '提取',
  delete: '删除页',
  rotate: '旋转',
  crop: '裁剪',
  resize: '尺寸',
  blank: '去空白',
};

function refreshPagesOutput() {
  const src = $('pagesSource').value;
  if (src && PAGE_OP_SUFFIX[op]) setPdfOutputField('pagesOutput', src, PAGE_OP_SUFFIX[op]);
}

function refreshMergeOutput() {
  if (mergeFiles.length > 0) setPdfOutputField('mergeOutput', mergeFiles[0], '合并');
}

function $(id) { return document.getElementById(id); }

async function refreshInfo(path) {
  const bar = $('pagesInfoBar');
  if (!path) { bar.hidden = true; return null; }
  try {
    const info = await invoke('get_pdf_info', { sourcePath: path });
    bar.textContent = `${info.file_name} · ${fmtSize(info.file_size)} · ${info.page_count} 页`;
    bar.hidden = false;
    return info;
  } catch {
    bar.hidden = true;
    return null;
  }
}

function setOp(next) {
  op = next;
  document.querySelectorAll('[data-page-op]').forEach((t) => {
    t.classList.toggle('active', t.dataset.pageOp === next);
  });

  document.querySelectorAll('.op-panel').forEach((p) => {
    const panelOp = p.dataset.op;
    if (SHARED_OPS.includes(next)) {
      p.hidden = panelOp !== 'extract';
    } else {
      p.hidden = panelOp !== next;
    }
  });

  const showRange = ['extract', 'delete', 'rotate', 'crop'].includes(next);
  $('pagesRangeField').hidden = !showRange;
  $('rotateDegreesField').hidden = next !== 'rotate';
  $('cropMarginField').hidden = next !== 'crop';
  $('resizeSizeField').hidden = next !== 'resize';
  $('pagesOutputField').hidden = false;

  const labels = {
    extract: ['提取页码', '如 1-5, 8, 10-12'],
    delete: ['删除页码', '如 2, 5-7'],
    rotate: ['旋转页码（留空=全部）', '留空旋转所有页'],
    crop: ['裁剪页码（留空=全部）', '留空裁剪所有页'],
  };
  if (labels[next]) {
    const [label, hint] = labels[next];
    $('pagesRangeLabel').textContent = label;
    $('pagesRangeHint').textContent = hint;
  }
  if (next === 'blank') {
    $('pagesRangeHint').textContent = '自动检测并删除空白页';
  }

  hideResults('pagesResult', 'pagesError');
  refreshPagesOutput();
  updateBtn();
}

function updateMergeList() {
  $('mergeFileCount').textContent = mergeFiles.length;
  $('mergeFileList').innerHTML = mergeFiles.map((p) => `<li>${escapeHtml(basename(p))}</li>`).join('');
  refreshMergeOutput();
  updateBtn();
}

function updateBtn() {
  const btn = $('pagesRun');
  if (!btn || !$('loadingOverlay').hidden) { if (btn) btn.disabled = true; return; }

  switch (op) {
    case 'merge':
      btn.disabled = mergeFiles.length < 2 || !$('mergeOutput').value;
      break;
    case 'split':
      btn.disabled = !$('splitSource').value || !$('splitOutputDir').value;
      break;
    case 'reorder':
      btn.disabled = !$('reorderSource').value || !$('reorderOrder').value || !$('reorderOutput').value;
      break;
    case 'insert':
      btn.disabled = !$('insertTarget').value || !$('insertFrom').value || !$('insertOutput').value;
      break;
    case 'blank':
      btn.disabled = !$('pagesSource').value || !$('pagesOutput').value;
      break;
    case 'resize':
      btn.disabled = !$('pagesSource').value || !$('pagesOutput').value;
      break;
    default:
      btn.disabled = !$('pagesSource').value || !$('pagesOutput').value
        || (['extract', 'delete'].includes(op) && !$('pagesRange').value);
  }
}

function showOpResult(result) {
  showSuccess({
    title: '处理完成',
    message: result.message,
    outputPath: result.output_path,
    detailsHtml: result.page_count != null
      ? `<ul><li>页数：${escapeHtml(String(result.page_count))}</li></ul>`
      : undefined,
  });
}

function showSplitResult(result, outputDir) {
  const lines = result.files.map((f) =>
    `<li class="ok">${escapeHtml(basename(f.path))}（${escapeHtml(String(f.pages))}，${escapeHtml(String(f.page_count))} 页）</li>`
  );
  showSuccess({
    title: '拆分完成',
    message: `已生成 ${result.files.length} 个文件`,
    outputPath: outputDir,
    detailsHtml: `<ul>${lines.join('')}</ul>`,
  });
}

export function initPages() {
  document.querySelectorAll('[data-page-op]').forEach((tab) => {
    tab.addEventListener('click', () => setOp(tab.dataset.pageOp));
  });

  $('mergePickFiles').addEventListener('click', async () => {
    try {
      const files = await pickPdf(true);
      if (files) { mergeFiles = [...new Set([...mergeFiles, ...files])]; updateMergeList(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('mergeClearFiles').addEventListener('click', () => { mergeFiles = []; updateMergeList(); });
  $('mergePickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(mergeFiles[0] ? defaultPdfOutput(mergeFiles[0], '合并') : undefined);
      if (p) { $('mergeOutput').value = p; updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });

  $('splitPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) { $('splitSource').value = files[0]; setOutputDirField('splitOutputDir', files[0]); await refreshInfo(files[0]); updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('splitPickOutputDir').addEventListener('click', async () => {
    try {
      const d = await pickDirectory();
      if (d) { $('splitOutputDir').value = d; updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('splitMode').addEventListener('change', () => {
    const isEvery = $('splitMode').value === 'every_n';
    $('splitRangesField').hidden = isEvery;
    $('splitEveryField').hidden = !isEvery;
  });

  $('reorderPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('reorderSource').value = files[0];
        setPdfOutputField('reorderOutput', files[0], '重排');
        const info = await refreshInfo(files[0]);
        if (info) $('reorderOrder').placeholder = `如 2,1,3（共 ${info.page_count} 页）`;
        updateBtn();
      }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('reorderPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('reorderSource').value, '重排'));
      if (p) { $('reorderOutput').value = p; updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('reorderOrder').addEventListener('input', updateBtn);

  $('insertPickTarget').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('insertTarget').value = files[0];
        setPdfOutputField('insertOutput', files[0], '插入');
        await refreshInfo(files[0]);
        updateBtn();
      }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('insertPickFrom').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) { $('insertFrom').value = files[0]; updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('insertPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('insertTarget').value, '插入'));
      if (p) { $('insertOutput').value = p; updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });

  $('pagesPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('pagesSource').value = files[0];
        refreshPagesOutput();
        await refreshInfo(files[0]);
        updateBtn();
      }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });
  $('pagesPickOutput').addEventListener('click', async () => {
    try {
      const suffix = PAGE_OP_SUFFIX[op] || '输出';
      const p = await pickPdfSave(defaultPdfOutput($('pagesSource').value, suffix));
      if (p) { $('pagesOutput').value = p; updateBtn(); }
    } catch (e) { showError(e.message, 'pagesError', 'pagesErrorText', 'pagesResult'); }
  });

  ['pagesRange', 'cropMargin', 'insertPosition', 'insertPages'].forEach((id) => {
    $(id)?.addEventListener('input', updateBtn);
  });

  $('pagesRun').addEventListener('click', async () => {
    hideResults('pagesResult', 'pagesError');

    // 单文件输出写前确认覆盖；拆分到目录提示同名文件会被覆盖
    const singleOutputs = {
      merge: 'mergeOutput', reorder: 'reorderOutput', insert: 'insertOutput',
      extract: 'pagesOutput', delete: 'pagesOutput', rotate: 'pagesOutput',
      crop: 'pagesOutput', resize: 'pagesOutput', blank: 'pagesOutput',
    };
    const targetId = singleOutputs[op];
    if (targetId) {
      if (!(await confirmOverwrite($(targetId).value.trim()))) return;
    } else if (op === 'split' && !window.confirm('拆分到目标目录时，同名文件会被覆盖，确定继续吗？')) {
      return;
    }

    showLoading('正在处理…');
    updateBtn();
    try {
      switch (op) {
        case 'merge':
          showOpResult(await invoke('merge_pdfs', {
            sourcePaths: mergeFiles,
            outputPath: $('mergeOutput').value.trim(),
          }));
          break;
        case 'split':
          showSplitResult(await invoke('split_pdf', {
            sourcePath: $('splitSource').value.trim(),
            outputDir: $('splitOutputDir').value.trim(),
            mode: $('splitMode').value,
            pageRanges: $('splitRanges').value.trim() || null,
            everyN: parseInt($('splitEvery').value, 10) || null,
          }), $('splitOutputDir').value.trim());
          break;
        case 'reorder':
          showOpResult(await invoke('reorder_pdf_pages', {
            sourcePath: $('reorderSource').value.trim(),
            outputPath: $('reorderOutput').value.trim(),
            order: $('reorderOrder').value.trim(),
          }));
          break;
        case 'insert':
          showOpResult(await invoke('insert_pdf', {
            targetPath: $('insertTarget').value.trim(),
            insertPath: $('insertFrom').value.trim(),
            outputPath: $('insertOutput').value.trim(),
            position: parseInt($('insertPosition').value, 10) || 1,
            pages: $('insertPages').value.trim() || null,
          }));
          break;
        case 'extract':
          showOpResult(await invoke('extract_pages', {
            sourcePath: $('pagesSource').value.trim(),
            outputPath: $('pagesOutput').value.trim(),
            pages: $('pagesRange').value.trim(),
          }));
          break;
        case 'delete':
          showOpResult(await invoke('delete_pages', {
            sourcePath: $('pagesSource').value.trim(),
            outputPath: $('pagesOutput').value.trim(),
            pages: $('pagesRange').value.trim(),
          }));
          break;
        case 'rotate':
          showOpResult(await invoke('rotate_pdf_pages', {
            sourcePath: $('pagesSource').value.trim(),
            outputPath: $('pagesOutput').value.trim(),
            pages: $('pagesRange').value.trim(),
            degrees: parseInt($('rotateDegrees').value, 10),
          }));
          break;
        case 'crop':
          showOpResult(await invoke('crop_pdf_pages', {
            sourcePath: $('pagesSource').value.trim(),
            outputPath: $('pagesOutput').value.trim(),
            pages: $('pagesRange').value.trim(),
            margin: parseFloat($('cropMargin').value) || 36,
          }));
          break;
        case 'resize':
          showOpResult(await invoke('resize_pdf_pages', {
            sourcePath: $('pagesSource').value.trim(),
            outputPath: $('pagesOutput').value.trim(),
            size: $('resizeSize').value,
          }));
          break;
        case 'blank':
          showOpResult(await invoke('remove_blank_pdf_pages', {
            sourcePath: $('pagesSource').value.trim(),
            outputPath: $('pagesOutput').value.trim(),
          }));
          break;
      }
    } catch (e) {
      showError(e.message || String(e), 'pagesError', 'pagesErrorText', 'pagesResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  setupDragDrop('pagesDropZone', (paths) => {
    handleDroppedPdfs(paths, async (pdfs) => {
      if (op === 'merge') {
        mergeFiles = [...new Set([...mergeFiles, ...pdfs])];
        updateMergeList();
      } else if (op === 'split') {
        $('splitSource').value = pdfs[0];
        setOutputDirField('splitOutputDir', pdfs[0]);
        await refreshInfo(pdfs[0]);
      } else if (op === 'reorder') {
        $('reorderSource').value = pdfs[0];
        setPdfOutputField('reorderOutput', pdfs[0], '重排');
        await refreshInfo(pdfs[0]);
      } else if (op === 'insert') {
        if (!$('insertTarget').value) {
          $('insertTarget').value = pdfs[0];
          setPdfOutputField('insertOutput', pdfs[0], '插入');
        } else {
          $('insertFrom').value = pdfs[0];
        }
        updateBtn();
      } else {
        $('pagesSource').value = pdfs[0];
        refreshPagesOutput();
        await refreshInfo(pdfs[0]);
      }
      updateBtn();
    }, (msg) => showError(msg, 'pagesError', 'pagesErrorText', 'pagesResult'));
  });

  setOp('merge');
  updateBtn();
}
