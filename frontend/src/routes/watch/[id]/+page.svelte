<script lang="ts">
  import { page } from '$app/stores';
  import { apiUrl } from '$lib/api';

  $: id = $page.params.id;
  $: streamUrl = apiUrl(`/api/videos/${id}/stream`);
</script>

<svelte:head>
  <title>Hermes | Watch</title>
</svelte:head>

<section class="watch-shell">
  <div class="watch-card">
    <p class="eyebrow">Watch Placeholder</p>
    <h1>{id}</h1>
    <p>
      This page will switch from immediate raw playback to HLS once the worker
      pipeline is added in later phases.
    </p>
    <code>{streamUrl}</code>
  </div>
</section>

<style>
  .watch-shell {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 2rem;
  }

  .watch-card {
    width: min(38rem, 100%);
    padding: 2rem;
    border-radius: 1.25rem;
    background: rgba(255, 252, 246, 0.84);
    border: 1px solid rgba(92, 71, 44, 0.18);
    box-shadow: 0 16px 40px rgba(53, 38, 20, 0.12);
  }

  .eyebrow {
    margin: 0 0 0.5rem;
    font-size: 0.78rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: #805c2e;
  }

  h1 {
    margin: 0;
    font-size: clamp(1.8rem, 4vw, 2.8rem);
  }

  p {
    line-height: 1.65;
  }

  code {
    display: block;
    margin-top: 1.25rem;
    padding: 1rem;
    border-radius: 0.85rem;
    background: rgba(32, 25, 16, 0.08);
    overflow: auto;
  }
</style>
