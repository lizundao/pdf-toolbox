import { getCurrentWindow } from '@tauri-apps/api/window';

function closeAllMenus({ focusTrigger = false } = {}) {
  document.querySelectorAll('.menu-root.open').forEach((root) => {
    root.classList.remove('open');
    const popup = root.querySelector('.menu-popup');
    if (popup) popup.hidden = true;
    const trigger = root.querySelector('.menu-trigger');
    if (trigger) {
      trigger.setAttribute('aria-expanded', 'false');
      if (focusTrigger) trigger.focus();
    }
  });
}

/** 自定义标题栏拖动（需 core:window:allow-start-dragging） */
function setupTitlebarDrag(win) {
  const titlebar = document.querySelector('.titlebar');
  if (!titlebar) return;

  titlebar.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    const target = e.target;
    if (target.closest('.window-controls, .menu-root, .menu-popup, button, a, select, input')) {
      return;
    }
    e.preventDefault();
    win.startDragging().catch(() => {});
  });
}

export function initMenubar(navigate) {
  const win = getCurrentWindow();

  setupTitlebarDrag(win);

  document.querySelectorAll('.menu-root').forEach((root, index) => {
    const trigger = root.querySelector('.menu-trigger');
    const popup = root.querySelector('.menu-popup');
    if (!trigger || !popup) return;

    // 补齐菜单的无障碍语义
    trigger.setAttribute('aria-haspopup', 'menu');
    trigger.setAttribute('aria-expanded', 'false');
    if (!popup.id) popup.id = `appMenuPopup${index}`;
    trigger.setAttribute('aria-controls', popup.id);
    popup.setAttribute('role', 'menu');
    const items = [...popup.querySelectorAll('button')];
    items.forEach((item) => item.setAttribute('role', 'menuitem'));

    const open = () => {
      closeAllMenus();
      root.classList.add('open');
      popup.hidden = false;
      trigger.setAttribute('aria-expanded', 'true');
      items[0]?.focus();
    };
    const close = (focusTrigger = false) => {
      root.classList.remove('open');
      popup.hidden = true;
      trigger.setAttribute('aria-expanded', 'false');
      if (focusTrigger) trigger.focus();
    };

    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      if (root.classList.contains('open')) close(true);
      else open();
    });

    // 键盘：↑↓ 在菜单项间移动，Esc 关闭并回到触发按钮，Tab 关闭菜单
    root.addEventListener('keydown', (e) => {
      if (!root.classList.contains('open')) return;
      if (e.key === 'Escape') { e.stopPropagation(); close(true); }
      else if (e.key === 'Tab') close();
      else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const current = items.indexOf(document.activeElement);
        const step = e.key === 'ArrowDown' ? 1 : -1;
        const next = (current + step + items.length) % items.length;
        items[next]?.focus();
      }
    });

    popup.querySelectorAll('[data-view]').forEach((item) => {
      item.addEventListener('click', () => {
        navigate(item.dataset.view);
        close();
      });
    });
    popup.querySelector('#menuQuit')?.addEventListener('click', () => close());
  });

  document.getElementById('menuQuit')?.addEventListener('click', () => {
    win.close().catch(() => {});
  });

  document.addEventListener('click', () => closeAllMenus());
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeAllMenus({ focusTrigger: true });
  });

  document.getElementById('winMinimize')?.addEventListener('click', () => {
    win.minimize().catch(() => {});
  });
  document.getElementById('winMaximize')?.addEventListener('click', async () => {
    try {
      if (await win.isMaximized()) await win.unmaximize();
      else await win.maximize();
    } catch { /* ignore */ }
  });
  document.getElementById('winClose')?.addEventListener('click', () => {
    win.close().catch(() => {});
  });

  win.onResized(async () => {
    const btn = document.getElementById('winMaximize');
    if (!btn) return;
    try {
      btn.textContent = (await win.isMaximized()) ? '❐' : '□';
    } catch { /* ignore */ }
  });
}
