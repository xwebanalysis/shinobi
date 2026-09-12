import { TestBed } from '@angular/core/testing';

import { ThemeService } from './theme.service';

describe('ThemeService', () => {
  beforeEach(() => {
    localStorage.clear();
    document.body.className = '';
    TestBed.configureTestingModule({});
  });

  it('should default to the dark theme', () => {
    const service = TestBed.inject(ThemeService);
    service.initTheme();

    expect(service.currentTheme()).toBe('theme-dark');
    expect(document.body.classList.contains('theme-dark')).toBe(true);
    expect(localStorage.getItem('shinobi-theme')).toBe('theme-dark');
  });

  it('should restore the stored light theme', () => {
    localStorage.setItem('shinobi-theme', 'theme-light');
    const service = TestBed.inject(ThemeService);
    service.initTheme();

    expect(service.currentTheme()).toBe('theme-light');
    expect(document.body.classList.contains('theme-light')).toBe(true);
    expect(service.nextThemeLabel()).toBe('DARK MODE');
  });

  it('should toggle the theme and persist the choice', () => {
    const service = TestBed.inject(ThemeService);
    service.initTheme();
    service.toggleTheme();

    expect(service.currentTheme()).toBe('theme-light');
    expect(document.body.classList.contains('theme-light')).toBe(true);
    expect(document.body.classList.contains('theme-dark')).toBe(false);
    expect(localStorage.getItem('shinobi-theme')).toBe('theme-light');

    service.toggleTheme();
    expect(service.currentTheme()).toBe('theme-dark');
    expect(localStorage.getItem('shinobi-theme')).toBe('theme-dark');
  });
});
