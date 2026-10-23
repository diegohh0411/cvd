import { ChangeDetectionStrategy, Component, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardInputComponent } from '@/shared/components/input/input.component';
import { messageFor } from '../../core/ipc/errors';
import { platform } from '../../core/platform/runtime';

/** Sign-in and sign-up are one flow: both providers create unknown accounts. */
@Component({
  selector: 'app-sign-in',
  imports: [ZardButtonComponent, ZardInputComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <main class="flex min-h-svh items-center justify-center p-4">
      <div class="grid w-full max-w-sm gap-4">
        <div>
          <h1 class="text-xl font-semibold">Sign in to CVD</h1>
          <p class="text-muted-foreground text-sm">New here? Signing in creates your account.</p>
        </div>
        <button z-button zType="outline" [disabled]="busy()" (click)="google()">Continue with Google</button>
        <form class="grid gap-2" (submit)="$event.preventDefault(); email(address())">
          <label class="grid gap-1.5 text-sm">
            <span class="font-medium">Email</span>
            <input z-input (input)="address.set($any($event.target).value)" type="email" required autocomplete="email" placeholder="you@example.com" />
          </label>
          <button z-button type="submit" [disabled]="busy()">Email me a sign-in link</button>
        </form>
        @if (sent()) {
          <p class="text-sm" role="status">Check {{ sent() }} for a sign-in link.</p>
        }
        @if (error()) {
          <p class="text-destructive text-sm" role="alert">{{ error() }}</p>
        }
      </div>
    </main>
  `,
})
export class SignInComponent {
  protected readonly address = signal('');
  protected readonly busy = signal(false);
  protected readonly sent = signal<string | null>(null);
  protected readonly error = signal<string | null>(null);

  protected google(): Promise<void> {
    return this.run(() => platform.auth.signInWithGoogle());
  }

  protected email(address: string): Promise<void> {
    if (!address.trim()) return Promise.resolve();
    return this.run(async () => {
      await platform.auth.signInWithEmail(address.trim());
      this.sent.set(address.trim());
    });
  }

  private async run(action: () => Promise<void>): Promise<void> {
    this.busy.set(true);
    this.error.set(null);
    try {
      await action();
    } catch (e) {
      this.error.set(messageFor(e));
    } finally {
      this.busy.set(false);
    }
  }
}
