import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchFeeds, searchPosts } from '../api';
import type { Post, SearchRequest } from '../types';
import { SearchControls } from './SearchControls';
import { Tweet } from './Tweet';

const POSTS_PER_PAGE = 30;

interface SearchFeedProps {
  draft: SearchRequest;
  onDraftChange: (next: SearchRequest) => void;
  submitted: SearchRequest;
  onApply: () => void;
}

export function SearchFeed({
  draft,
  onDraftChange,
  submitted,
  onApply,
}: SearchFeedProps) {
  const [posts, setPosts] = useState<Post[]>([]);
  const [related, setRelated] = useState<Record<string, Post>>({});
  const [feeds, setFeeds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showFilters, setShowFilters] = useState(false);
  const nextOffsetRef = useRef(0);
  const hasMoreRef = useRef(true);
  const requestIdRef = useRef(0);
  const loadMoreInFlightRef = useRef(false);

  const loadResults = useCallback(async (reset = false) => {
    if (!reset) {
      if (loadMoreInFlightRef.current || !hasMoreRef.current) return;
      loadMoreInFlightRef.current = true;
      setLoadingMore(true);
    }

    const requestId = ++requestIdRef.current;
    try {
      if (reset) {
        setLoading(true);
        setError(null);
        nextOffsetRef.current = 0;
      }

      const requestOffset = reset ? 0 : nextOffsetRef.current;
      const response = await searchPosts(submitted, POSTS_PER_PAGE, requestOffset);
      if (requestId !== requestIdRef.current) return;

      if (reset) {
        setPosts(response.posts);
        setRelated(response.related || {});
      } else {
        setPosts((prev) => {
          const seen = new Set(prev.map((post) => post.id));
          const incoming = response.posts.filter((post) => !seen.has(post.id));
          return [...prev, ...incoming];
        });
        setRelated((prev) => ({ ...prev, ...(response.related || {}) }));
      }

      nextOffsetRef.current = requestOffset + response.posts.length;
      hasMoreRef.current = response.has_more;
      setHasMore(response.has_more);
    } catch (err) {
      if (requestId === requestIdRef.current) {
        setError(err instanceof Error ? err.message : 'Failed to search posts');
      }
    } finally {
      if (reset && requestId === requestIdRef.current) {
        setLoading(false);
      }
      if (!reset) {
        loadMoreInFlightRef.current = false;
        if (requestId === requestIdRef.current) {
          setLoadingMore(false);
        }
      }
    }
  }, [submitted]);

  useEffect(() => {
    fetchFeeds().then(setFeeds).catch((err) => console.error('Failed to load feeds', err));
  }, []);

  useEffect(() => {
    hasMoreRef.current = true;
    setHasMore(true);
    setPosts([]);
    setRelated({});
    loadResults(true);
  }, [loadResults]);

  const hasActiveFilters = Object.entries(submitted).some(([, value]) => {
    if (value === undefined || value === null || value === '' || value === false) {
      return false;
    }
    return true;
  });

  const handleFeedbackChange = (id: string, feedback: number) => {
    setPosts((prev) => prev.map((post) => (post.id === id ? { ...post, feedback } : post)));
  };

  const handleTopLevelSearch = (event: React.FormEvent) => {
    event.preventDefault();
    onApply();
  };

  return (
    <div className="flex-1 border-x border-gray-200 dark:border-gray-800 min-h-screen max-w-[600px]">
      <div className="sticky top-0 z-10 backdrop-blur-md bg-white/80 dark:bg-black/80 border-b border-gray-200 dark:border-gray-800">
        <div className="px-4 py-3">
          <h1 className="text-xl font-bold text-gray-900 dark:text-white">Search</h1>
          <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
            Query the archive by text, regex, and tweet metadata.
          </p>
        </div>
        <form onSubmit={handleTopLevelSearch} className="px-4 pb-4 space-y-3">
          <input
            type="text"
            value={draft.q ?? ''}
            onChange={(e) => onDraftChange({ ...draft, q: e.target.value })}
            placeholder='words or "quoted phrase"'
            className="w-full rounded-full border border-gray-200 bg-white px-4 py-3 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
          />
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
              <input
                type="checkbox"
                checked={draft.mode === 'regex'}
                onChange={(e) =>
                  onDraftChange({ ...draft, mode: e.target.checked ? 'regex' : 'literal' })
                }
              />
              Regex
            </label>
            <button
              type="button"
              onClick={() => setShowFilters((prev) => !prev)}
              className="rounded-full border border-gray-300 px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
            >
              {showFilters ? 'Hide filters' : 'Show filters'}
            </button>
            <button
              type="submit"
              className="ml-auto rounded-full bg-blue-500 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-blue-600"
            >
              Search
            </button>
          </div>
        </form>
      </div>

      {showFilters && (
        <div className="border-b border-gray-200 bg-gray-50 px-4 py-4 dark:border-gray-800 dark:bg-gray-900 lg:hidden">
          <SearchControls
            value={draft}
            onChange={onDraftChange}
            onApply={onApply}
            feeds={feeds}
            compact
            showQuery={false}
            showSubmit={false}
          />
        </div>
      )}

      <div className="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">
        {hasActiveFilters ? 'Newest matching archived posts' : 'Add a query or filter to search the archive'}
      </div>

      {loading ? (
        <div className="flex items-center justify-center py-12">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
        </div>
      ) : error ? (
        <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
          <p className="text-red-500 mb-4">{error}</p>
          <button
            onClick={() => loadResults(true)}
            className="px-4 py-2 bg-blue-500 text-white rounded-full hover:bg-blue-600 transition-colors"
          >
            Retry
          </button>
        </div>
      ) : posts.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
          <p className="text-gray-500 mb-4">
            {hasActiveFilters ? 'No archived posts matched this search' : 'No search filters applied yet'}
          </p>
          <p className="text-gray-400 text-sm">
            {hasActiveFilters ? 'Try broader terms or loosen the filters.' : 'Enter a query or metadata filter and submit the form.'}
          </p>
        </div>
      ) : (
        <>
          {posts.map((post) => (
            <Tweet key={post.id} post={post} related={related} onFeedbackChange={handleFeedbackChange} />
          ))}
          <div className="flex justify-center py-6">
            <button
              onClick={() => loadResults(false)}
              disabled={loadingMore || !hasMore}
              className="px-6 py-2 bg-transparent border border-gray-300 dark:border-gray-700 text-gray-700 dark:text-gray-300 rounded-full hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors disabled:opacity-50"
            >
              {loadingMore ? 'Loading...' : hasMore ? 'Load more' : 'No more results'}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
