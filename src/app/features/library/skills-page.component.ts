import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardCardImports } from '@/shared/components/card/card.imports';
import type { Entry, Skill } from '../../core/ipc/types';
import { LibraryStore } from '../../core/stores/library.store';
import { PageHeaderComponent } from '../../shell/page-header.component';

export interface SkillGroup {
  category: string;
  skills: { skill: Skill; uses: number }[];
}

/** Skills grouped by category (uncategorized last), each with how many active entries use it. */
export function groupSkills(skills: readonly Skill[], entries: readonly Entry[]): SkillGroup[] {
  const uses = new Map<number, number>();
  for (const e of entries.filter((x) => !x.archived)) for (const s of e.skills) uses.set(s.id, (uses.get(s.id) ?? 0) + 1);
  const groups = new Map<string, SkillGroup['skills']>();
  for (const s of skills) {
    const key = s.category.trim() || 'Uncategorized';
    groups.set(key, [...(groups.get(key) ?? []), { skill: s, uses: uses.get(s.id) ?? 0 }]);
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a === 'Uncategorized' ? 1 : b === 'Uncategorized' ? -1 : a.localeCompare(b)))
    .map(([category, list]) => ({
      category,
      skills: list.sort((a, b) => b.uses - a.uses || a.skill.name.localeCompare(b.skill.name)),
    }));
}

@Component({
  selector: 'app-skills-page',
  imports: [PageHeaderComponent, ZardBadgeComponent, ...ZardCardImports],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Library', link: '/library/experience' }, { label: 'Skills' }]" />
    <div class="flex flex-1 flex-col gap-4 p-4">
      <div class="grid auto-rows-min gap-4 sm:grid-cols-3">
        <z-card class="gap-1 py-4"><z-card-content>
          <div class="text-muted-foreground text-xs">Skills</div>
          <div class="text-2xl font-semibold tabular-nums">{{ library.skills().length }}</div>
        </z-card-content></z-card>
        <z-card class="gap-1 py-4"><z-card-content>
          <div class="text-muted-foreground text-xs">Unused</div>
          <div class="text-2xl font-semibold tabular-nums">{{ unused() }}</div>
        </z-card-content></z-card>
        <z-card class="gap-1 py-4"><z-card-content>
          <div class="text-muted-foreground text-xs">Categories</div>
          <div class="text-2xl font-semibold tabular-nums">{{ groups().length }}</div>
        </z-card-content></z-card>
      </div>
      <p class="text-muted-foreground text-sm">
        Skills are created from the entry editor. Each match asks the model whether the role values each skill; the strongest
        matches form the CV's skills line.
      </p>
      @for (g of groups(); track g.category) {
        <section>
          <h2 class="mb-2 text-sm font-medium">{{ g.category }}</h2>
          <div class="flex flex-wrap gap-1.5">
            @for (s of g.skills; track s.skill.id) {
              <z-badge [zType]="s.uses ? 'secondary' : 'outline'">
                {{ s.skill.name }} <span class="text-muted-foreground tabular-nums">{{ s.uses }}</span>
              </z-badge>
            }
          </div>
        </section>
      } @empty {
        <p class="text-muted-foreground rounded-xl border border-dashed p-10 text-center text-sm">
          No skills yet. Add some to an entry in Experience or Projects.
        </p>
      }
    </div>
  `,
})
export class SkillsPageComponent {
  protected readonly library = inject(LibraryStore);
  protected readonly groups = computed(() => groupSkills(this.library.skills(), this.library.entries()));
  protected readonly unused = computed(() => this.groups().reduce((n, g) => n + g.skills.filter((s) => !s.uses).length, 0));
}
