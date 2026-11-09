<script lang="ts">
import { formatDate, formatSize } from "../format";
import { fr } from "../i18n/fr";
import { commands, type Program } from "../ipc/bindings";

interface Props {
  program: Program;
  /** Measured on disk; absent until computed or without an install folder. */
  diskSizeKib: number | undefined;
  onclose: () => void;
}

let { program, diskSizeKib, onclose }: Props = $props();

const t = fr.programs.details;
type Field = { label: string; value: string | null | undefined };

/** How long a confirmation such as "Copié" stays visible. */
const STATUS_MS = 2000;

let status = $state("");
let statusTimer: ReturnType<typeof setTimeout> | undefined;

/** Empty values are dropped: the panel lists only what the installer declared. */
const facts = $derived(
  (<Field[]>[
    { label: t.version, value: program.version },
    {
      label: t.diskSize,
      value: diskSizeKib != null ? formatSize(diskSizeKib) : null,
    },
    {
      label: t.size,
      value:
        program.estimatedSizeKib != null
          ? formatSize(program.estimatedSizeKib)
          : null,
    },
    {
      label: t.installed,
      value: program.installDate ? formatDate(program.installDate) : null,
    },
    { label: t.scope, value: fr.programs.scopeHint[program.scope] },
    { label: t.account, value: program.profile },
    { label: t.installer, value: program.msi ? t.msi : null },
    { label: t.comments, value: program.comments },
  ]).filter((f) => f.value),
);

const copyable = $derived(
  (<Field[]>[
    { label: t.folder, value: program.installLocation },
    { label: t.site, value: program.url },
    { label: t.uninstall, value: program.uninstallString },
    { label: t.quietUninstall, value: program.quietUninstallString },
    { label: t.registryKey, value: program.registryKey },
  ]).filter((f): f is { label: string; value: string } => !!f.value),
);

function say(message: string) {
  status = message;
  clearTimeout(statusTimer);
  statusTimer = setTimeout(() => (status = ""), STATUS_MS);
}

function copy(value: string) {
  navigator.clipboard.writeText(value).then(
    () => say(t.copied),
    () => say(t.copyFailed),
  );
}

function openFolder() {
  commands.openInstallFolder(program.id).then(
    (opened) => !opened && say(t.folderMissing),
    () => say(t.folderMissing),
  );
}
</script>

<aside class="panel" aria-label={t.label}>
  <header>
    <div class="titles">
      <h2>{program.name}</h2>
      {#if program.publisher}<p class="publisher">{program.publisher}</p>{/if}
    </div>
    <button class="icon" aria-label={t.close} title={t.close} onclick={onclose}>
      <span aria-hidden="true">&#xE711;</span>
    </button>
  </header>

  <dl>
    {#each facts as fact (fact.label)}
      <dt>{fact.label}</dt>
      <dd>{fact.value}</dd>
    {/each}
    {#each copyable as field (field.label)}
      <dt>{field.label}</dt>
      <dd class="copyable">
        <span class="path">{field.value}</span>
        <button
          class="icon"
          aria-label={t.copyField(field.label)}
          title={t.copy}
          onclick={() => copy(field.value)}
        >
          <span aria-hidden="true">&#xE8C8;</span>
        </button>
      </dd>
    {/each}
  </dl>

  {#if program.installLocation}
    <button class="action" onclick={openFolder}>{t.openFolder}</button>
  {/if}
  <p class="status" role="status">{status}</p>
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: var(--panel-width);
    min-height: 0;
    padding: var(--space-4);
    overflow-y: auto;
    border-radius: var(--radius-lg);
    background: var(--material-thick);
    backdrop-filter: var(--material-blur);
    animation: enter var(--motion-base) var(--ease-out);
  }
  @keyframes enter {
    from {
      opacity: 0;
      transform: translateX(var(--space-6));
    }
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--font-size-body);
    overflow-wrap: anywhere;
  }
  .publisher {
    margin: var(--space-1) 0 0;
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  dl {
    display: flex;
    flex-direction: column;
    margin: 0;
    font-size: var(--font-size-sm);
  }
  dt {
    margin-top: var(--space-3);
    color: var(--color-text-muted);
  }
  dd {
    margin: var(--space-1) 0 0;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .copyable {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
  }
  .path {
    flex: 1;
    min-width: 0;
    color: var(--color-text-secondary);
  }
  .icon {
    flex: none;
    padding: var(--space-1);
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--color-text-secondary);
    font-family: var(--font-icons);
    cursor: pointer;
    transition: background var(--motion-fast);
  }
  .icon:hover {
    background: var(--fill-hover);
    color: var(--color-text);
  }
  .icon:active {
    background: var(--fill-pressed);
  }
  .action {
    align-self: flex-start;
    padding: var(--space-2) var(--space-4);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    cursor: pointer;
    transition: background var(--motion-fast);
  }
  .action:hover {
    background: var(--fill-glass-hover);
  }
  .status {
    min-height: 1lh;
    margin: 0;
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
</style>
