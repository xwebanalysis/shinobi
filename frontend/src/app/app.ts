import { Component, OnInit, inject, signal } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';

import { ApiService } from './core/api.service';
import { ExtractorStatus } from './core/models';
import { ThemeService } from './core/theme.service';

@Component({
  selector: 'app-root',
  imports: [RouterLink, RouterLinkActive, RouterOutlet],
  templateUrl: './app.html',
  styleUrl: './app.scss',
})
export class App implements OnInit {
  private readonly api = inject(ApiService);
  private readonly theme = inject(ThemeService);

  /** null = not probed yet, true/false = /api/health reachable or not. */
  protected readonly rustOnline = signal<boolean | null>(null);
  protected readonly extractor = signal<ExtractorStatus>('unknown');

  ngOnInit(): void {
    this.theme.initTheme();

    this.api.health().subscribe({
      next: () => this.rustOnline.set(true),
      error: () => this.rustOnline.set(false),
    });

    this.api.extractorStatus().subscribe({
      next: (status) => this.extractor.set(status),
    });
  }

  protected toggleTheme(): void {
    this.theme.toggleTheme();
  }

  protected themeLabel(): string {
    return this.theme.nextThemeLabel();
  }

  protected rustClass(): string {
    const online = this.rustOnline();
    if (online === null) {
      return 'unknown';
    }
    return online ? 'online' : 'offline';
  }

  protected rustTitle(): string {
    const online = this.rustOnline();
    if (online === null) {
      return 'Rust API: probing /api/health';
    }
    return online ? 'Rust API online (/api/health)' : 'Rust API offline (/api/health failed)';
  }

  protected extractorTitle(): string {
    switch (this.extractor()) {
      case 'online':
        return 'Python extractor online (proxied /api/python/docs reachable)';
      case 'offline':
        return 'Python extractor offline (proxied /api/python/docs returned 502)';
      default:
        return 'Python extractor status unknown: no proxied health endpoint (probing /api/python/docs)';
    }
  }
}
