import type {
  Post,
  PostsResponse,
  FeedbackResponse,
  ListsResponse,
  BuildInfo,
  SearchRequest,
  SettingsPayload,
  SettingsStatus,
  SaveSettingsResponse,
} from './types';

const API_BASE = '/api';

export async function fetchPosts(
  limit = 50,
  offset = 0,
  feed?: string,
  query?: string
): Promise<PostsResponse> {
  const params = new URLSearchParams();
  params.set('limit', limit.toString());
  params.set('offset', offset.toString());
  if (feed) params.set('feed', feed);
  if (query) params.set('q', query);

  const res = await fetch(`${API_BASE}/posts?${params}`);
  if (!res.ok) throw new Error('Failed to fetch posts');
  return res.json();
}

export async function fetchPost(id: string): Promise<Post | null> {
  const res = await fetch(`${API_BASE}/post/${id}`);
  if (!res.ok) throw new Error('Failed to fetch post');
  return res.json();
}

export async function searchPosts(
  params: SearchRequest,
  limit = 50,
  offset = 0
): Promise<PostsResponse> {
  const query = new URLSearchParams();
  query.set('limit', limit.toString());
  query.set('offset', offset.toString());

  const entries = Object.entries(params) as Array<[keyof SearchRequest, SearchRequest[keyof SearchRequest]]>;
  for (const [key, value] of entries) {
    if (value === undefined || value === null || value === '' || value === false) continue;
    query.set(key, String(value));
  }

  const res = await fetch(`${API_BASE}/search?${query}`);
  if (!res.ok) {
    const message = await res.text().catch(() => 'Failed to search posts');
    throw new Error(message || 'Failed to search posts');
  }
  return res.json();
}

export async function fetchLists(): Promise<ListsResponse> {
  const res = await fetch(`${API_BASE}/lists`);
  if (!res.ok) throw new Error('Failed to fetch lists');
  return res.json();
}

export async function fetchFeeds(): Promise<string[]> {
  const res = await fetch(`${API_BASE}/feeds`);
  if (!res.ok) throw new Error('Failed to fetch feeds');
  return res.json();
}

export async function fetchBuildInfo(): Promise<BuildInfo> {
  const res = await fetch(`${API_BASE}/build`);
  if (!res.ok) throw new Error('Failed to fetch build info');
  return res.json();
}

export async function fetchSettingsStatus(): Promise<SettingsStatus> {
  const res = await fetch(`${API_BASE}/settings/status`);
  if (!res.ok) throw new Error('Failed to fetch settings status');
  return res.json();
}

export async function fetchSettings(): Promise<SettingsPayload> {
  const res = await fetch(`${API_BASE}/settings`);
  if (!res.ok) throw new Error('Failed to fetch settings');
  return res.json();
}

export async function saveSettings(settings: SettingsPayload): Promise<SaveSettingsResponse> {
  const res = await fetch(`${API_BASE}/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  });
  if (!res.ok) {
    const message = await res.text().catch(() => 'Failed to save settings');
    throw new Error(message || 'Failed to save settings');
  }
  return res.json();
}

export async function sendFeedback(id: string, value: 1 | -1 | 0, reason?: string): Promise<FeedbackResponse> {
  const res = await fetch(`${API_BASE}/feedback`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, value, reason }),  // Send ID as string to avoid precision loss
  });
  if (!res.ok) throw new Error('Failed to send feedback');
  return res.json();
}
