import { invoke } from '@tauri-apps/api/core';
import {
  confirmOverwrite, defaultPdfOutput, escapeHtml, handleDroppedPdfs, hideLoading, hideResults, pickPdf, pickPdfSave,
  setPdfOutputField, setupDragDrop, showError, showLoading, showSuccess,
} from '../common.js';

let mode = 'encrypt';

const SEC_SUFFIX = { encrypt: '加密', decrypt: '解密', meta: '清除元数据' };

function refreshSecOutput() {
  const src = $('secSource').value;
  if (src) setPdfOutputField('secOutput', src, SEC_SUFFIX[mode]);
}

function $(id) { return document.getElementById(id); }

function showMeta(meta) {
  // 元数据来自 PDF 文件内部，属于不可信内容，必须转义后再拼接
  const lines = [
    `<li>加密: ${meta.encrypted ? '是' : '否'}</li>`,
    meta.title ? `<li>标题: ${escapeHtml(meta.title)}</li>` : '',
    meta.author ? `<li>作者: ${escapeHtml(meta.author)}</li>` : '',
    meta.subject ? `<li>主题: ${escapeHtml(meta.subject)}</li>` : '',
    meta.keywords ? `<li>关键词: ${escapeHtml(meta.keywords)}</li>` : '',
    meta.creator ? `<li>创建程序: ${escapeHtml(meta.creator)}</li>` : '',
    meta.producer ? `<li>生成程序: ${escapeHtml(meta.producer)}</li>` : '',
  ].filter(Boolean);
  if (lines.length <= 1) lines.push('<li>无其他元数据字段</li>');
  $('secMetaList').innerHTML = lines.join('');
  $('secMetaInfo').hidden = false;
}

function setMode(next) {
  mode = next;
  document.querySelectorAll('[data-sec-mode]').forEach((tab) => {
    tab.classList.toggle('active', tab.dataset.secMode === next);
  });
  $('secEncryptPanel').hidden = next !== 'encrypt';
  $('secDecryptPanel').hidden = next !== 'decrypt';
  $('secMetaPanel').hidden = next !== 'meta';
  $('secMetaInfo').hidden = true;
  hideResults('secResult', 'secError');
  refreshSecOutput();
  updateBtn();
}

function updateBtn() {
  const btn = $('secRun');
  if (!btn || !$('loadingOverlay').hidden) { if (btn) btn.disabled = true; return; }
  const hasSource = $('secSource').value;
  const hasOutput = $('secOutput').value;
  if (!hasSource || !hasOutput) { btn.disabled = true; return; }
  if (mode === 'encrypt') {
    btn.disabled = !$('secUserPassword').value || !$('secOwnerPassword').value;
  } else if (mode === 'decrypt') {
    btn.disabled = !$('secDecryptPassword').value;
  } else {
    btn.disabled = false;
  }
}

async function refreshMetadata() {
  const path = $('secSource').value.trim();
  if (!path || mode !== 'meta') {
    $('secMetaInfo').hidden = true;
    return;
  }
  try {
    const pwd = $('secMetaPassword').value.trim() || null;
    showMeta(await invoke('get_pdf_metadata', { sourcePath: path, password: pwd }));
  } catch (e) {
    $('secMetaInfo').hidden = true;
    showError(e.message || String(e), 'secError', 'secErrorText', 'secResult');
  }
}

export function initSecurity() {
  document.querySelectorAll('[data-sec-mode]').forEach((tab) => {
    tab.addEventListener('click', () => {
      setMode(tab.dataset.secMode);
      if (tab.dataset.secMode === 'meta') refreshMetadata();
    });
  });

  ['secUserPassword', 'secOwnerPassword', 'secDecryptPassword', 'secMetaPassword'].forEach((id) => {
    $(id)?.addEventListener('input', updateBtn);
  });

  $('secPickSource').addEventListener('click', async () => {
    try {
      const files = await pickPdf(false);
      if (files?.[0]) {
        $('secSource').value = files[0];
        refreshSecOutput();
        updateBtn();
        hideResults('secResult', 'secError');
        if (mode === 'meta') await refreshMetadata();
      }
    } catch (e) { showError(e.message, 'secError', 'secErrorText', 'secResult'); }
  });
  $('secPickOutput').addEventListener('click', async () => {
    try {
      const p = await pickPdfSave(defaultPdfOutput($('secSource').value, SEC_SUFFIX[mode]));
      if (p) { $('secOutput').value = p; updateBtn(); hideResults('secResult', 'secError'); }
    } catch (e) { showError(e.message, 'secError', 'secErrorText', 'secResult'); }
  });
  $('secRefreshMeta').addEventListener('click', refreshMetadata);

  setupDragDrop('secDropZone', (paths) => {
    handleDroppedPdfs(paths, async (pdfs) => {
      $('secSource').value = pdfs[0];
      refreshSecOutput();
      updateBtn();
      hideResults('secResult', 'secError');
      if (mode === 'meta') await refreshMetadata();
    }, (msg) => showError(msg, 'secError', 'secErrorText', 'secResult'));
  });

  $('secRun').addEventListener('click', async () => {
    const sourcePath = $('secSource').value.trim();
    const outputPath = $('secOutput').value.trim();
    hideResults('secResult', 'secError');
    if (!(await confirmOverwrite(outputPath))) return;
    showLoading('正在处理…');
    updateBtn();
    try {
      let result;
      if (mode === 'encrypt') {
        const userPwd = $('secUserPassword').value;
        const ownerPwd = $('secOwnerPassword').value;
        if (!userPwd || !ownerPwd) {
          throw new Error('打开密码和权限密码均为必填项');
        }
        if (userPwd === ownerPwd) {
          throw new Error('权限密码必须与打开密码不同，否则打印/复制限制会失效');
        }
        result = await invoke('encrypt_pdf', {
          sourcePath,
          outputPath,
          userPassword: userPwd,
          ownerPassword: ownerPwd,
          allowPrint: $('secAllowPrint').checked,
          allowCopy: $('secAllowCopy').checked,
        });
      } else if (mode === 'decrypt') {
        result = await invoke('decrypt_pdf', {
          sourcePath,
          outputPath,
          password: $('secDecryptPassword').value,
        });
      } else {
        result = await invoke('clear_pdf_metadata', {
          sourcePath,
          outputPath,
          password: $('secMetaPassword').value.trim() || null,
        });
      }
      showSuccess({
        title: '处理完成',
        message: result.message,
        outputPath: result.output_path,
      });
    } catch (e) {
      showError(e.message || String(e), 'secError', 'secErrorText', 'secResult');
    } finally {
      hideLoading();
      updateBtn();
    }
  });

  setMode('encrypt');
  updateBtn();
}
