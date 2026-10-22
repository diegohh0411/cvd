import { CdkDrag, CdkDragDrop, CdkDragHandle, CdkDropList, moveItemInArray } from '@angular/cdk/drag-drop';
import { ChangeDetectionStrategy, Component, ElementRef, model, viewChildren } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideGripVertical, lucidePlus, lucideX } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';

/** Long bullets read poorly on a 1-page CV; this only hints, never blocks. */
export const BULLET_HINT_CHARS = 200;

/** Swaps item `i` with its neighbour in direction `d`; returns the new index. */
export function moveBy<T>(list: T[], i: number, d: -1 | 1): number {
  const j = i + d;
  if (j < 0 || j >= list.length) return i;
  moveItemInArray(list, i, j);
  return j;
}

@Component({
  selector: 'app-bullet-list-editor',
  imports: [CdkDropList, CdkDrag, CdkDragHandle, NgIcon, ZardButtonComponent],
  viewProviders: [provideIcons({ lucideGripVertical, lucidePlus, lucideX })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <ol cdkDropList (cdkDropListDropped)="drop($event)" class="flex flex-col gap-1.5" aria-label="Bullets">
      @for (b of bullets(); track $index; let i = $index) {
        <li cdkDrag class="bg-background flex items-start gap-1.5 rounded-md">
          <button
            type="button"
            cdkDragHandle
            class="text-muted-foreground hover:text-foreground mt-2 cursor-grab"
            [attr.aria-label]="'Reorder bullet ' + (i + 1) + ' (Alt+Up/Down)'"
            tabindex="-1"
          >
            <ng-icon name="lucideGripVertical" class="size-4" />
          </button>
          <div class="flex-1">
            <textarea
              #field
              rows="2"
              class="border-input focus-visible:ring-ring/50 focus-visible:border-ring w-full resize-y rounded-md border bg-transparent px-2.5 py-1.5 text-sm outline-none focus-visible:ring-[3px]"
              [value]="b"
              [attr.aria-label]="'Bullet ' + (i + 1)"
              (input)="edit(i, $any($event.target).value)"
              (keydown)="key($event, i)"
            ></textarea>
            @if (b.length > hint) {
              <p class="text-muted-foreground text-xs">{{ b.length }} characters; consider trimming.</p>
            }
          </div>
          <button z-button zType="ghost" zSize="icon-sm" type="button" (click)="remove(i)" [attr.aria-label]="'Remove bullet ' + (i + 1)">
            <ng-icon name="lucideX" />
          </button>
        </li>
      }
    </ol>
    <button z-button zType="outline" zSize="sm" type="button" class="mt-2" (click)="add(bullets().length)">
      <ng-icon name="lucidePlus" /> Add bullet
    </button>
  `,
})
export class BulletListEditorComponent {
  readonly bullets = model<string[]>([]);
  private readonly fields = viewChildren<ElementRef<HTMLTextAreaElement>>('field');
  protected readonly hint = BULLET_HINT_CHARS;

  drop(e: CdkDragDrop<string[]>): void {
    const next = [...this.bullets()];
    moveItemInArray(next, e.previousIndex, e.currentIndex);
    this.bullets.set(next);
  }

  edit(i: number, text: string): void {
    this.bullets.update((l) => l.map((b, k) => (k === i ? text : b)));
  }

  add(at: number): void {
    this.bullets.update((l) => [...l.slice(0, at), '', ...l.slice(at)]);
    this.focus(at);
  }

  remove(i: number): void {
    this.bullets.update((l) => l.filter((_, k) => k !== i));
    this.focus(Math.max(0, i - 1));
  }

  key(e: KeyboardEvent, i: number): void {
    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      const next = [...this.bullets()];
      const j = moveBy(next, i, e.key === 'ArrowUp' ? -1 : 1);
      this.bullets.set(next);
      this.focus(j);
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      this.add(i + 1);
    } else if (e.key === 'Backspace' && this.bullets()[i] === '') {
      e.preventDefault();
      this.remove(i);
    }
  }

  private focus(i: number): void {
    setTimeout(() => this.fields()[i]?.nativeElement.focus());
  }
}
