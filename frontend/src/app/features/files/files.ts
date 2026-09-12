import { Component, HostListener, OnDestroy, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ApiService, requestErrorMessage } from '../../core/api.service';
import { FileInfo } from '../../core/models';

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'];

@Component({
  selector: 'app-files',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './files.html',
  styleUrl: './files.scss',
})
export class FilesComponent implements OnInit, OnDestroy {
  private readonly api = inject(ApiService);

  protected readonly files = signal<FileInfo[]>([]);
  protected readonly total = signal(0);
  protected readonly page = signal(0);
  protected readonly loading = signal(false);
  protected readonly searchQuery = signal('');

  protected readonly previewFile = signal<FileInfo | null>(null);
  protected readonly previewText = signal<string | null>(null);
  protected readonly previewImageUrl = signal<string | null>(null);
  protected readonly previewLoading = signal(false);

  protected readonly pageSize = 25;

  protected readonly filteredFiles = computed(() => {
    const query = this.searchQuery().toLowerCase();
    const visible = this.files().filter((file) => !file.is_dir);
    if (!query) {
      return visible;
    }
    return visible.filter(
      (file) => file.path.toLowerCase().includes(query) || file.name.toLowerCase().includes(query),
    );
  });

  private statusTimeout: ReturnType<typeof setTimeout> | null = null;

  @HostListener('document:keydown', ['$event'])
  handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      this.closePreview();
    }
  }

  ngOnInit(): void {
    this.loadFiles();
  }

  ngOnDestroy(): void {
    if (this.statusTimeout) {
      clearTimeout(this.statusTimeout);
    }
  }

  protected loadFiles(): void {
    this.loading.set(true);
    this.api.listFiles('', this.page(), this.pageSize).subscribe({
      next: (response) => {
        this.files.set(response.items);
        this.total.set(response.total);
        this.loading.set(false);
      },
      error: () => this.loading.set(false),
    });
  }

  protected preview(file: FileInfo): void {
    this.previewFile.set(file);
    this.previewText.set(null);
    this.previewImageUrl.set(null);
    this.previewLoading.set(true);

    const extension = this.extension(file.name);
    if (IMAGE_EXTENSIONS.includes(extension)) {
      this.previewImageUrl.set(this.api.fileUrl(file.path));
      this.previewLoading.set(false);
      return;
    }

    this.api.getFileText(file.path).subscribe({
      next: (text) => {
        this.previewText.set(
          text.length > 50000 ? `${text.slice(0, 50000)}\n\n[... TRUNCATED ...]` : text,
        );
        this.previewLoading.set(false);
      },
      error: (error: unknown) => {
        this.previewText.set(`[ERROR: ${requestErrorMessage(error)}]`);
        this.previewLoading.set(false);
      },
    });
  }

  protected closePreview(): void {
    this.previewFile.set(null);
    this.previewText.set(null);
    this.previewImageUrl.set(null);
  }

  protected prevPage(): void {
    this.page.update((page) => Math.max(0, page - this.pageSize));
    this.loadFiles();
  }

  protected nextPage(): void {
    if (this.page() + this.pageSize < this.total()) {
      this.page.update((page) => page + this.pageSize);
      this.loadFiles();
    }
  }

  protected pageInfo(): string {
    if (!this.total()) {
      return '';
    }
    return `${this.page() + 1}-${Math.min(this.page() + this.pageSize, this.total())} / ${this.total()}`;
  }

  protected extension(name: string): string {
    return name.split('.').pop()?.toLowerCase() ?? '';
  }

  protected formatSize(bytes: number): string {
    if (bytes < 1024) {
      return `${bytes}B`;
    }
    if (bytes < 1024 * 1024) {
      return `${(bytes / 1024).toFixed(1)}KB`;
    }
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  }
}
