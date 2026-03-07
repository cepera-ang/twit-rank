import { useState, useEffect, useCallback, useRef } from 'react';
import type { ReactNode } from 'react';
import type { Post, ListInfo } from '../types';
import { fetchPosts, fetchLists } from '../api';
import { Tweet } from './Tweet';

const POSTS_PER_PAGE = 30;

type FeedType = 'forYou' | 'following' | string;

function dedupePosts(posts: Post[]): Post[] {
  const seen = new Set<string>();
  const out: Post[] = [];
  for (const post of posts) {
    if (seen.has(post.id)) continue;
    seen.add(post.id);
    out.push(post);
  }
  return out;
}

interface FeedProps {
  topNotice?: ReactNode;
}

export function Feed({ topNotice }: FeedProps) {
  const [posts, setPosts] = useState<Post[]>([]);
  const [related, setRelated] = useState<Record<string, Post>>({});
  const [lists, setLists] = useState<ListInfo[]>([]);
  const [currentFeed, setCurrentFeed] = useState<FeedType>('forYou');
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const nextOffsetRef = useRef(0);
  const hasMoreRef = useRef(true);
  const loadMoreInFlightRef = useRef(false);
  const requestIdRef = useRef(0);

  const loadPosts = useCallback(async (reset = false) => {
    if (!reset) {
      if (loadMoreInFlightRef.current || !hasMoreRef.current) {
        return;
      }
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
      const response = await fetchPosts(POSTS_PER_PAGE, requestOffset, currentFeed);

      if (requestId !== requestIdRef.current) {
        return;
      }

      if (reset) {
        setPosts(dedupePosts(response.posts));
        setRelated(response.related || {});
      } else {
        setPosts((prev) => {
          const seen = new Set(prev.map((p) => p.id));
          const uniqueIncoming = response.posts.filter((p) => !seen.has(p.id));
          return [...prev, ...uniqueIncoming];
        });
        setRelated((prev) => ({ ...prev, ...(response.related || {}) }));
      }
      nextOffsetRef.current = requestOffset + response.posts.length;
      hasMoreRef.current = response.has_more;
      setHasMore(response.has_more);
    } catch (err) {
      if (requestId === requestIdRef.current) {
        setError(err instanceof Error ? err.message : 'Failed to load posts');
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
  }, [currentFeed]);

  const loadLists = useCallback(async () => {
    try {
      const response = await fetchLists();
      setLists(response.lists);
    } catch (err) {
      console.error('Failed to load lists:', err);
    }
  }, []);

  const handleFeedChange = (feed: FeedType) => {
    if (feed !== currentFeed) {
      setCurrentFeed(feed);
      setPosts([]);
      nextOffsetRef.current = 0;
      hasMoreRef.current = true;
      setHasMore(true);
    }
  };

  const handleFeedbackChange = (id: string, newFeedback: number) => {
    setPosts((prev) =>
      prev.map((p) => (p.id === id ? { ...p, feedback: newFeedback } : p))
    );
  };

  useEffect(() => {
    loadLists();
  }, [loadLists]);

  useEffect(() => {
    loadPosts(true);
  }, [loadPosts]);

  return (
    <div className="flex-1 border-x border-gray-200 dark:border-gray-800 min-h-screen max-w-[600px]">
      {/* Header with tabs */}
      <div className="sticky top-0 z-10 backdrop-blur-md bg-white/80 dark:bg-black/80 border-b border-gray-200 dark:border-gray-800">
        <div className="flex items-center justify-between px-4 py-3">
          <h1 className="text-xl font-bold text-gray-900 dark:text-white">Home</h1>
          <button
            onClick={() => loadPosts(true)}
            disabled={loading}
            className={`p-2 rounded-full hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors ${loading ? 'opacity-50 cursor-not-allowed' : ''
              }`}
            title="Reload"
          >
            <svg
              className={`w-5 h-5 text-gray-600 dark:text-gray-400 ${loading ? 'animate-spin' : ''}`}
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"
              />
            </svg>
          </button>
        </div>

        {/* Feed tabs */}
        <div className="flex border-b border-gray-200 dark:border-gray-800">
          <button
            onClick={() => handleFeedChange('forYou')}
            className={`flex-1 py-4 text-sm font-medium transition-colors relative ${currentFeed === 'forYou'
                ? 'text-gray-900 dark:text-white'
                : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'
              }`}
          >
            For you
            {currentFeed === 'forYou' && (
              <div className="absolute bottom-0 left-1/2 -translate-x-1/2 w-16 h-1 bg-blue-500 rounded-full" />
            )}
          </button>
          <button
            onClick={() => handleFeedChange('following')}
            className={`flex-1 py-4 text-sm font-medium transition-colors relative ${currentFeed === 'following'
                ? 'text-gray-900 dark:text-white'
                : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'
              }`}
          >
            Following
            {currentFeed === 'following' && (
              <div className="absolute bottom-0 left-1/2 -translate-x-1/2 w-16 h-1 bg-blue-500 rounded-full" />
            )}
          </button>

          {/* Lists dropdown */}
          {lists.length > 0 && (
            <div className="relative group">
              <button
                className={`flex-1 py-4 px-4 text-sm font-medium transition-colors ${currentFeed.startsWith('list:')
                    ? 'text-gray-900 dark:text-white'
                    : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'
                  }`}
              >
                Lists ▾
                {currentFeed.startsWith('list:') && (
                  <div className="absolute bottom-0 left-1/2 -translate-x-1/2 w-12 h-1 bg-blue-500 rounded-full" />
                )}
              </button>
              <div className="absolute top-full left-0 mt-1 w-48 bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all z-20">
                {lists.map((list) => (
                  <button
                    key={list.id}
                    onClick={() => handleFeedChange(list.id)}
                    className={`w-full text-left px-4 py-2 text-sm hover:bg-gray-100 dark:hover:bg-gray-800 first:rounded-t-lg last:rounded-b-lg ${currentFeed === list.id ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-600' : ''
                      }`}
                  >
                    {list.name}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Content */}
      {topNotice ? <div className="border-b border-gray-200 dark:border-gray-800">{topNotice}</div> : null}
      {loading ? (
        <div className="flex items-center justify-center py-12">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
        </div>
      ) : error ? (
        <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
          <p className="text-red-500 mb-4">{error}</p>
          <button
            onClick={() => loadPosts(true)}
            className="px-4 py-2 bg-blue-500 text-white rounded-full hover:bg-blue-600 transition-colors"
          >
            Retry
          </button>
        </div>
      ) : posts.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-12 px-4 text-center">
          <p className="text-gray-500 mb-4">No posts in this feed yet</p>
          <p className="text-gray-400 text-sm">The archiver is collecting tweets in the background</p>
        </div>
      ) : (
        <>
          {posts.map((post) => (
            <Tweet
              key={post.id}
              post={post}
              related={related}
              onFeedbackChange={handleFeedbackChange}
            />
          ))}

          {/* Load more */}
          <div className="flex justify-center py-6">
            <button
              onClick={() => loadPosts(false)}
              disabled={loadingMore || !hasMore}
              className="px-6 py-2 bg-transparent border border-gray-300 dark:border-gray-700 text-gray-700 dark:text-gray-300 rounded-full hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors disabled:opacity-50"
            >
              {loadingMore ? (
                <span className="flex items-center gap-2">
                  <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-current"></div>
                  Loading...
                </span>
              ) : hasMore ? (
                'Load more'
              ) : (
                'No more posts'
              )}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
