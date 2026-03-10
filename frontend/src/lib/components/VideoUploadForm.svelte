<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { apiUrl } from '$lib/api';

  type UploadVideoResponse = {
    id: string;
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

  const uploadUrl = apiUrl('/api/videos');

  let file: File | null = null;
  let error = '';
  let submitting = false;
  let completed = false;
  let uploadedVideoId = '';
  let progressPercent = 0;
  let progressBytes = 0;
  let uploadTotalBytes = 0;

  async function submitUpload(event: SubmitEvent) {
    event.preventDefault();
    error = '';
    completed = false;
    uploadedVideoId = '';
    progressPercent = 0;
    progressBytes = 0;
    uploadTotalBytes = file?.size ?? 0;

    if (!file) {
      error = 'Choose a video file first.';
      return;
    }

    const formData = new FormData();
    formData.append('video', file);
    submitting = true;

    try {
      const payload = await uploadVideo(formData);
      uploadedVideoId = payload.id;
      progressBytes = uploadTotalBytes;
      progressPercent = 100;
      completed = true;
      dispatch('uploaded', payload);
    } catch (requestError) {
      error =
        requestError instanceof Error
          ? requestError.message
          : 'Upload request failed.';
    } finally {
      submitting = false;
    }
  }

  function uploadVideo(formData: FormData): Promise<UploadVideoResponse> {
    return new Promise((resolve, reject) => {
      const request = new XMLHttpRequest();
      request.open('POST', uploadUrl);
      request.responseType = 'json';

      request.upload.onprogress = (event) => {
        if (!event.lengthComputable) {
          return;
        }

        progressBytes = event.loaded;
        uploadTotalBytes = event.total;
        progressPercent = Math.round((event.loaded / event.total) * 100);
      };

      request.onerror = () => {
        reject(new Error('Upload request failed.'));
      };

      request.onload = () => {
        const payload =
          (request.response as
            | UploadVideoResponse
            | { error?: string }
            | null) ?? safeParseJson(request.responseText);

        if (request.status < 200 || request.status >= 300) {
          const message =
            payload && typeof payload === 'object' && 'error' in payload
              ? typeof payload.error === 'string'
                ? payload.error
                : null
              : null;
          reject(new Error(message ?? 'Upload failed.'));
          return;
        }

        resolve(payload as UploadVideoResponse);
      };

      request.send(formData);
    });
  }

  function safeParseJson(raw: string): unknown {
    try {
      return JSON.parse(raw);
    } catch {
      return null;
    }
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

  {#if submitting || completed}
    <div class="progress-card" aria-live="polite">
      <div class="progress-copy">
        <strong>{completed ? 'Upload complete' : 'Uploading video'}</strong>
        <span>
          {#if completed}
            Ready to switch to {uploadedVideoId || 'the new watch portal'}.
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
      {#if !completed}
        <p>{formatBytes(progressBytes)} transferred</p>
      {/if}
    </div>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="actions">
    <button type="submit" disabled={submitting}>
      {#if submitting}
        Uploading...
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
    font-size: 0.79rem;
    font-weight: 700;
  }

  .format-pill {
    background: rgba(108, 176, 228, 0.14);
    color: var(--sky-900);
  }

  .limit-pill {
    background: rgba(255, 255, 255, 0.7);
    color: var(--ink-700);
    border: 1px solid var(--border-soft);
  }

  .picker {
    display: grid;
    gap: 0.7rem;
    padding: 1rem;
    border-radius: 1.25rem;
    background: rgba(255, 255, 255, 0.58);
    border: 1px dashed rgba(44, 102, 154, 0.35);
    font-weight: 700;
    color: var(--sky-900);
  }

  input[type='file'] {
    font: inherit;
    color: var(--ink-700);
  }

  .file-row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    font-size: 0.95rem;
    color: var(--ink-900);
  }

  .file-row strong {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .file-row span {
    color: var(--ink-500);
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem;
  }

  button {
    font: inherit;
    min-height: 3rem;
    padding: 0 1.25rem;
    border-radius: 999px;
    font-weight: 700;
    border: 0;
    cursor: pointer;
    color: white;
    background: linear-gradient(135deg, var(--sky-700), var(--sky-500));
    box-shadow: 0 14px 28px rgba(47, 112, 162, 0.24);
  }

  button:disabled {
    opacity: 0.72;
    cursor: progress;
  }

  .progress-card {
    display: grid;
    gap: 0.75rem;
    padding: 1rem;
    border-radius: 1.1rem;
    background: rgba(255, 255, 255, 0.62);
    border: 1px solid var(--border-soft);
  }

  .progress-copy {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    flex-wrap: wrap;
  }

  .progress-copy span,
  .progress-card p {
    color: var(--ink-500);
    margin: 0;
    font-size: 0.95rem;
  }

  .progress-track {
    height: 0.7rem;
    border-radius: 999px;
    overflow: hidden;
    background: rgba(44, 102, 154, 0.14);
  }

  .progress-bar {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--sky-600), var(--sky-500));
    transition: width 0.2s ease;
  }

  .error {
    margin: 0;
    color: var(--danger-700);
    font-weight: 700;
  }

  @media (max-width: 640px) {
    .file-row {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
