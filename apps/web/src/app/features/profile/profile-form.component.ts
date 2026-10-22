import { ChangeDetectionStrategy, Component, model } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ZardInputComponent } from '@/shared/components/input/input.component';
import type { Profile } from '../../core/ipc/types';

export function profileErrors(p: Profile): string[] {
  const errors: string[] = [];
  if (!p.name.trim()) errors.push('Name is required.');
  if (p.email.trim() && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(p.email.trim())) errors.push('Email looks invalid.');
  return errors;
}

export function cleanProfile(p: Profile): Profile {
  return {
    name: p.name.trim(),
    headline: p.headline.trim(),
    email: p.email.trim(),
    phone: p.phone.trim(),
    location: p.location.trim(),
    links: p.links.map((l) => l.trim()).filter(Boolean),
    summary: p.summary.trim(),
  };
}

@Component({
  selector: 'app-profile-form',
  imports: [FormsModule, ZardInputComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-3">
      <div class="grid gap-3 sm:grid-cols-2">
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="p-name">Full name</label>
          <input z-input id="p-name" [ngModel]="profile().name" (ngModelChange)="patch({ name: $event })" autocomplete="name" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="p-headline">Headline</label>
          <input z-input id="p-headline" [ngModel]="profile().headline" (ngModelChange)="patch({ headline: $event })" placeholder="Backend engineer" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="p-email">Email</label>
          <input z-input id="p-email" type="email" [ngModel]="profile().email" (ngModelChange)="patch({ email: $event })" autocomplete="email" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="p-phone">Phone</label>
          <input z-input id="p-phone" type="tel" [ngModel]="profile().phone" (ngModelChange)="patch({ phone: $event })" autocomplete="tel" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="p-location">Location</label>
          <input z-input id="p-location" [ngModel]="profile().location" (ngModelChange)="patch({ location: $event })" placeholder="Berlin, Germany" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="p-links">Links (one per line)</label>
          <textarea
            id="p-links"
            rows="2"
            class="border-input rounded-md border bg-transparent px-2.5 py-1.5 text-sm"
            [ngModel]="profile().links.join('\\n')"
            (ngModelChange)="patch({ links: $event.split('\\n') })"
            placeholder="github.com/you"
          ></textarea>
        </div>
      </div>
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="p-summary">Summary</label>
        <textarea
          id="p-summary"
          rows="3"
          class="border-input rounded-md border bg-transparent px-2.5 py-1.5 text-sm"
          [ngModel]="profile().summary"
          (ngModelChange)="patch({ summary: $event })"
        ></textarea>
      </div>
    </div>
  `,
})
export class ProfileFormComponent {
  readonly profile = model.required<Profile>();

  patch(p: Partial<Profile>): void {
    this.profile.update((x) => ({ ...x, ...p }));
  }
}
