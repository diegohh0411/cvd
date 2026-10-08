import { ChangeDetectionStrategy, Component, computed, DestroyRef, effect, inject, input, signal } from '@angular/core';
import { DomSanitizer, SafeResourceUrl } from '@angular/platform-browser';

import type { AppError } from '../../core/ipc/types';

/**
 * Live preview of the fitted PDF via the webview's built-in PDF viewer
 * (`frame-src blob:` is allowed by the CSP). Keeps the last good document
 * when a compile fails (spec §8).
 */
@Component({
  selector: 'app-pdf-preview',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="bg-muted/40 relative h-full min-h-[480px] overflow-hidden rounded-xl border">
      @if (src(); as s) {
        <iframe [src]="s" title="CV preview" class="h-full min-h-[480px] w-full"></iframe>
      } @else {
        <div class="text-muted-foreground flex h-full min-h-[480px] items-center justify-center text-sm">
          {{ busy() ? 'Rendering preview…' : 'No preview yet' }}
        </div>
      }
      @if (error(); as e) {
        <div role="alert" class="bg-destructive/10 text-destructive absolute inset-x-3 top-3 rounded-md border px-3 py-2 text-xs">
          Could not update the preview: {{ e.message }} Showing the last good version.
        </div>
      }
    </div>
  `,
})
export class PdfPreviewComponent {
  readonly pdf = input<Uint8Array | null>(null);
  readonly error = input<AppError | null>(null);
  readonly busy = input(false);
  private readonly sanitizer = inject(DomSanitizer);
  private readonly url = signal<string | null>(null);
  protected readonly src = computed<SafeResourceUrl | null>(() => {
    const u = this.url();
    return u ? this.sanitizer.bypassSecurityTrustResourceUrl(`${u}#toolbar=0&view=FitH`) : null;
  });

  constructor() {
    effect((onCleanup) => {
      const bytes = this.pdf();
      if (!bytes) return;
      const u = URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'application/pdf' }));
      this.url.set(u);
      onCleanup(() => setTimeout(() => URL.revokeObjectURL(u), 1000));
    });
    inject(DestroyRef).onDestroy(() => {
      const u = this.url();
      if (u) URL.revokeObjectURL(u);
    });
  }
}
