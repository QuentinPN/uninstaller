<script lang="ts">
import { portal } from "../ui/portal";
import type { ListDocument } from "./export";

let { doc }: { doc: ListDocument } = $props();
const [head = [], ...body] = $derived(doc.rows);
</script>

<!-- Outside #app, which printing hides (app.css): only this sheet reaches the paper. -->
<section class="sheet" use:portal>
  <h1>{doc.title}</h1>
  <p>{doc.about}</p>
  <table>
    <thead>
      <tr>
        {#each head as text, i (i)}<th>{text}</th>{/each}
      </tr>
    </thead>
    <tbody>
      {#each body as row, r (r)}
        <tr>
          {#each row as text, i (i)}<td>{text}</td>{/each}
        </tr>
      {/each}
    </tbody>
  </table>
</section>

<style>
  .sheet {
    display: none;
  }
  @media print {
    .sheet {
      display: block;
      color: var(--print-ink);
      font-size: var(--print-font-size);
    }
  }
  h1 {
    margin: 0;
    font-size: var(--font-size-title);
  }
  p {
    margin: var(--space-1) 0 var(--space-4);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--print-rule);
    text-align: left;
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  /* Repeated at the top of each page. */
  thead {
    display: table-header-group;
  }
  tr {
    break-inside: avoid;
  }
</style>
