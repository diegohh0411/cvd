import { Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { onInboxChanged, onScanFinished, onWatcherStatus } from '../ipc/events';
import type { InboxFilter, ProcessedEmail, WatcherStatus } from '../ipc/types';

/**
 * The inbox of email results and the mail watcher's state, shared by the
 * Inbox page and the sidebar badge. Events are subscribed once.
 */
@Injectable({ providedIn: 'root' })
export class InboxStore {
  readonly items = signal<ProcessedEmail[]>([]);
  readonly filter = signal<InboxFilter>('all');
  readonly unread = signal(0);
  readonly watcher = signal<WatcherStatus | null>(null);

  private ready: Promise<void> | null = null;

  ensureReady(): Promise<void> {
    return (this.ready ??= this.setUp());
  }

  async setFilter(filter: InboxFilter): Promise<void> {
    this.filter.set(filter);
    await this.refresh();
  }

  async refresh(): Promise<void> {
    const [items, unread] = await Promise.all([api.listInbox(this.filter()), api.unreadCount()]);
    this.items.set(items);
    this.unread.set(unread);
  }

  async markRead(messageId: string): Promise<void> {
    const row = this.items().find((r) => r.message_id === messageId);
    if (!row || row.read_at !== null) return;
    await api.markRead(messageId);
    await this.refresh();
  }

  async markAllRead(): Promise<void> {
    await api.markAllRead();
    await this.refresh();
  }

  async apply(messageId: string): Promise<void> {
    await api.applySuggestion(messageId);
    await this.refresh();
  }

  async dismiss(messageId: string): Promise<void> {
    await api.dismissSuggestion(messageId);
    await this.refresh();
  }

  async undo(messageId: string): Promise<void> {
    await api.undoApplied(messageId);
    await this.refresh();
  }

  async retry(): Promise<void> {
    await api.retryWatcher();
  }

  private async setUp(): Promise<void> {
    const refresh = () => void this.refresh().catch(() => undefined);
    await Promise.all([
      onWatcherStatus((s) => this.watcher.set(s)),
      onInboxChanged(refresh),
      onScanFinished(refresh),
    ]);
    const [status] = await Promise.all([api.getWatcherStatus().catch(() => null), this.refresh().catch(() => undefined)]);
    if (status && this.watcher() === null) this.watcher.set(status);
  }
}
