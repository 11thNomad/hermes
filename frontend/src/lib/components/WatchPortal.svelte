<script lang="ts">
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { apiUrl } from '$lib/api';
  import DevDiagnosticsFooter from '$lib/components/DevDiagnosticsFooter.svelte';
  import VideoUploadForm from '$lib/components/VideoUploadForm.svelte';
  import { onMount } from 'svelte';
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

  type VideoListItem = {
    id: string;
    filename: string;
    status: VideoStatus;
    size_bytes: number;
  };

  const POLL_INTERVAL_MS = 3000;
  const AUTOPLAY_STORAGE_KEY = 'hermes-autoplay-video-id';

  export let routeVideoId = '';

  let id = '';
  let activeVideoId = '';
  let activationId = 0;
  let routeInitialized = false;

  let status: VideoStatusPayload | null = null;
  let recentVideos: VideoListItem[] = [];
  let loading = true;
  let error = '';
  let libraryError = '';
  let liveMode: LiveMode = 'offline';
  let playbackMode: PlaybackMode = 'raw';
  let playbackMessage =
    'Choose or upload a video to start watching in this portal.';
  let switchingToHls = false;
  let sidebarOpen = true;
  let autoplayRequested = false;

  let videoEl: HTMLVideoElement | null = null;
  let eventSource: EventSource | null = null;
  let pollHandle: ReturnType<typeof setInterval> | null = null;
  let hls: Hls | null = null;
  let activeHlsUrl: string | null = null;

  $: id = routeVideoId;
  $: rawStreamUrl =
    status?.raw_stream_url ?? (id ? apiUrl(`/api/videos/${id}/stream`) : '');
  $: hlsPlaylistUrl =
    status?.hls_playlist_url ??
    (id ? apiUrl(`/api/videos/${id}/hls/index.m3u8`) : '');
  $: if (browser && (!routeInitialized || id !== activeVideoId)) {
    routeInitialized = true;
    void activatePortal(id);
  }
  $: if (status?.status === 'ready' && status.hls_playlist_url) {
    void upgradeToHls(status.hls_playlist_url, id);
  }

  onMount(() => {
    return () => {
      stopEvents();
      stopPolling();
      destroyHls();
    };
  });

  async function activatePortal(nextId: string) {
    const nextActivationId = ++activationId;

    if (!nextId) {
      activeVideoId = '';
      resetSession();
      loading = false;
      playbackMessage =
        'Upload from the sidebar or open a recent video to start playback here.';
      await refreshRecentVideos();
      return;
    }

    activeVideoId = nextId;
    autoplayRequested = consumeAutoplayIntent(nextId);
    resetSession();
    if (autoplayRequested) {
      playbackMessage =
        'Upload complete. Hermes will try to start playback automatically.';
    }

    await Promise.all([
      refreshStatus(nextId, nextActivationId),
      refreshRecentVideos()
    ]);

    if (nextActivationId !== activationId) {
      return;
    }

    if (typeof EventSource === 'undefined') {
      startPolling(nextId, nextActivationId);
      return;
    }

    connectEvents(nextId, nextActivationId);
  }

  function resetSession() {
    stopEvents();
    stopPolling();
    destroyHls();
    activeHlsUrl = null;
    switchingToHls = false;
    status = null;
    error = '';
    loading = true;
    liveMode = 'offline';
    playbackMode = 'raw';
    if (!autoplayRequested) {
      playbackMessage = 'Starting on the raw source stream.';
    }
  }

  async function refreshStatus(
    targetId: string = id,
    requestActivationId: number = activationId
  ) {
    if (!targetId) {
      return;
    }

    loading = !status;

    try {
      const response = await fetch(apiUrl(`/api/videos/${targetId}/status`));
      const payload = (await response.json()) as
        | VideoStatusPayload
        | { error?: string };

      if (requestActivationId !== activationId || targetId !== activeVideoId) {
        return;
      }

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
      if (requestActivationId !== activationId || targetId !== activeVideoId) {
        return;
      }

      error =
        requestError instanceof Error
          ? requestError.message
          : 'Failed to load video status.';
    } finally {
      if (requestActivationId === activationId && targetId === activeVideoId) {
        loading = false;
      }
    }
  }

  async function refreshRecentVideos() {
    try {
      const response = await fetch(apiUrl('/api/videos'));
      const payload = (await response.json()) as
        | VideoListItem[]
        | { error?: string };

      if (!response.ok) {
        libraryError =
          !Array.isArray(payload) && typeof payload?.error === 'string'
            ? payload.error
            : 'Failed to load recent uploads.';
        return;
      }

      recentVideos = payload as VideoListItem[];
      libraryError = '';
    } catch (requestError) {
      libraryError =
        requestError instanceof Error
          ? requestError.message
          : 'Failed to load recent uploads.';
    }
  }

  function connectEvents(targetId: string, requestActivationId: number) {
    stopEvents();

    const source = new EventSource(apiUrl(`/api/videos/${targetId}/events`));
    eventSource = source;
    liveMode = 'sse';

    source.addEventListener('status', (event) => {
      if (requestActivationId !== activationId || targetId !== activeVideoId) {
        return;
      }

      const message = event as MessageEvent<string>;
      status = JSON.parse(message.data) as VideoStatusPayload;
      error = '';
      loading = false;
      void refreshRecentVideos();
    });

    source.onerror = () => {
      if (requestActivationId !== activationId || targetId !== activeVideoId) {
        return;
      }

      stopEvents();
      startPolling(targetId, requestActivationId);
    };
  }

  function startPolling(targetId: string, requestActivationId: number) {
    stopPolling();
    liveMode = 'polling';

    pollHandle = setInterval(() => {
      if (requestActivationId !== activationId || targetId !== activeVideoId) {
        stopPolling();
        return;
      }

      void refreshStatus(targetId, requestActivationId);
      void refreshRecentVideos();
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

  async function upgradeToHls(playlistUrl: string, targetId: string) {
    if (!videoEl || switchingToHls || targetId !== activeVideoId) {
      return;
    }

    if (playbackMode === 'hls' && activeHlsUrl === playlistUrl) {
      return;
    }

    const resumeTime = Number.isFinite(videoEl.currentTime)
      ? videoEl.currentTime
      : 0;
    const shouldResume =
      autoplayRequested || (!videoEl.paused && !videoEl.ended);
    switchingToHls = true;
    playbackMessage = `HLS is ready. Seeking back to ${formatTime(resumeTime)} after the switch.`;

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
            'This browser cannot play HLS, so Hermes stayed on the raw source stream.'
          );
        }

        destroyHls();
        await attachHlsWithLibrary(HlsCtor, playlistUrl);
      }

      if (targetId !== activeVideoId) {
        return;
      }

      restorePlaybackPosition(resumeTime, shouldResume);
      autoplayRequested = false;
      playbackMode = 'hls';
      activeHlsUrl = playlistUrl;
      playbackMessage = `Switched to adaptive HLS playback and restored the position near ${formatTime(resumeTime)}.`;
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
    if (!videoEl || !rawStreamUrl) {
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
      void videoEl.play().catch(() => {
        playbackMessage =
          'The browser blocked autoplay. Press play to start playback.';
      });
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

  async function handleSidebarUploaded(
    event: CustomEvent<{ id: string; shareable_url: string }>
  ) {
    rememberAutoplayIntent(event.detail.id);
    playbackMessage =
      'Upload complete. Switching the watch portal to the new video.';
    await new Promise((resolve) => setTimeout(resolve, 350));
    await goto(event.detail.shareable_url, {
      keepFocus: true,
      noScroll: true
    });
  }

  function handleRawLoadedMetadata() {
    if (!autoplayRequested || !videoEl) {
      return;
    }

    autoplayRequested = false;
    void videoEl
      .play()
      .then(() => {
        playbackMessage = 'Autoplay started on the raw source stream.';
      })
      .catch(() => {
        playbackMessage =
          'The browser blocked autoplay. Press play to start playback.';
      });
  }

  function rememberAutoplayIntent(videoId: string) {
    if (!browser) {
      return;
    }

    sessionStorage.setItem(AUTOPLAY_STORAGE_KEY, videoId);
  }

  function consumeAutoplayIntent(videoId: string): boolean {
    if (!browser) {
      return false;
    }

    const queuedVideoId = sessionStorage.getItem(AUTOPLAY_STORAGE_KEY);
    if (queuedVideoId !== videoId) {
      return false;
    }

    sessionStorage.removeItem(AUTOPLAY_STORAGE_KEY);
    return true;
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
        return 'Awaiting video';
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

  function indicatorTone(
    value: VideoStatus | undefined
  ): 'ok' | 'warn' | 'error' | 'loading' {
    switch (value) {
      case 'ready':
        return 'ok';
      case 'pending':
      case 'processing':
        return 'warn';
      case 'failed':
        return 'error';
      default:
        return 'loading';
    }
  }

  function formatBytes(value: number): string {
    if (value <= 0) {
      return '0 MB';
    }

    return `${(value / 1024 / 1024).toFixed(value >= 100 * 1024 * 1024 ? 0 : 1)} MB`;
  }

  function formatTime(value: number): string {
    const safeValue = Number.isFinite(value)
      ? Math.max(0, Math.floor(value))
      : 0;
    const minutes = Math.floor(safeValue / 60);
    const seconds = safeValue % 60;
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  }
</script>

<section class="portal-shell">
  <aside class:collapsed={!sidebarOpen} class="library panel">
    <div class="library-head">
      <div class="library-copy">
        <p class="eyebrow">Library</p>
        <h2>Uploads and switching</h2>
        {#if sidebarOpen}
          <p>
            Queue another file while the current stream keeps playing. The watch
            portal switches only after upload completes.
          </p>
        {/if}
      </div>
      <button
        class="collapse-toggle"
        type="button"
        aria-label={sidebarOpen ? 'Collapse sidebar' : 'Expand sidebar'}
        on:click={() => {
          sidebarOpen = !sidebarOpen;
        }}
      >
        {sidebarOpen ? '<' : '>'}
      </button>
    </div>

    {#if sidebarOpen}
      <div class="sidebar-uploader panel inset-panel">
        <VideoUploadForm
          compact={true}
          title="Upload from the portal"
          description="Hermes uploads in the sidebar, keeps the current stream alive, then moves the portal to the new video when the upload finishes."
          ctaLabel="Upload and switch"
          on:uploaded={handleSidebarUploaded}
        />
      </div>

      {#if libraryError}
        <p class="error">{libraryError}</p>
      {/if}

      <div class="library-list">
        {#if recentVideos.length === 0}
          <div class="empty-state">
            <strong>No videos yet</strong>
            <p>
              Your recent uploads appear here after the first successful upload.
            </p>
          </div>
        {:else}
          {#each recentVideos as video}
            <a
              class:active={video.id === id}
              class="video-link"
              href={`/watch/${video.id}`}
            >
              <div class="video-link-top">
                <strong>{video.filename}</strong>
                <span
                  class:ready={video.status === 'ready'}
                  class:failed={video.status === 'failed'}
                  class="status-pill"
                >
                  {statusLabel(video.status)}
                </span>
              </div>
              <span class="video-meta">{formatBytes(video.size_bytes)}</span>
            </a>
          {/each}
        {/if}
      </div>
    {:else}
      <div class="collapsed-summary">
        <span class="summary-count">{recentVideos.length}</span>
        <span>videos</span>
      </div>
    {/if}
  </aside>

  <div class="viewer-column">
    <div class="topbar panel">
      <div class="viewer-copy">
        <p class="eyebrow">Viewing portal</p>
        <h1>{id || 'No active video selected'}</h1>
        <p>
          {#if id}
            Raw playback starts immediately in this portal. When HLS is ready,
            Hermes switches the player source in place and seeks back near the
            last watch time.
          {:else}
            This is now the primary full-page portal. Upload from the sidebar or
            pick a recent item to start watching here.
          {/if}
        </p>
      </div>

      <div class="topbar-actions">
        <div
          class:ok={indicatorTone(status?.status) === 'ok'}
          class:warn={indicatorTone(status?.status) === 'warn'}
          class:error={indicatorTone(status?.status) === 'error'}
          class:loading={indicatorTone(status?.status) === 'loading'}
          class="signal-pill"
        >
          <span class="signal-dot"></span>
          <span>{statusLabel(status?.status)}</span>
        </div>
        <button
          class="mobile-toggle"
          type="button"
          on:click={() => {
            sidebarOpen = !sidebarOpen;
          }}
        >
          {sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
        </button>
      </div>
    </div>

    <div class="player-card panel">
      <div class="card-head">
        <div>
          <p class="eyebrow">Player</p>
          <h2>Watch here, not on the raw endpoint</h2>
        </div>
        <div class="api-note">
          <strong>Raw `/stream`</strong>
          <span>API media response only</span>
        </div>
      </div>

      {#if id}
        {#key id}
          <video
            bind:this={videoEl}
            controls
            playsinline
            preload="metadata"
            src={rawStreamUrl}
            on:loadedmetadata={handleRawLoadedMetadata}
          >
            <track kind="captions" />
          </video>
        {/key}
      {:else}
        <div class="video-placeholder">
          <strong>Portal ready</strong>
          <p>
            The sidebar uploader now replaces the old landing page. Upload a
            file or pick a recent item to activate the player.
          </p>
        </div>
      {/if}

      <div class="details">
        <div class="detail-card">
          <span class="detail-label">Playback mode</span>
          <strong>{playbackLabel(playbackMode)}</strong>
          <p>{playbackMessage}</p>
        </div>

        <div class="detail-card">
          <span class="detail-label">Job status</span>
          <strong>{statusLabel(status?.status)}</strong>
          {#if !id}
            <p>
              No active video is loaded yet. Upload or select one from the
              sidebar.
            </p>
          {:else if loading}
            <p>Loading the latest job state for this video.</p>
          {:else if status?.status === 'ready'}
            <p>
              HLS is available. Hermes keeps the portal in place and seeks close
              to the last watch time during the switch.
            </p>
          {:else if status?.status === 'failed'}
            <p>
              Raw playback still works here, but adaptive HLS is unavailable for
              this upload.
            </p>
          {:else}
            <p>
              The amber indicator means processing is still active. The player
              stays on the raw source until HLS is ready.
            </p>
          {/if}
        </div>

        <div class="detail-card">
          <span class="detail-label">Live updates</span>
          <strong>{liveModeLabel(liveMode)}</strong>
          <p>
            Server-sent events drive the switch. Polling takes over if the live
            stream drops.
          </p>
        </div>
      </div>

      {#if status?.status === 'failed'}
        <div class="failure-card" role="alert">
          <span class="detail-label">Transcode failed</span>
          <strong
            >Hermes kept the raw upload, but the adaptive stream could not be
            prepared.</strong
          >
          <p>
            Stay in this portal to watch the raw source, or upload a replacement
            from the sidebar without leaving the player.
          </p>
        </div>
      {/if}

      {#if status?.error_msg}
        <p class="error">{status.error_msg}</p>
      {/if}

      {#if error}
        <p class="error">{error}</p>
      {/if}

      {#if id}
        <div class="links">
          <a
            class="secondary-link"
            href={apiUrl(`/api/videos/${id}/status`)}
            target="_blank"
            rel="noreferrer"
          >
            JSON status
          </a>
          <a
            class="secondary-link"
            href={apiUrl(`/api/videos/${id}/events`)}
            target="_blank"
            rel="noreferrer"
          >
            SSE stream
          </a>
          {#if status?.hls_playlist_url}
            <a
              class="secondary-link"
              href={hlsPlaylistUrl}
              target="_blank"
              rel="noreferrer"
            >
              HLS playlist
            </a>
          {/if}
          <a
            class="secondary-link subtle"
            href={rawStreamUrl}
            target="_blank"
            rel="noreferrer"
          >
            Raw `/stream` API URL
          </a>
        </div>
      {/if}
    </div>

    <DevDiagnosticsFooter
      videoId={id || null}
      {liveMode}
      {playbackMode}
      recentVideoCount={recentVideos.length}
    />
  </div>
</section>

<style>
  .portal-shell {
    min-height: 100vh;
    padding: 1.25rem;
    display: grid;
    grid-template-columns: 24rem minmax(0, 1fr);
    gap: 1.25rem;
  }

  .panel {
    border-radius: 1.6rem;
    background: var(--cloud-200);
    border: 1px solid var(--border-soft);
    box-shadow: var(--shadow-soft);
    backdrop-filter: blur(16px);
  }

  .inset-panel {
    padding: 1rem;
    background: rgba(255, 255, 255, 0.44);
  }

  .library,
  .topbar,
  .player-card {
    padding: 1.35rem;
  }

  .library {
    display: grid;
    align-content: start;
    gap: 1rem;
    position: sticky;
    top: 1.25rem;
    max-height: calc(100vh - 2.5rem);
    overflow: auto;
  }

  .library.collapsed {
    width: 5.5rem;
    overflow: hidden;
    justify-items: center;
  }

  .viewer-column {
    display: grid;
    align-content: start;
    gap: 1rem;
    min-width: 0;
  }

  .topbar {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .viewer-copy {
    min-width: 0;
  }

  .viewer-copy p:last-child {
    margin: 0.65rem 0 0;
    color: var(--ink-700);
    line-height: 1.6;
    max-width: 42rem;
  }

  .topbar-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    align-items: center;
    justify-content: flex-end;
  }

  .library-head {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: flex-start;
  }

  .library-copy {
    min-width: 0;
  }

  .library-copy p:last-child {
    margin: 0.55rem 0 0;
    line-height: 1.55;
    color: var(--ink-700);
  }

  .eyebrow {
    margin: 0 0 0.55rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 0.78rem;
    color: var(--sky-700);
    font-weight: 700;
  }

  h1,
  h2 {
    margin: 0;
    color: var(--ink-900);
    overflow-wrap: anywhere;
  }

  h1 {
    font-size: clamp(1.35rem, 2.8vw, 2.2rem);
  }

  h2 {
    font-size: clamp(1.2rem, 2vw, 1.7rem);
  }

  .collapse-toggle,
  .mobile-toggle {
    font: inherit;
    min-height: 2.55rem;
    min-width: 2.55rem;
    padding: 0 0.9rem;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: rgba(255, 255, 255, 0.7);
    color: var(--sky-900);
    font-weight: 700;
    cursor: pointer;
  }

  .library-list {
    display: grid;
    gap: 0.7rem;
  }

  .empty-state {
    padding: 1rem;
    border-radius: 1.1rem;
    background: rgba(255, 255, 255, 0.66);
    border: 1px solid var(--border-soft);
  }

  .empty-state p {
    margin: 0.45rem 0 0;
    color: var(--ink-700);
    line-height: 1.5;
  }

  .video-link {
    display: grid;
    gap: 0.45rem;
    padding: 0.95rem 1rem;
    border-radius: 1.15rem;
    text-decoration: none;
    border: 1px solid transparent;
    background: rgba(255, 255, 255, 0.52);
    transition:
      transform 0.16s ease,
      border-color 0.16s ease,
      background 0.16s ease;
  }

  .video-link:hover,
  .video-link.active {
    transform: translateY(-1px);
    border-color: var(--border-strong);
    background: rgba(255, 255, 255, 0.82);
  }

  .video-link-top {
    display: flex;
    justify-content: space-between;
    gap: 0.7rem;
    align-items: flex-start;
  }

  .video-link-top strong {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .video-meta {
    color: var(--ink-500);
    font-size: 0.88rem;
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    white-space: nowrap;
    min-height: 1.9rem;
    padding: 0 0.7rem;
    border-radius: 999px;
    background: var(--warning-100);
    color: var(--warning-700);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .status-pill.ready {
    background: var(--success-100);
    color: var(--success-700);
  }

  .status-pill.failed {
    background: var(--danger-100);
    color: var(--danger-700);
  }

  .collapsed-summary {
    display: grid;
    justify-items: center;
    gap: 0.4rem;
    padding: 1rem 0;
    color: var(--ink-700);
    font-weight: 700;
  }

  .summary-count {
    display: grid;
    place-items: center;
    width: 2.7rem;
    height: 2.7rem;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.76);
    color: var(--sky-900);
    font-size: 1rem;
  }

  .signal-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    min-height: 2.45rem;
    padding: 0 0.95rem;
    border-radius: 999px;
    font-weight: 700;
    background: rgba(255, 255, 255, 0.72);
    color: var(--ink-700);
    border: 1px solid var(--border-soft);
  }

  .signal-pill.warn {
    color: var(--warning-700);
    background: var(--warning-100);
  }

  .signal-pill.ok {
    color: var(--success-700);
    background: var(--success-100);
  }

  .signal-pill.error {
    color: var(--danger-700);
    background: var(--danger-100);
  }

  .signal-dot {
    width: 0.78rem;
    height: 0.78rem;
    border-radius: 999px;
    background: currentColor;
    box-shadow: 0 0 0 0.22rem rgba(255, 255, 255, 0.6);
  }

  .player-card {
    display: grid;
    gap: 1rem;
    min-width: 0;
  }

  .card-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: flex-start;
  }

  .api-note {
    display: grid;
    gap: 0.25rem;
    justify-items: end;
    padding: 0.8rem 1rem;
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.62);
    border: 1px solid var(--border-soft);
    text-align: right;
  }

  .api-note span {
    color: var(--ink-500);
    font-size: 0.88rem;
  }

  video,
  .video-placeholder {
    width: 100%;
    border-radius: 1.35rem;
    background: #071a2f;
    aspect-ratio: 16 / 9;
  }

  .video-placeholder {
    display: grid;
    place-items: center;
    padding: 2rem;
    color: #e6f6ff;
    text-align: center;
  }

  .video-placeholder p {
    margin: 0.6rem 0 0;
    max-width: 34rem;
    color: rgba(230, 246, 255, 0.8);
    line-height: 1.6;
  }

  .details {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.9rem;
  }

  .detail-card,
  .failure-card {
    padding: 1rem;
    border-radius: 1.1rem;
    background: rgba(255, 255, 255, 0.58);
    border: 1px solid var(--border-soft);
  }

  .failure-card {
    background: rgba(255, 255, 255, 0.76);
    border-color: rgba(167, 45, 59, 0.18);
  }

  .detail-label {
    display: inline-block;
    margin-bottom: 0.5rem;
    font-size: 0.78rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--sky-700);
    font-weight: 700;
  }

  .detail-card strong,
  .failure-card strong {
    display: block;
    margin-bottom: 0.45rem;
    color: var(--ink-900);
  }

  .detail-card p,
  .failure-card p {
    margin: 0;
    color: var(--ink-700);
    line-height: 1.55;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
  }

  .secondary-link {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 2.8rem;
    padding: 0 1rem;
    border-radius: 999px;
    text-decoration: none;
    font-weight: 700;
    color: var(--sky-900);
    border: 1px solid var(--border-strong);
    background: rgba(255, 255, 255, 0.62);
  }

  .secondary-link.subtle {
    color: var(--ink-700);
  }

  .error {
    margin: 0;
    color: var(--danger-700);
    font-weight: 700;
  }

  @media (max-width: 1120px) {
    .portal-shell {
      grid-template-columns: 1fr;
    }

    .library {
      position: static;
      max-height: none;
    }
  }

  @media (max-width: 860px) {
    .topbar,
    .card-head,
    .details {
      grid-template-columns: 1fr;
    }

    .topbar,
    .card-head {
      display: grid;
    }

    .details {
      display: grid;
    }

    .api-note {
      justify-items: start;
      text-align: left;
    }
  }

  @media (max-width: 640px) {
    .portal-shell {
      padding: 1rem;
    }

    .library,
    .topbar,
    .player-card {
      padding: 1rem;
    }

    .video-link-top,
    .topbar-actions {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
