<script lang="ts">
  import { goto } from '$app/navigation';
  import { apiUrl } from '$lib/api';

  const healthUrl = apiUrl('/api/healthz');
  const uploadUrl = apiUrl('/api/videos');

  let file: File | null = null;
  let error = '';
  let submitting = false;

  async function submitUpload(event: SubmitEvent) {
    event.preventDefault();
    error = '';

    if (!file) {
      error = 'Choose a video file first.';
      return;
    }

    const formData = new FormData();
    formData.append('video', file);
    submitting = true;

    try {
      const response = await fetch(uploadUrl, {
        method: 'POST',
        body: formData
      });
      const payload = await response.json();

      if (!response.ok) {
        error = payload?.error ?? 'Upload failed.';
        return;
      }

      await goto(payload.shareable_url);
    } catch (requestError) {
      error =
        requestError instanceof Error
          ? requestError.message
          : 'Upload request failed.';
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:head>
  <title>Hermes | Upload</title>
</svelte:head>

<section class="shell">
  <div class="panel hero">
    <p class="eyebrow">Phase 2</p>
    <h1>Upload once. Watch immediately.</h1>
    <p class="lede">
      Hermes now accepts a raw video upload, stores it, queues background work,
      and gives you a watch page that can start from the original file before
      HLS exists.
    </p>
  </div>

  <div class="panel flow">
    <form class="upload-form" on:submit={submitUpload}>
      <label class="picker" for="video">
        <span>Video file</span>
        <input
          id="video"
          name="video"
          type="file"
          accept="video/mp4,video/webm,video/quicktime,video/x-matroska,.mkv"
          on:change={(event) => {
            const input = event.currentTarget as HTMLInputElement;
            file = input.files?.[0] ?? null;
          }}
        />
      </label>

      <div class="file-row">
        <strong>{file ? file.name : 'No file selected yet'}</strong>
        {#if file}
          <span>{Math.round(file.size / 1024 / 1024)} MB</span>
        {/if}
      </div>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <div class="actions">
        <button type="submit" disabled={submitting}>
          {submitting ? 'Uploading...' : 'Upload Video'}
        </button>
      </div>
    </form>

    <div class="meta">
      <p>Accepted formats: MP4, MOV, WebM, MKV</p>
      <a href="/watch/example">Open watch page shell</a>
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
    align-content: center;
    padding: 2rem;
    gap: 1.25rem;
  }

  .panel {
    width: min(46rem, 100%);
    padding: 2rem 2.2rem;
    border-radius: 1.5rem;
    background: rgba(255, 252, 246, 0.82);
    border: 1px solid rgba(92, 71, 44, 0.2);
    box-shadow: 0 20px 60px rgba(53, 38, 20, 0.14);
  }

  .hero {
    padding-bottom: 1.6rem;
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

  .flow {
    display: grid;
    gap: 1.25rem;
  }

  .upload-form {
    display: grid;
    gap: 1rem;
  }

  .picker {
    display: grid;
    gap: 0.7rem;
    padding: 1rem;
    border-radius: 1rem;
    background: rgba(255, 246, 231, 0.85);
    border: 1px dashed rgba(92, 71, 44, 0.28);
    font-weight: 600;
  }

  input[type='file'] {
    font: inherit;
  }

  .file-row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    font-size: 0.95rem;
  }

  .file-row span {
    color: #6d5840;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem;
  }

  button {
    font: inherit;
    color: #201910;
    background: #f3ab5a;
    padding: 0.85rem 1.1rem;
    border-radius: 999px;
    font-weight: 600;
    border: 0;
    cursor: pointer;
    text-decoration: none;
  }

  button:disabled {
    opacity: 0.7;
    cursor: progress;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem 1.1rem;
    align-items: center;
    color: #5a4935;
    font-size: 0.95rem;
  }

  .meta p {
    margin: 0;
  }

  .meta a:last-child {
    background: transparent;
    border: 1px solid rgba(32, 25, 16, 0.2);
  }

  .meta a {
    color: #201910;
    text-decoration: none;
    font-weight: 600;
  }

  .error {
    margin: 0;
    color: #9e2b25;
    font-weight: 600;
  }

  @media (max-width: 640px) {
    .panel {
      padding: 1.5rem;
    }

    .file-row {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
