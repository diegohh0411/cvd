import { Injectable, signal } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class FirstRunService {
  readonly visible = signal(false);
  /** Opens once per session; dismissing does not reopen it. */
  private shown = false;

  open(): void {
    if (this.shown) return;
    this.shown = true;
    this.visible.set(true);
  }

  close(): void {
    this.visible.set(false);
  }
}
