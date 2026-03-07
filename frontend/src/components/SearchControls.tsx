import type { FormEvent } from 'react';
import type { SearchRequest } from '../types';

interface SearchControlsProps {
  value: SearchRequest;
  onChange: (next: SearchRequest) => void;
  onApply: () => void;
  feeds: string[];
  compact?: boolean;
  className?: string;
  showQuery?: boolean;
  showSubmit?: boolean;
}

const KINDS: Array<SearchRequest['kind']> = ['any', 'original', 'reply', 'quote', 'retweet'];

export function SearchControls({
  value,
  onChange,
  onApply,
  feeds,
  compact = false,
  className = '',
  showQuery = true,
  showSubmit = true,
}: SearchControlsProps) {
  const setField = <K extends keyof SearchRequest,>(key: K, fieldValue: SearchRequest[K]) => {
    onChange({ ...value, [key]: fieldValue });
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    onApply();
  };

  const panelClass = compact
    ? 'space-y-3'
    : 'space-y-4 rounded-2xl border border-gray-200 dark:border-gray-800 bg-gray-50 dark:bg-gray-900 p-4';

  return (
    <form className={`${panelClass} ${className}`.trim()} onSubmit={handleSubmit}>
      {showQuery && (
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Query</label>
          <input
            type="text"
            value={value.q ?? ''}
            onChange={(e) => setField('q', e.target.value)}
            placeholder='words or "quoted phrase"'
            className="w-full rounded-2xl border border-gray-200 bg-white px-4 py-3 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
          <label className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
            <input
              type="checkbox"
              checked={value.mode === 'regex'}
              onChange={(e) => setField('mode', e.target.checked ? 'regex' : 'literal')}
            />
            Regex mode
          </label>
        </div>
      )}

      <div className={`grid gap-3 ${compact ? 'grid-cols-1' : 'grid-cols-1 md:grid-cols-2'}`}>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Author</label>
          <input
            type="text"
            value={value.author ?? ''}
            onChange={(e) => setField('author', e.target.value)}
            placeholder="@user or full name"
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Feed</label>
          <select
            value={value.feed ?? ''}
            onChange={(e) => setField('feed', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          >
            <option value="">Any feed</option>
            {feeds.map((feed) => (
              <option key={feed} value={feed}>
                {feed}
              </option>
            ))}
          </select>
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Created from</label>
          <input
            type="date"
            value={value.created_from ?? ''}
            onChange={(e) => setField('created_from', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Created to</label>
          <input
            type="date"
            value={value.created_to ?? ''}
            onChange={(e) => setField('created_to', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Min likes</label>
          <input
            type="number"
            min="0"
            value={value.min_likes ?? ''}
            onChange={(e) => setField('min_likes', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Min retweets</label>
          <input
            type="number"
            min="0"
            value={value.min_retweets ?? ''}
            onChange={(e) => setField('min_retweets', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Min replies</label>
          <input
            type="number"
            min="0"
            value={value.min_replies ?? ''}
            onChange={(e) => setField('min_replies', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Min views</label>
          <input
            type="number"
            min="0"
            value={value.min_views ?? ''}
            onChange={(e) => setField('min_views', e.target.value || undefined)}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
        </div>
      </div>

      <div className={`grid gap-3 ${compact ? 'grid-cols-1' : 'grid-cols-2'}`}>
        <div className="space-y-2">
          <label className="text-xs font-semibold uppercase tracking-wide text-gray-500">Kind</label>
          <select
            value={value.kind ?? 'any'}
            onChange={(e) => setField('kind', e.target.value as SearchRequest['kind'])}
            className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          >
            {KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {kind}
              </option>
            ))}
          </select>
        </div>
        <div className="flex flex-wrap items-end gap-4 text-sm text-gray-600 dark:text-gray-300">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={Boolean(value.has_media)}
              onChange={(e) => setField('has_media', e.target.checked)}
            />
            Has media
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={Boolean(value.has_photos)}
              onChange={(e) => setField('has_photos', e.target.checked)}
            />
            Photos
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={Boolean(value.has_videos)}
              onChange={(e) => setField('has_videos', e.target.checked)}
            />
            Videos
          </label>
        </div>
      </div>

      {showSubmit && (
        <button
          type="submit"
          className="w-full rounded-full bg-blue-500 px-4 py-3 text-sm font-semibold text-white transition-colors hover:bg-blue-600"
        >
          Search archive
        </button>
      )}
    </form>
  );
}
