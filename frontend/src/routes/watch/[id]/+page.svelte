<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { apiUrl } from '$lib/api';
  import type Hls from 'hls.js';

  type VideoStatus = 'pending' | 'processing' | 'ready' | 'failed';
  type LiveMode = 'sse' | 'polling' | 'offline';
  type PlaybackMode = 'raw' | 'hls';

  type VideoStatusPayload = {
    id: string;
    status: VideoStatus;
    raw_stream_url: string;
    hls_playlist_url: string | null;
    error_msg: string | null;
  };

  const POLL_INTERVAL_MS = 3000;

  let id = '';
  let status: VideoStatusPayload | null = null;
  let loading = true;
  let error = '';
  let liveMode: LiveMode = 'offline';
  let playbackMode: PlaybackMode = 'raw';
  let playbackMessage = 'Starting on the raw source stream.';
  let switchingToHls = false;

  let videoEl: HTMLVideoElement | null = null;
  let eventSource: EventSource | null = null;
  let pollHandle: ReturnType<typeof setInterval> | null = null;
  let hls: Hls | null = null;
  let activeHlsUrl: string | null = null;

  $: id = $page.params.id ?? '';
  $: rawStreamUrl =
    status?.raw_stream_url ?? apiUrl(`/api/videos/${id}/stream`);
  $: hlsPlaylistUrl =
    status?.hls_playlist_url ?? apiUrl(`/api/videos/${id}/hls/index.m3u8`);
  $: if (status?.status === 'ready' && status.hls_playlist_url) {
    void upgradeToHls(status.hls_playlist_url);
  }

  onMount(() => {
    let cancelled = false;

    async function bootstrap() {
      await refreshStatus();
      if (cancelled) {
        return;
      }

      if (typeof EventSource === 'undefined') {
        startPolling();
        return;
      }

      connectEvents();
    }

    bootstrap();

    return () => {
      cancelled = true;
      stopEvents();
      stopPolling();
      destroyHls();
    };
  });

  async function refreshStatus() {
    loading = !status;

    try {
      const response = await fetch(apiUrl(`/api/videos/${id}/status`));
      const payload = (await response.json()) as
        | VideoStatusPayload
        | { error?: string };

      if (!response.ok) {
        error =
          'error' in payload
            ? (payload.error ?? 'Failed to load video status.')
            : 'Failed to load video status.';
        return;
      }

      status = payload as VideoStatusPayload;
      error = '';
    } catch (requestError) {
      error =
        requestError instanceof Error
          ? requestError.message
          : 'Failed to load video status.';
    } finally {
      loading = false;
    }
  }

  function connectEvents() {
    stopEvents();

    const source = new EventSource(apiUrl(`/api/videos/${id}/events`));
    eventSource = source;
    liveMode = 'sse';

    source.addEventListener('status', (event) => {
      const message = event as MessageEvent<string>;
      status = JSON.parse(message.data) as VideoStatusPayload;
      error = '';
    });

    source.onerror = () => {
      stopEvents();
      startPolling();
    };
  }

  function startPolling() {
    if (pollHandle) {
      return;
    }

    liveMode = 'polling';
    pollHandle = setInterval(() => {
      void refreshStatus();
    }, POLL_INTERVAL_MS);
  }

  function stopEvents() {
    if (!eventSource) {
      return;
    }

    eventSource.close();
    eventSource = null;
  }

  function stopPolling() {
    if (!pollHandle) {
      return;
    }

    clearInterval(pollHandle);
    pollHandle = null;
  }

  async function upgradeToHls(playlistUrl: string) {
    if (!videoEl || switchingToHls) {
      return;
    }

    if (playbackMode === 'hls' && activeHlsUrl === playlistUrl) {
      return;
    }

    const resumeTime = Number.isFinite(videoEl.currentTime)
      ? videoEl.currentTime
      : 0;
    const shouldResume = !videoEl.paused && !videoEl.ended;
    switchingToHls = true;
    playbackMessage = 'HLS is ready. Switching the player...';

    try {
      if (supportsNativeHls(videoEl)) {
        destroyHls();
        videoEl.src = playlistUrl;
        videoEl.load();
        await waitForLoadedMetadata(videoEl);
      } else {
        const hlsModule = await import('hls.js');
        const HlsCtor = hlsModule.default;
        if (!HlsCtor.isSupported()) {
          throw new Error(
            'This browser cannot play HLS; staying on the raw stream.'
          );
        }

        destroyHls();
        await attachHlsWithLibrary(HlsCtor, playlistUrl);
      }

      restorePlaybackPosition(resumeTime, shouldResume);
      playbackMode = 'hls';
      activeHlsUrl = playlistUrl;
      playbackMessage = 'Switched to adaptive HLS playback.';
      error = '';
    } catch (switchError) {
      destroyHls();
      activeHlsUrl = null;
      playbackMode = 'raw';
      await restoreRawPlayback(resumeTime, shouldResume);
      playbackMessage =
        'HLS is ready, but this browser stayed on the raw stream.';
      if (switchError instanceof Error) {
        error = switchError.message;
      }
    } finally {
      switchingToHls = false;
    }
  }

  async function attachHlsWithLibrary(
    HlsCtor: typeof import('hls.js').default,
    playlistUrl: string
  ) {
    if (!videoEl) {
      return;
    }
    const target = videoEl;

    const instance = new HlsCtor();
    hls = instance;

    await new Promise<void>((resolve, reject) => {
      const onMediaAttached = () => {
        instance.off(HlsCtor.Events.MEDIA_ATTACHED, onMediaAttached);
        instance.loadSource(playlistUrl);
      };

      const onManifestParsed = () => {
        cleanup();
        resolve();
      };

      const onError = (
        _event: string,
        data: { fatal: boolean; details: string }
      ) => {
        if (!data.fatal) {
          return;
        }

        cleanup();
        reject(new Error(`HLS playback failed: ${data.details}`));
      };

      const cleanup = () => {
        instance.off(HlsCtor.Events.MEDIA_ATTACHED, onMediaAttached);
        instance.off(HlsCtor.Events.MANIFEST_PARSED, onManifestParsed);
        instance.off(HlsCtor.Events.ERROR, onError);
      };

      instance.on(HlsCtor.Events.MEDIA_ATTACHED, onMediaAttached);
      instance.on(HlsCtor.Events.MANIFEST_PARSED, onManifestParsed);
      instance.on(HlsCtor.Events.ERROR, onError);
      instance.attachMedia(target);
    });

    await waitForLoadedMetadata(target);
  }

  async function restoreRawPlayback(resumeTime: number, shouldResume: boolean) {
    if (!videoEl) {
      return;
    }

    videoEl.src = rawStreamUrl;
    videoEl.load();
    await waitForLoadedMetadata(videoEl);
    restorePlaybackPosition(resumeTime, shouldResume);
  }

  async function waitForLoadedMetadata(target: HTMLVideoElement) {
    if (target.readyState >= 1) {
      return;
    }

    await new Promise<void>((resolve) => {
      target.addEventListener('loadedmetadata', () => resolve(), {
        once: true
      });
    });
  }

  function restorePlaybackPosition(resumeTime: number, shouldResume: boolean) {
    if (!videoEl) {
      return;
    }

    if (resumeTime > 0) {
      try {
        videoEl.currentTime = resumeTime;
      } catch {
        // Ignore browsers that reject seeks before the timeline is ready.
      }
    }

    if (shouldResume) {
      void videoEl.play().catch(() => {});
    }
  }

  function destroyHls() {
    if (!hls) {
      return;
    }

    hls.destroy();
    hls = null;
  }

  function supportsNativeHls(target: HTMLVideoElement) {
    return (
      target.canPlayType('application/vnd.apple.mpegurl') !== '' ||
      target.canPlayType('application/x-mpegURL') !== ''
    );
  }

  function statusLabel(value: VideoStatus | undefined): string {
    switch (value) {
      case 'pending':
        return 'Queued';
      case 'processing':
        return 'Transcoding';
      case 'ready':
        return 'HLS Ready';
      case 'failed':
        return 'Failed';
      default:
        return 'Loading';
    }
  }

  function liveModeLabel(value: LiveMode): string {
    switch (value) {
      case 'sse':
        return 'Live updates';
      case 'polling':
        return 'Polling fallback';
      default:
        return 'Connecting';
    }
  }

  function playbackLabel(value: PlaybackMode): string {
    return value === 'hls' ? 'Adaptive HLS' : 'Raw playback';
  }
</script>

<svelte:head>
  <title>Hermes | Watch</title>
</svelte:head>

<section class="watch-shell">
  <div class="watch-card">
    <div class="card-top">
      <div>
        <p class="eyebrow">Phase 4 Checkpoint</p>
        <h1>{id}</h1>
      </div>
      <div class="status-stack">
        <span
          class:ready={status?.status === 'ready'}
          class:failed={status?.status === 'failed'}
          class="status-pill"
        >
          {statusLabel(status?.status)}
        </span>
        <span class="live-pill">{liveModeLabel(liveMode)}</span>
      </div>
    </div>

    <p class="lede">
      Playback starts from the raw source immediately, then upgrades to HLS as
      soon as the worker publishes a ready state.
    </p>

    <video
      bind:this={videoEl}
      controls
      playsinline
      preload="metadata"
      src={rawStreamUrl}
    >
      <track kind="captions" />
    </video>

    <div class="details">
      <div class="detail-card">
        <span class="detail-label">Current mode</span>
        <strong>{playbackLabel(playbackMode)}</strong>
        <p>{playbackMessage}</p>
      </div>

      <div class="detail-card">
        <span class="detail-label">Background state</span>
        <strong>{statusLabel(status?.status)}</strong>
        {#if loading}
          <p>Loading current job state...</p>
        {:else if status?.status === 'ready'}
          <p>
            Transcoding is complete. The player should now be on the HLS
            playlist when the browser supports it.
          </p>
        {:else if status?.status === 'failed'}
          <p>
            The worker reported a failed transcode. Check the message below for
            details.
          </p>
        {:else}
          <p>
            The worker is still processing. The page keeps following updates and
            will switch when HLS is ready.
          </p>
        {/if}
      </div>
    </div>

    {#if status?.error_msg}
      <p class="error">{status.error_msg}</p>
    {/if}

    {#if error}
      <p class="error">{error}</p>
    {/if}

    <div class="links">
      <a href={rawStreamUrl} target="_blank" rel="noreferrer">Open raw stream</a
      >
      <a
        href={apiUrl(`/api/videos/${id}/status`)}
        target="_blank"
        rel="noreferrer">Open JSON status</a
      >
      <a
        href={apiUrl(`/api/videos/${id}/events`)}
        target="_blank"
        rel="noreferrer">Open SSE stream</a
      >
      {#if status?.hls_playlist_url}
        <a href={hlsPlaylistUrl} target="_blank" rel="noreferrer"
          >Open HLS playlist</a
        >
      {/if}
    </div>
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
    width: min(60rem, 100%);
    padding: 2rem;
    border-radius: 1.5rem;
    background: rgba(255, 252, 246, 0.84);
    border: 1px solid rgba(92, 71, 44, 0.18);
    box-shadow: 0 16px 40px rgba(53, 38, 20, 0.12);
  }

  .card-top {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: flex-start;
  }

  .status-stack {
    display: grid;
    gap: 0.65rem;
    justify-items: end;
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

  .lede {
    line-height: 1.65;
    max-width: 48rem;
  }

  .status-pill,
  .live-pill {
    display: inline-flex;
    align-items: center;
    border-radius: 999px;
    padding: 0.45rem 0.8rem;
    font-size: 0.85rem;
    font-weight: 700;
    background: rgba(244, 205, 150, 0.45);
    color: #553712;
  }

  .status-pill.ready {
    background: rgba(107, 172, 112, 0.18);
    color: #24502b;
  }

  .status-pill.failed {
    background: rgba(184, 63, 63, 0.14);
    color: #8d1e1e;
  }

  video {
    display: block;
    width: 100%;
    margin-top: 1.25rem;
    border-radius: 1rem;
    background: #120f0c;
    aspect-ratio: 16 / 9;
  }

  .details {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
    margin-top: 1.25rem;
  }

  .detail-card {
    padding: 1rem;
    border-radius: 1rem;
    background: rgba(255, 247, 235, 0.82);
    border: 1px solid rgba(92, 71, 44, 0.14);
  }

  .detail-card strong {
    display: block;
    margin-top: 0.35rem;
    font-size: 1.05rem;
  }

  .detail-card p {
    margin: 0.55rem 0 0;
    line-height: 1.55;
  }

  .detail-label {
    font-size: 0.78rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #7c6548;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem 1rem;
    margin-top: 1.25rem;
  }

  .links a {
    color: #201910;
    text-decoration: none;
    font-weight: 600;
  }

  .error {
    margin: 1rem 0 0;
    color: #9e2b25;
    font-weight: 600;
    line-height: 1.55;
  }

  @media (max-width: 720px) {
    .card-top {
      flex-direction: column;
    }

    .status-stack {
      justify-items: start;
    }

    .details {
      grid-template-columns: 1fr;
    }
  }
</style>
