<script lang="ts">
import { onMount } from "svelte";
import { fr } from "./lib/i18n/fr";
import { commands } from "./lib/ipc/bindings";
import NavPill from "./lib/nav/NavPill.svelte";
import { type SectionId, sectionForShortcut } from "./lib/nav/sections";
import EmptyState from "./lib/ui/EmptyState.svelte";
import TitleBar from "./lib/window/TitleBar.svelte";

let section = $state<SectionId>("programs");
let elevated = $state(false);

onMount(() => {
  // Outside Tauri (UI previews) the call fails: stay on "not elevated".
  void commands.appInfo().then(
    (info) => (elevated = info.elevated),
    () => {},
  );
});

function onkeydown(event: KeyboardEvent) {
  const target = sectionForShortcut(event);
  if (!target) return;
  event.preventDefault();
  section = target;
}
</script>

<svelte:window {onkeydown} />

<TitleBar {elevated} />
<header>
  <NavPill current={section} onselect={(id) => (section = id)} />
</header>
<main>
  <h1>{fr.nav[section]}</h1>
  <EmptyState title={fr.upcoming.title} text={fr.upcoming[section]} />
</main>

<style>
  header {
    display: flex;
    justify-content: center;
    padding: var(--space-3) var(--space-4);
  }
  main {
    padding: 0 var(--space-8);
  }
  h1 {
    margin: var(--space-6) 0 0;
    font-size: var(--font-size-title);
  }
</style>
