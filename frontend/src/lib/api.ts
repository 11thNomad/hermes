const API_BASE = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

export function apiUrl(path: string): string {
  return new URL(path, API_BASE).toString();
}
