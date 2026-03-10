<script lang="ts">
  const title = 'Hermes';
</script>

<svelte:head>
  <title>{title}</title>
  <meta
    name="description"
    content="Hermes is a phased build for a private video upload and streaming service."
  />
</svelte:head>

<slot />

<style>
  :global(:root) {
    --sky-950: #0f2742;
    --sky-900: #173c63;
    --sky-800: #245684;
    --sky-700: #2f70a2;
    --sky-600: #4d92c7;
    --sky-500: #6cb0e4;
    --sky-300: #b9ddf7;
    --sky-200: #d7ecfb;
    --sky-100: #eef8ff;
    --cloud-100: rgba(255, 255, 255, 0.72);
    --cloud-200: rgba(255, 255, 255, 0.84);
    --ink-900: #10243d;
    --ink-700: #365575;
    --ink-500: #58789a;
    --danger-700: #a72d3b;
    --danger-100: rgba(212, 63, 87, 0.1);
    --warning-700: #9a6816;
    --warning-100: rgba(231, 165, 42, 0.16);
    --success-700: #1f6e5a;
    --success-500: #2d9d7e;
    --success-100: rgba(36, 154, 118, 0.14);
    --border-soft: rgba(29, 73, 112, 0.12);
    --border-strong: rgba(29, 73, 112, 0.24);
    --shadow-soft: 0 22px 50px rgba(11, 39, 70, 0.14);
  }

  :global(html) {
    background:
      radial-gradient(
        circle at top left,
        rgba(133, 193, 233, 0.42),
        transparent 28%
      ),
      radial-gradient(
        circle at top right,
        rgba(216, 240, 255, 0.8),
        transparent 24%
      ),
      linear-gradient(180deg, #f2fbff 0%, #deeffb 52%, #cae5f8 100%);
  }

  :global(body) {
    margin: 0;
    min-height: 100vh;
    color: var(--ink-900);
    font-family: 'IBM Plex Sans', 'Segoe UI', sans-serif;
  }

  :global(*) {
    box-sizing: border-box;
  }

  :global(a) {
    color: inherit;
  }
</style>
