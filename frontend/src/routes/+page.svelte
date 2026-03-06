<script lang="ts">
  import { apiUrl } from '$lib/api';

  const healthUrl = apiUrl('/api/healthz');
</script>

<svelte:head>
  <title>Hermes | Upload</title>
</svelte:head>

<section class="shell">
  <div class="panel">
    <p class="eyebrow">Phase 1</p>
    <h1>Private video streaming, built incrementally.</h1>
    <p class="lede">
      The repo now has the workspace, shared bootstrap flow, container layout,
      and frontend shell in place. Uploads and playback arrive in the next
      phases.
    </p>

    <div class="actions">
      <a href="/watch/example">Open watch placeholder</a>
      <a href={healthUrl} target="_blank" rel="noreferrer">Check API health</a>
    </div>
  </div>
</section>

<style>
  :global(body) {
    margin: 0;
    font-family: 'IBM Plex Sans', 'Segoe UI', sans-serif;
    background:
      radial-gradient(
        circle at top left,
        rgba(243, 171, 90, 0.35),
        transparent 35%
      ),
      linear-gradient(135deg, #f5efe2 0%, #ebe4d6 45%, #d9d0c2 100%);
    color: #201910;
    min-height: 100vh;
  }

  .shell {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 2rem;
  }

  .panel {
    width: min(42rem, 100%);
    padding: 2.5rem;
    border-radius: 1.5rem;
    background: rgba(255, 252, 246, 0.82);
    border: 1px solid rgba(92, 71, 44, 0.2);
    box-shadow: 0 20px 60px rgba(53, 38, 20, 0.14);
  }

  .eyebrow {
    margin: 0 0 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: 0.8rem;
    color: #805c2e;
  }

  h1 {
    margin: 0;
    font-size: clamp(2rem, 4vw, 3.5rem);
    line-height: 0.95;
  }

  .lede {
    margin: 1.25rem 0 0;
    font-size: 1.05rem;
    line-height: 1.7;
    max-width: 36rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem;
    margin-top: 2rem;
  }

  .actions a {
    text-decoration: none;
    color: #201910;
    background: #f3ab5a;
    padding: 0.85rem 1.1rem;
    border-radius: 999px;
    font-weight: 600;
  }

  .actions a:last-child {
    background: transparent;
    border: 1px solid rgba(32, 25, 16, 0.2);
  }
</style>
