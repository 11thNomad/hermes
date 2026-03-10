<script lang="ts">
  import { onMount } from 'svelte';
  import { apiUrl } from '$lib/api';

  type PlaybackMode = 'raw' | 'hls';
  type LiveMode = 'sse' | 'offline';
  type VideoStatus = 'pending' | 'processing' | 'ready' | 'failed';

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

  type DiagnosticLine = {
    key: string;
    endpoint: string;
    response: string;
    state: 'ok' | 'warn' | 'error' | 'loading';
  };

  export let videoId: string | null = null;
  export let liveMode: LiveMode = 'offline';
  export let playbackMode: PlaybackMode = 'raw';
  export let recentVideos: VideoListItem[] = [];
  export let statusPayload: VideoStatusPayload | null = null;

  let diagnosticsOpen = true;
  let staticLines: DiagnosticLine[] = [
    loadingLine('info', '/'),
    loadingLine('healthz', '/healthz'),
    loadingLine('api-healthz', '/api/healthz')
  ];

  $: lines = [
    ...staticLines,
    buildVideosLine(),
    buildStatusLine(),
    buildSessionLine()
  ];

  onMount(() => {
    void refreshStaticDiagnostics();
  });

  async function refreshStaticDiagnostics() {
    staticLines = await Promise.all([
      fetchJsonLine('info', '/'),
      fetchJsonLine('healthz', '/healthz'),
      fetchJsonLine('api-healthz', '/api/healthz')
    ]);
  }

  async function fetchJsonLine(
    key: string,
    path: string
  ): Promise<DiagnosticLine> {
    try {
      const response = await fetch(apiUrl(path));
      const payload = await safeJson(response);

      return {
        key,
        endpoint: path,
        response: formatHttpResponse(response.status, payload),
        state: response.ok ? 'ok' : 'error'
      };
    } catch (error) {
      return {
        key,
        endpoint: path,
        response:
          error instanceof Error
            ? error.message
            : 'request failed unexpectedly',
        state: 'error'
      };
    }
  }

  async function safeJson(response: Response): Promise<unknown> {
    try {
      return await response.json();
    } catch {
      return null;
    }
  }

  function buildVideosLine(): DiagnosticLine {
    return {
      key: 'videos',
      endpoint: '/api/videos',
      response: formatHttpResponse(200, recentVideos),
      state: 'ok'
    };
  }

  function buildStatusLine(): DiagnosticLine {
    if (!videoId) {
      return {
        key: 'status',
        endpoint: '/api/videos/:id/status',
        response: 'no active video selected',
        state: 'warn'
      };
    }

    if (!statusPayload) {
      return {
        key: 'status',
        endpoint: `/api/videos/${videoId}/status`,
        response: 'waiting for the first status payload',
        state: 'loading'
      };
    }

    return {
      key: 'status',
      endpoint: `/api/videos/${videoId}/status`,
      response: formatPayload(statusPayload),
      state: statusState(statusPayload.status)
    };
  }

  function buildSessionLine(): DiagnosticLine {
    return {
      key: 'session',
      endpoint: 'session',
      response: JSON.stringify({
        liveMode,
        playbackMode,
        recentVideoCount: recentVideos.length,
        activeVideoId: videoId
      }),
      state: liveMode === 'offline' && videoId ? 'warn' : 'ok'
    };
  }

  function statusState(
    value: VideoStatus
  ): 'ok' | 'warn' | 'error' | 'loading' {
    switch (value) {
      case 'ready':
        return 'ok';
      case 'failed':
        return 'error';
      case 'pending':
      case 'processing':
        return 'warn';
      default:
        return 'loading';
    }
  }

  function loadingLine(key: string, endpoint: string): DiagnosticLine {
    return {
      key,
      endpoint,
      response: 'waiting for response',
      state: 'loading'
    };
  }

  function sessionModeLabel(): string {
    return `${liveMode.toUpperCase()} updates • ${playbackMode.toUpperCase()} playback`;
  }

  function formatHttpResponse(status: number, payload: unknown): string {
    return `HTTP ${status} ${formatPayload(payload)}`;
  }

  function formatPayload(payload: unknown): string {
    if (payload == null) {
      return 'null';
    }

    if (typeof payload === 'string') {
      return truncate(payload);
    }

    try {
      return truncate(JSON.stringify(payload));
    } catch {
      return '[unserializable payload]';
    }
  }

  function truncate(value: string): string {
    const MAX_PREVIEW_CHARS = 240;
    if (value.length <= MAX_PREVIEW_CHARS) {
      return value;
    }

    return `${value.slice(0, MAX_PREVIEW_CHARS - 3)}...`;
  }
</script>

<footer class:open={diagnosticsOpen} class="dev-footer">
  <div class="footer-bar">
    <div>
      <p class="eyebrow">Dev diagnostics</p>
      <strong>{sessionModeLabel()}</strong>
      <p>
        {#if videoId}
          active video {videoId}
        {:else}
          watch portal idle
        {/if}
      </p>
    </div>

    <div class="footer-actions">
      <span class="summary-pill">{recentVideos.length} uploads listed</span>
      <button
        class="toggle-button"
        type="button"
        on:click={() => {
          diagnosticsOpen = !diagnosticsOpen;
        }}
      >
        {diagnosticsOpen ? 'Hide footer' : 'Show footer'}
      </button>
    </div>
  </div>

  {#if diagnosticsOpen}
    <div class="diagnostic-lines">
      {#each lines as line}
        <div class="diagnostic-line">
          <span
            class:ok={line.state === 'ok'}
            class:warn={line.state === 'warn'}
            class:error={line.state === 'error'}
            class:loading={line.state === 'loading'}
            class="state-dot"
          ></span>
          <code>{line.endpoint} -&gt; {line.response}</code>
        </div>
      {/each}
    </div>
  {/if}
</footer>

<style>
  .dev-footer {
    position: sticky;
    bottom: 0;
    z-index: 10;
    margin-top: 1rem;
    padding: 0.95rem 1rem 1rem;
    border-radius: 1.4rem 1.4rem 0 0;
    background: rgba(12, 33, 57, 0.88);
    border: 1px solid rgba(182, 217, 243, 0.18);
    box-shadow: 0 -18px 38px rgba(8, 27, 47, 0.18);
    color: #f4fbff;
    backdrop-filter: blur(18px);
  }

  .footer-bar {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .footer-bar strong {
    display: block;
  }

  .footer-bar p {
    margin: 0.3rem 0 0;
    color: rgba(235, 246, 255, 0.78);
    line-height: 1.45;
  }

  .eyebrow {
    margin: 0 0 0.35rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 0.72rem;
    color: rgba(169, 217, 255, 0.88);
    font-weight: 700;
  }

  .footer-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.75rem;
  }

  .summary-pill {
    display: inline-flex;
    align-items: center;
    min-height: 2rem;
    padding: 0 0.85rem;
    border-radius: 999px;
    background: rgba(169, 217, 255, 0.16);
    color: white;
    font-weight: 700;
    font-size: 0.82rem;
  }

  .toggle-button {
    font: inherit;
    min-height: 2.35rem;
    padding: 0 0.95rem;
    border-radius: 999px;
    border: 1px solid rgba(196, 227, 250, 0.28);
    background: rgba(255, 255, 255, 0.08);
    color: white;
    font-weight: 700;
    cursor: pointer;
  }

  .diagnostic-lines {
    margin-top: 0.95rem;
    display: grid;
    gap: 0.55rem;
  }

  .diagnostic-line {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 0.7rem;
    align-items: start;
    padding: 0.35rem 0;
    border-bottom: 1px solid rgba(196, 227, 250, 0.1);
  }

  .diagnostic-line:last-child {
    border-bottom: 0;
  }

  .state-dot {
    width: 0.7rem;
    height: 0.7rem;
    border-radius: 999px;
    margin-top: 0.32rem;
    background: rgba(255, 255, 255, 0.28);
  }

  .state-dot.ok {
    background: #4fd1ad;
    box-shadow: 0 0 0 0.2rem rgba(79, 209, 173, 0.16);
  }

  .state-dot.warn {
    background: #f2bb4b;
    box-shadow: 0 0 0 0.2rem rgba(242, 187, 75, 0.16);
  }

  .state-dot.error {
    background: #ff7885;
    box-shadow: 0 0 0 0.2rem rgba(255, 120, 133, 0.16);
  }

  .state-dot.loading {
    background: #9ebdd7;
  }

  code {
    display: block;
    color: rgba(228, 243, 255, 0.94);
    font-size: 0.8rem;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  @media (max-width: 720px) {
    .footer-bar {
      flex-direction: column;
      align-items: flex-start;
    }

    .footer-actions {
      justify-content: flex-start;
    }
  }
</style>
