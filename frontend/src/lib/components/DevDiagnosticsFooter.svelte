<script lang="ts">
  import { onMount } from 'svelte';
  import { apiUrl } from '$lib/api';

  type PlaybackMode = 'raw' | 'hls';
  type LiveMode = 'sse' | 'polling' | 'offline';
  type VideoStatus = 'pending' | 'processing' | 'ready' | 'failed';

  type DiagnosticCard = {
    key: string;
    label: string;
    detail: string;
    state: 'ok' | 'warn' | 'error' | 'loading';
    httpStatus: number | null;
    payloadPreview: string;
  };

  type StatusPayload = {
    status: VideoStatus;
    hls_playlist_url: string | null;
  };

  export let videoId: string | null = null;
  export let liveMode: LiveMode = 'offline';
  export let playbackMode: PlaybackMode = 'raw';
  export let recentVideoCount = 0;

  const POLL_INTERVAL_MS = 4000;

  let diagnosticsOpen = true;
  let cards: DiagnosticCard[] = [
    loadingCard('info', 'Service info'),
    loadingCard('healthz', '/healthz'),
    loadingCard('api-healthz', '/api/healthz'),
    loadingCard('videos', '/api/videos'),
    loadingCard('status', 'Video status')
  ];
  let lastRefreshedAt = 'waiting for first poll';

  onMount(() => {
    let cancelled = false;
    let pollHandle: ReturnType<typeof setInterval> | null = null;

    async function bootstrap() {
      await refreshDiagnostics();
      if (cancelled) {
        return;
      }

      pollHandle = setInterval(() => {
        void refreshDiagnostics();
      }, POLL_INTERVAL_MS);
    }

    bootstrap();

    return () => {
      cancelled = true;
      if (pollHandle) {
        clearInterval(pollHandle);
      }
    };
  });

  async function refreshDiagnostics() {
    const nextCards = await Promise.all([
      fetchJsonCard('info', 'Service info', '/'),
      fetchJsonCard('healthz', '/healthz', '/healthz'),
      fetchJsonCard('api-healthz', '/api/healthz', '/api/healthz'),
      fetchVideosCard(),
      fetchStatusCard()
    ]);

    cards = nextCards;
    lastRefreshedAt = new Date().toLocaleTimeString();
  }

  async function fetchJsonCard(
    key: string,
    label: string,
    path: string
  ): Promise<DiagnosticCard> {
    try {
      const response = await fetch(apiUrl(path));
      const payload = await safeJson(response);
      const ok =
        response.ok &&
        payload &&
        typeof payload === 'object' &&
        'ok' in payload;

      return {
        key,
        label,
        detail:
          ok && payload && typeof payload === 'object' && 'service' in payload
            ? `${String(payload.service)} responded`
            : response.ok
              ? 'reachable'
              : extractError(payload),
        state: response.ok ? 'ok' : 'error',
        httpStatus: response.status,
        payloadPreview: formatPayload(payload)
      };
    } catch (error) {
      return {
        key,
        label,
        detail: error instanceof Error ? error.message : 'request failed',
        state: 'error',
        httpStatus: null,
        payloadPreview:
          error instanceof Error ? error.message : 'request failed'
      };
    }
  }

  async function fetchVideosCard(): Promise<DiagnosticCard> {
    try {
      const response = await fetch(apiUrl('/api/videos'));
      const payload = await safeJson(response);

      if (!response.ok) {
        return {
          key: 'videos',
          label: '/api/videos',
          detail: extractError(payload),
          state: 'error',
          httpStatus: response.status,
          payloadPreview: formatPayload(payload)
        };
      }

      const count = Array.isArray(payload) ? payload.length : recentVideoCount;

      return {
        key: 'videos',
        label: '/api/videos',
        detail: `${count} recent uploads visible`,
        state: 'ok',
        httpStatus: response.status,
        payloadPreview: formatPayload(payload)
      };
    } catch (error) {
      return {
        key: 'videos',
        label: '/api/videos',
        detail: error instanceof Error ? error.message : 'request failed',
        state: 'error',
        httpStatus: null,
        payloadPreview:
          error instanceof Error ? error.message : 'request failed'
      };
    }
  }

  async function fetchStatusCard(): Promise<DiagnosticCard> {
    if (!videoId) {
      return {
        key: 'status',
        label: 'Video status',
        detail: 'no active video selected',
        state: 'warn',
        httpStatus: null,
        payloadPreview: 'No active video selected.'
      };
    }

    try {
      const response = await fetch(apiUrl(`/api/videos/${videoId}/status`));
      const payload = (await safeJson(response)) as
        | StatusPayload
        | { error?: string }
        | null;

      if (!response.ok) {
        return {
          key: 'status',
          label: 'Video status',
          detail: extractError(payload),
          state: 'error',
          httpStatus: response.status,
          payloadPreview: formatPayload(payload)
        };
      }

      const status = (payload as StatusPayload).status;
      const hasHls = Boolean((payload as StatusPayload).hls_playlist_url);

      return {
        key: 'status',
        label: 'Video status',
        detail: hasHls ? `${status} with HLS` : status,
        state:
          status === 'failed' ? 'error' : status === 'ready' ? 'ok' : 'warn',
        httpStatus: response.status,
        payloadPreview: formatPayload(payload)
      };
    } catch (error) {
      return {
        key: 'status',
        label: 'Video status',
        detail: error instanceof Error ? error.message : 'request failed',
        state: 'error',
        httpStatus: null,
        payloadPreview:
          error instanceof Error ? error.message : 'request failed'
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

  function extractError(payload: unknown): string {
    if (payload && typeof payload === 'object' && 'error' in payload) {
      return typeof payload.error === 'string'
        ? payload.error
        : 'request failed';
    }

    return 'request failed';
  }

  function loadingCard(key: string, label: string): DiagnosticCard {
    return {
      key,
      label,
      detail: 'polling...',
      state: 'loading',
      httpStatus: null,
      payloadPreview: 'Waiting for the first response payload...'
    };
  }

  function sessionModeLabel(): string {
    return `${liveMode.toUpperCase()} updates • ${playbackMode.toUpperCase()} playback`;
  }

  function formatPayload(payload: unknown): string {
    if (payload == null) {
      return 'null';
    }

    if (typeof payload === 'string') {
      return truncate(payload);
    }

    try {
      return truncate(JSON.stringify(payload, null, 2));
    } catch {
      return '[unserializable payload]';
    }
  }

  function truncate(value: string): string {
    const MAX_PREVIEW_CHARS = 420;
    if (value.length <= MAX_PREVIEW_CHARS) {
      return value;
    }

    return `${value.slice(0, MAX_PREVIEW_CHARS)}\n...`;
  }
</script>

<footer class:open={diagnosticsOpen} class="dev-footer">
  <div class="footer-bar">
    <div>
      <p class="eyebrow">Dev diagnostics</p>
      <strong>Last refresh {lastRefreshedAt}</strong>
      <p>{sessionModeLabel()}</p>
    </div>

    <div class="footer-actions">
      <span class="summary-pill">{recentVideoCount} uploads listed</span>
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
    <div class="diagnostic-grid">
      {#each cards as card}
        <div class="diagnostic-card">
          <div class="card-head">
            <span
              class:ok={card.state === 'ok'}
              class:warn={card.state === 'warn'}
              class:error={card.state === 'error'}
              class:loading={card.state === 'loading'}
              class="state-dot"
            ></span>
            <strong>{card.label}</strong>
          </div>
          <p>{card.detail}</p>
          <span class="http-meta">
            {#if card.httpStatus}
              HTTP {card.httpStatus}
            {:else}
              no HTTP code
            {/if}
          </span>
          <pre>{card.payloadPreview}</pre>
        </div>
      {/each}

      <div class="diagnostic-card session-card">
        <div class="card-head">
          <span class="state-dot ok"></span>
          <strong>Session</strong>
        </div>
        <p>{sessionModeLabel()}</p>
        <span class="http-meta">
          {#if videoId}
            active video {videoId}
          {:else}
            watch portal idle
          {/if}
        </span>
        <pre>{JSON.stringify(
            { liveMode, playbackMode, recentVideoCount },
            null,
            2
          )}</pre>
      </div>
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

  .footer-bar strong,
  .diagnostic-card strong {
    display: block;
  }

  .footer-bar p,
  .diagnostic-card p {
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

  .diagnostic-grid {
    margin-top: 0.95rem;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
    gap: 0.8rem;
  }

  .diagnostic-card {
    min-width: 0;
    padding: 0.9rem;
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(196, 227, 250, 0.14);
  }

  .card-head {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    min-width: 0;
  }

  .card-head strong {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .state-dot {
    width: 0.7rem;
    height: 0.7rem;
    border-radius: 999px;
    flex: 0 0 auto;
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

  .http-meta {
    display: block;
    margin-top: 0.55rem;
    color: rgba(206, 229, 246, 0.82);
    font-size: 0.82rem;
  }

  pre {
    margin: 0.65rem 0 0;
    padding: 0.7rem;
    border-radius: 0.8rem;
    background: rgba(4, 18, 32, 0.34);
    color: rgba(228, 243, 255, 0.92);
    font-size: 0.76rem;
    line-height: 1.45;
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
