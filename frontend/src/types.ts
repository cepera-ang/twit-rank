export interface Post {
  id: string;
  user: string;
  fullname: string;
  content: string;
  link: string;
  published: string;
  published_ts: number;
  feedback: number;
  likes: number;
  retweets: number;
  replies: number;
  views: number;
  feed_kind: string;
  // New fields
  user_pic: string | null;
  photos: string[];
  videos: { kind: string; url: string; sources?: string[]; poster: string | null }[];
  quote_id: string | null;
  retweet_id: string | null;
  reply_to_id: string | null;
}

export interface PostsResponse {
  posts: Post[];
  total: number;
  has_more: boolean;
  related: Record<string, Post>;
}

export interface SearchRequest {
  q?: string;
  mode?: 'literal' | 'regex';
  author?: string;
  feed?: string;
  created_from?: string;
  created_to?: string;
  min_likes?: string;
  min_retweets?: string;
  min_replies?: string;
  min_views?: string;
  has_photos?: boolean;
  has_videos?: boolean;
  has_media?: boolean;
  kind?: 'any' | 'original' | 'reply' | 'quote' | 'retweet';
}

export interface FeedbackResponse {
  success: boolean;
}

export interface ListInfo {
  id: string;
  name: string;
}

export interface ListsResponse {
  lists: ListInfo[];
}

export interface BuildInfo {
  build_id: string;
  build_epoch: number | null;
  package_version: string;
}

export interface SessionSettings {
  id: string;
  username: string;
  auth_token: string;
  ct0: string;
}

export interface SettingsStatus {
  settings_file_exists: boolean;
  has_sessions: boolean;
  needs_setup: boolean;
  session_count: number;
  settings_path: string;
}

export interface SettingsPayload {
  archive_path: string;
  sessions: SessionSettings[];
  list_ids: string[];
  poll_mins: number;
  max_pages: number;
  page_delay_ms: number;
  feed_delay_ms: number;
  tid_disable: boolean;
  tid_pairs_url: string;
}

export interface SaveSettingsResponse {
  success: boolean;
  restart_required: boolean;
}
