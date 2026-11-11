const STORAGE_KEY = 'pdf-toolbox-theme';

export function getTheme() {
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
}

export function setTheme(theme) {
  const next = theme === 'light' ? 'light' : 'dark';
  if (next === 'light') {
    document.documentElement.dataset.theme = 'light';
  } else {
    delete document.documentElement.dataset.theme;
  }
  localStorage.setItem(STORAGE_KEY, next);
  syncThemeToggle(next);
}

export function toggleTheme() {
  setTheme(getTheme() === 'dark' ? 'light' : 'dark');
}

function syncThemeToggle(theme) {
  const btn = document.getElementById('themeToggle');
  if (!btn) return;
  const icon = btn.querySelector('.theme-toggle-icon');
  if (theme === 'light') {
    icon.textContent = '🌙';
    btn.title = '切换为深色主题';
    btn.setAttribute('aria-label', '切换为深色主题');
  } else {
    icon.textContent = '☀';
    btn.title = '切换为浅色主题';
    btn.setAttribute('aria-label', '切换为浅色主题');
  }
}

export function initTheme() {
  syncThemeToggle(getTheme());
  document.getElementById('themeToggle')?.addEventListener('click', toggleTheme);
}
