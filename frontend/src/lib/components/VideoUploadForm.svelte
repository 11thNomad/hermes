<script lang="ts">
  import { browser } from '$app/environment';
  import { createEventDispatcher } from 'svelte';
  import { apiUrl } from '$lib/api';

  type UploadVideoResponse = {
    id: string;
    shareable_url: string;
  };

  type DirectUploadInitResponse = {
    video_id: string;
    upload_url: string;
    shareable_url: string;
  };

  export let compact = false;
  export let eyebrow = 'Upload';
  export let title = 'Choose a video';
  export let description =
    'Hermes stores the raw source first, then swaps the player to HLS when processing is ready.';
  export let ctaLabel = 'Upload video';
  export let limitLabel = '1 GB max';

  const dispatch = createEventDispatcher<{
    uploaded: UploadVideoResponse;
  }>();

  const initUploadUrl = apiUrl('/api/uploads/init');

  let file: File | null = null;
  let error = '';
  let uploading = false;
  let finalizing = false;
  let completed = false;
  let uploadedVideoId = '';
  let progressPercent = 0;
  let progressBytes = 0;
  let uploadTotalBytes = 0;
  let shareablePath = '';
  let shareableUrl = '';
  let copiedShareable = false;

  async function submitUpload(event: SubmitEvent) {
    event.preventDefault();
    error = '';
    completed = false;
    uploadedVideoId = '';
    progressPercent = 0;
    progressBytes = 0;
    uploadTotalBytes = file?.size ?? 0;
    shareablePath = '';
    shareableUrl = '';
    copiedShareable = false;

    if (!file) {
      error = 'Choose a video file first.';
      return;
    }

    try {
      const initialized = await initDirectUpload(file);
      shareablePath = initialized.shareable_url;
      shareableUrl = toAbsoluteShareableUrl(initialized.shareable_url);

      uploading = true;
      await uploadToObjectStorage(file, initialized.upload_url);
      uploading = false;
      finalizing = true;

      const payload = await completeDirectUpload(
        initialized.video_id,
        file.name
      );
      uploadedVideoId = payload.id;
      progressBytes = uploadTotalBytes;
      progressPercent = 100;
      completed = true;
      shareablePath = payload.shareable_url;
      shareableUrl = toAbsoluteShareableUrl(payload.shareable_url);
      dispatch('uploaded', payload);
    } catch (requestError) {
      error =
        requestError instanceof Error
          ? requestError.message
          : 'Upload request failed.';
    } finally {
      uploading = false;
      finalizing = false;
    }
  }

  async function initDirectUpload(
    selectedFile: File
  ): Promise<DirectUploadInitResponse> {
    const response = await fetch(initUploadUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        filename: selectedFile.name,
        size_bytes: selectedFile.size
      })
    });
    const payload = (await response.json()) as
      | DirectUploadInitResponse
      | { error?: string }
      | null;

    if (!response.ok) {
      const message =
        payload && typeof payload === 'object' && 'error' in payload
          ? typeof payload.error === 'string'
            ? payload.error
            : null
          : null;
      throw new Error(message ?? 'Failed to initialize upload.');
    }

    return payload as DirectUploadInitResponse;
  }

  function uploadToObjectStorage(
    fileToUpload: File,
    uploadUrl: string
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      const request = new XMLHttpRequest();
      request.open('PUT', uploadUrl);

      request.upload.onprogress = (event) => {
        if (!event.lengthComputable) {
          return;
        }

        progressBytes = event.loaded;
        uploadTotalBytes = event.total;
        progressPercent = Math.round((event.loaded / event.total) * 100);
      };

      request.onerror = () => {
        reject(new Error('Direct upload to object storage failed.'));
      };

      request.onload = () => {
        if (request.status < 200 || request.status >= 300) {
          reject(
            new Error(
              `Object storage upload failed with HTTP ${request.status}.`
            )
          );
          return;
        }

        resolve();
      };

      request.send(fileToUpload);
    });
  }

  async function completeDirectUpload(
    videoId: string,
    filename: string
  ): Promise<UploadVideoResponse> {
    const response = await fetch(apiUrl(`/api/uploads/${videoId}/complete`), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ filename })
    });
    const payload = (await response.json()) as
      | UploadVideoResponse
      | { error?: string }
      | null;

    if (!response.ok) {
      const message =
        payload && typeof payload === 'object' && 'error' in payload
          ? typeof payload.error === 'string'
            ? payload.error
            : null
          : null;
      throw new Error(message ?? 'Failed to finalize upload.');
    }

    return payload as UploadVideoResponse;
  }

  async function copyShareableUrl() {
    if (!browser || !shareableUrl) {
      return;
    }

    try {
      await navigator.clipboard.writeText(shareableUrl);
      copiedShareable = true;
    } catch {
      error = 'Failed to copy the shareable URL.';
    }
  }

  function toAbsoluteShareableUrl(path: string): string {
    if (!browser) {
      return path;
    }

    return new URL(path, window.location.origin).toString();
  }

  function formatBytes(value: number): string {
    if (value <= 0) {
      return '0 MB';
    }

    return `${(value / 1024 / 1024).toFixed(value >= 100 * 1024 * 1024 ? 0 : 1)} MB`;
  }
</script>

<form class:compact class="upload-form" on:submit={submitUpload}>
  <div class="section-head">
    <div class="copy-block">
      <p class="eyebrow">{eyebrow}</p>
      <h2>{title}</h2>
      <p>{description}</p>
    </div>
    <div class="pill-row">
      <span class="format-pill">MP4, MOV, WebM, MKV</span>
      <span class="limit-pill">{limitLabel}</span>
    </div>
  </div>

  <label class="picker" for="video">
    <span>Select file</span>
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
      <span>{formatBytes(file.size)}</span>
    {/if}
  </div>

  {#if shareableUrl}
    <div class="share-card">
      <div class="share-copy">
        <strong>Shareable URL</strong>
        <p>{shareableUrl}</p>
      </div>
      <div class="share-actions">
        <button
          class="secondary-button"
          type="button"
          on:click={copyShareableUrl}
        >
          {copiedShareable ? 'Copied' : 'Copy link'}
        </button>
        <a
          class="secondary-link"
          href={shareablePath}
          target="_blank"
          rel="noreferrer"
        >
          Open in new tab
        </a>
      </div>
      <span class="hint">
        {#if completed}
          Another tab can play the raw stream now. Hermes will switch to HLS
          when it is ready.
        {:else if finalizing}
          Upload reached object storage. Hermes is finalizing the raw stream
          now.
        {:else}
          Another tab can use this link as soon as upload finalization
          completes.
        {/if}
      </span>
    </div>
  {/if}

  {#if uploading || finalizing || completed}
    <div class="progress-card" aria-live="polite">
      <div class="progress-copy">
        <strong>
          {#if completed}
            Upload complete
          {:else if finalizing}
            Finalizing raw stream
          {:else}
            Uploading video
          {/if}
        </strong>
        <span>
          {#if completed}
            Ready to switch to {uploadedVideoId || 'the new watch portal'}.
          {:else if finalizing}
            Raw playback is being registered and the transcode job is being
            queued.
          {:else}
            {progressPercent}% of {formatBytes(
              uploadTotalBytes || file?.size || 0
            )}
          {/if}
        </span>
      </div>
      <div class="progress-track">
        <div class="progress-bar" style={`width: ${progressPercent}%`}></div>
      </div>
      {#if uploading}
        <p>
          {formatBytes(progressBytes)} transferred directly to object storage
        </p>
      {/if}
    </div>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="actions">
    <button type="submit" disabled={uploading || finalizing}>
      {#if uploading}
        Uploading...
      {:else if finalizing}
        Finalizing...
      {:else}
        {ctaLabel}
      {/if}
    </button>
  </div>
</form>

<style>
  .upload-form {
    display: grid;
    gap: 1rem;
  }

  .upload-form.compact {
    gap: 0.85rem;
  }

  .section-head {
    display: grid;
    gap: 0.85rem;
  }

  .copy-block {
    min-width: 0;
  }

  .eyebrow {
    margin: 0 0 0.45rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 0.76rem;
    color: var(--sky-700);
    font-weight: 700;
  }

  h2 {
    margin: 0;
    color: var(--ink-900);
    font-size: 1.45rem;
    line-height: 1.05;
  }

  .compact h2 {
    font-size: 1.18rem;
  }

  .copy-block p:last-child {
    margin: 0.55rem 0 0;
    line-height: 1.55;
    color: var(--ink-700);
    font-size: 0.95rem;
  }

  .pill-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
  }

  .format-pill,
  .limit-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 2.1rem;
    padding: 0 0.9rem;
    border-radius: 999px;
    font-size: 0.82rem;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .format-pill {
    background: rgba(85, 173, 255, 0.14);
    color: var(--sky-900);
  }

  .limit-pill {
    background: rgba(14, 76, 146, 0.08);
    color: var(--ink-700);
  }

  .picker {
    display: grid;
    gap: 0.65rem;
    padding: 1rem 1.05rem;
    border-radius: 1rem;
    border: 1px dashed rgba(45, 124, 204, 0.35);
    background: rgba(255, 255, 255, 0.72);
    color: var(--ink-800);
    font-weight: 700;
  }

  .picker input {
    font: inherit;
    color: var(--ink-700);
  }

  .file-row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    min-width: 0;
  }

  .file-row strong,
  .file-row span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .file-row span {
    color: var(--ink-600);
    font-size: 0.9rem;
  }

  .share-card,
  .progress-card {
    display: grid;
    gap: 0.75rem;
    padding: 0.9rem 1rem;
    border-radius: 1rem;
    background: rgba(13, 101, 190, 0.08);
    border: 1px solid rgba(30, 128, 226, 0.18);
  }

  .share-copy strong,
  .progress-copy strong {
    display: block;
    color: var(--ink-900);
  }

  .share-copy p,
  .progress-copy span,
  .progress-card p,
  .hint {
    margin: 0.2rem 0 0;
    color: var(--ink-700);
    line-height: 1.5;
    font-size: 0.9rem;
    overflow-wrap: anywhere;
  }

  .share-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.65rem;
  }

  .secondary-button,
  .secondary-link {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 2.3rem;
    padding: 0 0.95rem;
    border-radius: 999px;
    border: 1px solid rgba(45, 124, 204, 0.22);
    background: rgba(255, 255, 255, 0.86);
    color: var(--sky-900);
    font: inherit;
    font-weight: 700;
    text-decoration: none;
    cursor: pointer;
  }

  .progress-track {
    height: 0.7rem;
    border-radius: 999px;
    background: rgba(18, 88, 160, 0.12);
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #4ab0ff 0%, #0f7ad8 100%);
    transition: width 180ms ease-out;
  }

  .error {
    margin: 0;
    color: #c63f5c;
    font-weight: 700;
  }

  .actions {
    display: flex;
    justify-content: flex-start;
  }

  button[type='submit'] {
    min-height: 2.85rem;
    padding: 0 1.15rem;
    border-radius: 999px;
    border: 0;
    background: linear-gradient(135deg, #1690ff 0%, #0f6bca 100%);
    color: white;
    font: inherit;
    font-weight: 800;
    cursor: pointer;
  }

  button[disabled] {
    opacity: 0.76;
    cursor: progress;
  }
</style>
