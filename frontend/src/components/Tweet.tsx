import { useState, useMemo, useEffect } from 'react';
import type { Post } from '../types';
import { sendFeedback, fetchPost } from '../api';

interface TweetProps {
  post: Post;
  related?: Record<string, Post>;
  onFeedbackChange?: (id: string, newFeedback: number) => void;
  isQuote?: boolean;
}

// Generate a consistent color based on username
function getUserColor(username: string): string {
  const colors = [
    'from-blue-400 to-blue-600',
    'from-purple-400 to-purple-600',
    'from-pink-400 to-pink-600',
    'from-green-400 to-green-600',
    'from-yellow-400 to-orange-500',
    'from-red-400 to-red-600',
    'from-indigo-400 to-indigo-600',
    'from-teal-400 to-teal-600',
    'from-cyan-400 to-cyan-600',
    'from-rose-400 to-rose-600',
  ];
  const hash = username.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
  return colors[hash % colors.length];
}

// Process tweet HTML content from the current backend renderer.
function processContent(html: string): string {
  try {
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const anchors = doc.querySelectorAll('a');
    anchors.forEach((a) => {
      a.setAttribute('target', '_blank');
      a.setAttribute('rel', 'noopener noreferrer');
      a.classList.add('text-blue-500', 'hover:underline');
    });
    return doc.body.innerHTML;
  } catch {
    return html;
  }
}

function formatDate(dateStr: string): string {
  const date = new Date(dateStr);
  if (isNaN(date.getTime())) return dateStr;

  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffHours < 1) {
    const diffMins = Math.floor(diffMs / (1000 * 60));
    return `${diffMins}m`;
  } else if (diffHours < 24) {
    return `${diffHours}h`;
  } else if (diffDays < 7) {
    return `${diffDays}d`;
  } else {
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  }
}

function formatFullDate(dateStr: string): string {
  const date = new Date(dateStr);
  if (isNaN(date.getTime())) return dateStr;
  return date.toLocaleString('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
    month: 'short',
    day: 'numeric',
    year: 'numeric'
  });
}


function formatNumber(num: number): string {
  if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
  if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
  return num.toString();
}

const MEDIA_ERROR_LABELS: Record<number, string> = {
  1: 'aborted',
  2: 'network',
  3: 'decode',
  4: 'not-supported',
};

function isInteractiveTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest('a, button, video, audio, input, textarea, select, label, [role="button"]')
  );
}

// Feedback Modal Component
function FeedbackModal({
  isOpen,
  voteType,
  onSubmit,
  onClose
}: {
  isOpen: boolean;
  voteType: 1 | -1;
  onSubmit: (reason: string) => void;
  onClose: () => void;
}) {
  const [reason, setReason] = useState('');

  if (!isOpen) return null;

  const handleSubmit = () => {
    onSubmit(reason);
    setReason('');
  };

  const handleSkip = () => {
    onSubmit('');
    setReason('');
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
      <div
        className="bg-white dark:bg-gray-900 rounded-2xl p-6 max-w-md w-full mx-4 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-lg font-bold text-gray-900 dark:text-white mb-2">
          {voteType === 1 ? '👍 Upvote' : '👎 Downvote'}
        </h3>
        <p className="text-gray-600 dark:text-gray-400 text-sm mb-4">
          Why did you {voteType === 1 ? 'like' : 'dislike'} this? (optional)
        </p>
        <textarea
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder="e.g., Great insight, Useful info, Off-topic, Misleading..."
          className="w-full p-3 border border-gray-200 dark:border-gray-700 rounded-xl bg-gray-50 dark:bg-gray-800 text-gray-900 dark:text-white resize-none focus:outline-none focus:ring-2 focus:ring-blue-500"
          rows={3}
          autoFocus
        />
        <div className="flex gap-3 mt-4">
          <button
            onClick={handleSkip}
            className="flex-1 px-4 py-2 text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-full transition-colors"
          >
            Skip
          </button>
          <button
            onClick={handleSubmit}
            className={`flex-1 px-4 py-2 rounded-full font-semibold text-white transition-colors ${voteType === 1
              ? 'bg-green-500 hover:bg-green-600'
              : 'bg-orange-500 hover:bg-orange-600'
              }`}
          >
            Submit
          </button>
        </div>
      </div>
    </div>
  );
}

// Quoted Tweet component
function QuotedTweet({ quoteId, preloaded }: { quoteId: string; preloaded?: Post | null }) {
  const [quote, setQuote] = useState<Post | null>(preloaded || null);
  const [loading, setLoading] = useState(!preloaded);

  useEffect(() => {
    if (preloaded || quote) return;
    fetchPost(quoteId)
      .then(setQuote)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, [quoteId, preloaded, quote]);

  if (loading) {
    return (
      <div className="mt-3 border border-gray-200 dark:border-gray-700 rounded-2xl p-3 animate-pulse">
        <div className="h-4 bg-gray-200 dark:bg-gray-700 rounded w-1/4 mb-2"></div>
        <div className="h-3 bg-gray-200 dark:bg-gray-700 rounded w-3/4"></div>
      </div>
    );
  }

  if (!quote) {
    return (
      <a
        href={`https://x.com/i/status/${quoteId}`}
        target="_blank"
        rel="noopener noreferrer"
        className="mt-3 block border border-gray-200 dark:border-gray-700 rounded-2xl p-3 text-gray-500 hover:bg-gray-50 dark:hover:bg-gray-800/50"
      >
        View quoted post →
      </a>
    );
  }

  const avatarColor = getUserColor(quote.user);
  const hasUserPic = quote.user_pic && quote.user_pic.length > 0;

  return (
    <a
      href={quote.link}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(e) => e.stopPropagation()}
      className="mt-3 block border border-gray-200 dark:border-gray-700 rounded-2xl p-3 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
    >
      <div className="flex items-center gap-2 mb-1">
        {hasUserPic ? (
          <img src={quote.user_pic!} alt="" className="w-5 h-5 rounded-full" />
        ) : (
          <div className={`w-5 h-5 rounded-full bg-gradient-to-br ${avatarColor} flex items-center justify-center text-white text-[10px] font-bold`}>
            {quote.user.charAt(0).toUpperCase()}
          </div>
        )}
        <span className="font-bold text-sm text-gray-900 dark:text-white">{quote.fullname}</span>
        <span className="text-gray-500 text-sm">@{quote.user}</span>
        <span className="text-gray-500 text-sm">· {formatDate(quote.published)}</span>
      </div>
      <div
        className="text-sm text-gray-900 dark:text-white line-clamp-3 tweet-content"
        dangerouslySetInnerHTML={{ __html: processContent(quote.content) }}
      />
      {quote.photos && quote.photos.length > 0 && (
        <img
          src={quote.photos[0]}
          alt=""
          className="mt-2 rounded-xl max-h-40 object-cover"
        />
      )}
    </a>
  );
}

// Parent tweet shown above a reply (compact, with thread line)
function ParentTweet({ parent }: { parent: Post }) {
  const avatarColor = getUserColor(parent.user);
  const hasUserPic = parent.user_pic && parent.user_pic.length > 0;

  return (
    <div
      className="px-4 pt-3 pb-0 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-900/50 transition-colors"
      onClick={(e) => {
        if (isInteractiveTarget(e.target)) return;
        window.open(parent.link, '_blank');
      }}
    >
      <div className="flex gap-3">
        {/* Avatar with thread line */}
        <div className="flex flex-col items-center flex-shrink-0">
          <a
            href={`https://x.com/${parent.user}`}
            target="_blank"
            rel="noopener noreferrer"
            onClick={(e) => e.stopPropagation()}
          >
            {hasUserPic ? (
              <img src={parent.user_pic!} alt={parent.fullname} className="w-10 h-10 rounded-full object-cover" />
            ) : (
              <div className={`w-10 h-10 rounded-full bg-gradient-to-br ${avatarColor} flex items-center justify-center text-white font-bold text-sm`}>
                {parent.user.charAt(0).toUpperCase()}
              </div>
            )}
          </a>
          {/* Thread connector line */}
          <div className="w-0.5 flex-1 mt-1 bg-gray-300 dark:bg-gray-700 min-h-[8px]" />
        </div>
        {/* Content */}
        <div className="flex-1 min-w-0 pb-3">
          <div className="flex items-center gap-1 text-sm">
            <a
              href={`https://x.com/${parent.user}`}
              target="_blank"
              rel="noopener noreferrer"
              className="font-bold text-gray-900 dark:text-white truncate hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              {parent.fullname}
            </a>
            <span className="text-gray-500 truncate">@{parent.user}</span>
            <span className="text-gray-500">·</span>
            <span className="text-gray-500">{formatDate(parent.published)}</span>
          </div>
          <div
            className="mt-1 text-gray-900 dark:text-white text-[15px] leading-normal break-words tweet-content"
            dangerouslySetInnerHTML={{ __html: processContent(parent.content) }}
          />
        </div>
      </div>
    </div>
  );
}

function InlineVideo({ video }: { video: Post['videos'][number] }) {
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [sourceIdx, setSourceIdx] = useState(0);
  const isGif = video.kind === 'animated_gif';
  const sourceUrls = useMemo(() => {
    const out: string[] = [];
    const pushUnique = (u: string | undefined) => {
      if (!u) return;
      if (!out.includes(u)) out.push(u);
    };
    if (video.sources) {
      for (const u of video.sources) pushUnique(u);
    }
    pushUnique(video.url);
    return out;
  }, [video.sources, video.url]);
  const activeSrc = sourceUrls[sourceIdx] || video.url;

  return (
    <div className="space-y-1">
      <video
        key={activeSrc}
        src={activeSrc}
        poster={video.poster || undefined}
        controls={!isGif}
        autoPlay={isGif}
        loop={isGif}
        muted={isGif}
        playsInline
        preload="metadata"
        onClick={(e) => e.stopPropagation()}
        onMouseDown={(e) => e.stopPropagation()}
        onPointerDown={(e) => e.stopPropagation()}
        onLoadedData={() => setErrorDetail(null)}
        onError={(e) => {
          const media = e.currentTarget;
          const code = media.error?.code ?? 0;
          const label = MEDIA_ERROR_LABELS[code] || 'unknown';
          const detail = `${label} (code ${code}, ready=${media.readyState}, net=${media.networkState})`;
          if (sourceIdx + 1 < sourceUrls.length) {
            const next = sourceIdx + 1;
            setSourceIdx(next);
            setErrorDetail(`source ${sourceIdx + 1}/${sourceUrls.length} failed: ${detail}; trying ${next + 1}/${sourceUrls.length}`);
            console.warn('Video source fallback', {
              from: media.currentSrc || activeSrc,
              to: sourceUrls[next],
              code,
            });
            return;
          }
          setErrorDetail(`all ${sourceUrls.length} source(s) failed: ${detail}`);
          console.error('Video playback error', {
            src: media.currentSrc || activeSrc,
            code,
            readyState: media.readyState,
            networkState: media.networkState,
          });
        }}
        className="w-full rounded-2xl overflow-hidden bg-black"
      />
      {errorDetail && (
        <div className="text-xs text-red-500">
          Video failed to load: {errorDetail}.{' '}
          <a
            href={activeSrc}
            target="_blank"
            rel="noopener noreferrer"
            className="underline"
            onClick={(e) => e.stopPropagation()}
          >
            Open video file
          </a>
        </div>
      )}
    </div>
  );
}

export function Tweet({ post, related, onFeedbackChange, isQuote = false }: TweetProps) {
  const [feedback, setFeedback] = useState(post.feedback);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [showModal, setShowModal] = useState(false);
  const [pendingVote, setPendingVote] = useState<1 | -1 | null>(null);
  const [retweetData, setRetweetData] = useState<Post | null>(null);

  const retweetId = post.retweet_id;

  // Prefer preloaded retweet data immediately to avoid briefly showing the reposter as the author.
  const preloadedRetweet = retweetId ? related?.[retweetId] : undefined;
  const effectiveRetweet = retweetData || preloadedRetweet || null;

  const fullDate = useMemo(() => formatFullDate(post.published), [post.published]);

  const isRetweet = retweetId != null;
  const displayPost = effectiveRetweet || post;
  const displayUser = displayPost.user;
  const displayFullname = displayPost.fullname;
  const displayUserPic = displayPost.user_pic;
  const hasUserPic = Boolean(displayUserPic && displayUserPic.length > 0);
  const avatarColor = useMemo(() => getUserColor(displayUser), [displayUser]);
  const processedContent = useMemo(
    () => processContent(displayPost.content),
    [displayPost.content]
  );

  // Use preloaded retweet data or fetch on-demand as fallback
  useEffect(() => {
    if (!retweetId) return;
    if (preloadedRetweet) return;
    fetchPost(retweetId)
      .then(rt => { if (rt) setRetweetData(rt); })
      .catch(err => console.error('Failed to fetch retweet:', err));
  }, [retweetId, preloadedRetweet]);

  const handleVoteClick = async (value: 1 | -1) => {
    if (isSubmitting) return;

    // If clicking the same vote again, toggle back to neutral (delete from DB)
    if (feedback === value) {
      setIsSubmitting(true);
      try {
        await sendFeedback(post.id, 0);
        setFeedback(0);
        onFeedbackChange?.(post.id, 0);
      } catch (error) {
        console.error('Failed to delete feedback:', error);
      } finally {
        setIsSubmitting(false);
      }
      return;
    }

    // For new votes, show the modal
    setPendingVote(value);
    setShowModal(true);
  };

  const handleFeedbackSubmit = async (reason: string) => {
    if (!pendingVote || isSubmitting) return;
    setShowModal(false);
    setIsSubmitting(true);
    try {
      await sendFeedback(post.id, pendingVote, reason || undefined);
      setFeedback(pendingVote);
      onFeedbackChange?.(post.id, pendingVote);
    } catch (error) {
      console.error('Failed to send feedback:', error);
    } finally {
      setIsSubmitting(false);
      setPendingVote(null);
    }
  };

  // Resolve parent tweet for replies
  const isReply = post.reply_to_id != null;
  const parentPost = isReply ? related?.[post.reply_to_id!] : undefined;
  const [parentData, setParentData] = useState<Post | null>(null);

  useEffect(() => {
    if (!isReply) return;
    const pid = post.reply_to_id;
    if (!pid) return;
    if (parentPost) return;
    fetchPost(pid)
      .then((p) => { if (p) setParentData(p); })
      .catch((err) => console.error('Failed to fetch parent:', err));
  }, [isReply, post.reply_to_id, parentPost]);

  if (isQuote) {
    return <QuotedTweet quoteId={post.id} />;
  }

  return (
    <>
      <FeedbackModal
        isOpen={showModal}
        voteType={pendingVote || 1}
        onSubmit={handleFeedbackSubmit}
        onClose={() => { setShowModal(false); setPendingVote(null); }}
      />
      {/* Parent tweet shown above reply */}
      {isReply && (parentPost || parentData) && <ParentTweet parent={(parentPost || parentData)!} />}
      <article
        className={`border-b border-gray-200 dark:border-gray-800 px-4 py-3 hover:bg-gray-50 dark:hover:bg-gray-900/50 transition-colors cursor-pointer${isReply && parentPost ? ' -mt-0' : ''}`}
        onClick={(e) => {
          if (isInteractiveTarget(e.target)) return;
          window.open(post.link, '_blank');
        }}
      >
        {/* Retweet indicator */}
        {isRetweet && (
          <div className="flex items-center gap-2 text-gray-500 text-sm mb-1 ml-12">
            <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M4.5 3.88l4.432 4.14-1.364 1.46L5.5 7.55V16c0 1.1.896 2 2 2H13v2H7.5c-2.209 0-4-1.79-4-4V7.55L1.432 9.48.068 8.02 4.5 3.88zM16.5 6H11V4h5.5c2.209 0 4 1.79 4 4v8.45l2.068-1.93 1.364 1.46-4.432 4.14-4.432-4.14 1.364-1.46 2.068 1.93V8c0-1.1-.896-2-2-2z" />
            </svg>
            <span>{post.fullname} reposted</span>
          </div>
        )}
        <div className="flex gap-3">
          {/* Avatar */}
          <a
            href={`https://x.com/${displayUser}`}
            target="_blank"
            rel="noopener noreferrer"
            className="flex-shrink-0"
            onClick={(e) => e.stopPropagation()}
          >
            {hasUserPic ? (
              <img
                src={displayUserPic!}
                alt={displayFullname}
                className="w-10 h-10 rounded-full object-cover hover:opacity-90 transition-opacity"
                onError={(e) => {
                  const target = e.target as HTMLImageElement;
                  target.style.display = 'none';
                  target.nextElementSibling?.classList.remove('hidden');
                }}
              />
            ) : null}
            <div className={`w-10 h-10 rounded-full bg-gradient-to-br ${avatarColor} flex items-center justify-center text-white font-bold text-sm hover:opacity-90 transition-opacity ${hasUserPic ? 'hidden' : ''}`}>
              {displayUser.charAt(0).toUpperCase()}
            </div>
          </a>
          {/* Content */}
          <div className="flex-1 min-w-0">
            {/* Header */}
            <div className="flex items-center gap-1 text-sm">
              <a
                href={`https://x.com/${displayUser}`}
                target="_blank"
                rel="noopener noreferrer"
                className="font-bold text-gray-900 dark:text-white truncate hover:underline"
                onClick={(e) => e.stopPropagation()}
              >
                {displayFullname}
              </a>
              <span className="text-gray-500 truncate">@{displayUser}</span>
              <span className="text-gray-500">·</span>
              <a
                href={post.link}
                target="_blank"
                rel="noopener noreferrer"
                className="text-gray-500 hover:underline"
                onClick={(e) => e.stopPropagation()}
                title={fullDate}
              >
                {formatDate(post.published)}
              </a>
            </div>

            {/* Tweet text */}
            <div
              className="mt-1 text-gray-900 dark:text-white text-[15px] leading-normal break-words tweet-content"
              dangerouslySetInnerHTML={{ __html: processedContent }}
            />

            {/* Photos */}
            {displayPost.photos && displayPost.photos.length > 0 && (
              <div className={`mt-3 grid gap-0.5 rounded-2xl overflow-hidden ${displayPost.photos.length === 1 ? 'grid-cols-1' :
                displayPost.photos.length === 2 ? 'grid-cols-2' :
                  displayPost.photos.length === 3 ? 'grid-cols-2' :
                    'grid-cols-2'
                }`}>
                {displayPost.photos.slice(0, 4).map((photo, i) => (
                  <a
                    key={i}
                    href={photo}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={(e) => e.stopPropagation()}
                    className={`block ${displayPost.photos.length === 3 && i === 0 ? 'row-span-2' : ''}`}
                  >
                    <img
                      src={photo}
                      alt={`Photo ${i + 1}`}
                      className="w-full h-full object-cover hover:opacity-90 transition-opacity"
                      style={{ maxHeight: displayPost.photos.length === 1 ? '500px' : '286px' }}
                    />
                  </a>
                ))}
              </div>
            )}

            {/* Videos / GIFs */}
            {displayPost.videos && displayPost.videos.length > 0 && (
              <div
                className="mt-3 space-y-2"
                onClick={(e) => e.stopPropagation()}
                onMouseDown={(e) => e.stopPropagation()}
                onPointerDown={(e) => e.stopPropagation()}
              >
                {displayPost.videos.map((v, i) => {
                  const videoKey = `${v.kind}-${v.url}-${(v.sources?.join("|")) ?? ""}-${i}`;
                  return <InlineVideo key={videoKey} video={v} />;
                })}
              </div>
            )}

            {/* Quoted Tweet */}
            {(displayPost.quote_id || post.quote_id) && (
              <QuotedTweet
                quoteId={(displayPost.quote_id || post.quote_id)!}
                preloaded={related?.[(displayPost.quote_id || post.quote_id)!]}
              />
            )}

            {/* Actions */}
            <div className="flex items-center justify-between mt-3 max-w-md">
              {/* Reply */}
              <button className="group flex items-center gap-1 text-gray-500 hover:text-blue-500 transition-colors">
                <div className="p-2 rounded-full group-hover:bg-blue-500/10">
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" strokeWidth={1.5} viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M12 20.25c4.97 0 9-3.694 9-8.25s-4.03-8.25-9-8.25S3 7.444 3 12c0 2.104.859 4.023 2.273 5.48.432.447.74 1.04.586 1.641a4.483 4.483 0 01-.923 1.785A5.969 5.969 0 006 21c1.282 0 2.47-.402 3.445-1.087.81.22 1.668.337 2.555.337z" />
                  </svg>
                </div>
                {post.replies > 0 && <span className="text-xs">{formatNumber(post.replies)}</span>}
              </button>

              {/* Repost */}
              <button className="group flex items-center gap-1 text-gray-500 hover:text-green-500 transition-colors">
                <div className="p-2 rounded-full group-hover:bg-green-500/10">
                  <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M4.5 3.88l4.432 4.14-1.364 1.46L5.5 7.55V16c0 1.1.896 2 2 2H13v2H7.5c-2.209 0-4-1.79-4-4V7.55L1.432 9.48.068 8.02 4.5 3.88zM16.5 6H11V4h5.5c2.209 0 4 1.79 4 4v8.45l2.068-1.93 1.364 1.46-4.432 4.14-4.432-4.14 1.364-1.46 2.068 1.93V8c0-1.1-.896-2-2-2z" />
                  </svg>
                </div>
                {post.retweets > 0 && <span className="text-xs">{formatNumber(post.retweets)}</span>}
              </button>

              {/* Upvote */}
              <button
                onClick={(e) => { e.stopPropagation(); handleVoteClick(1); }}
                disabled={isSubmitting}
                className={`group flex items-center gap-1 transition-colors ${feedback === 1 ? 'text-green-600' : 'text-gray-500 hover:text-green-600'
                  }`}
                title="Upvote"
              >
                <div className="p-2 rounded-full group-hover:bg-green-500/10">
                  <svg className="w-5 h-5" fill={feedback === 1 ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M5 15l7-7 7 7" />
                  </svg>
                </div>
              </button>

              {/* Downvote */}
              <button
                onClick={(e) => { e.stopPropagation(); handleVoteClick(-1); }}
                disabled={isSubmitting}
                className={`group flex items-center gap-1 transition-colors ${feedback === -1 ? 'text-orange-600' : 'text-gray-500 hover:text-orange-600'
                  }`}
                title="Downvote"
              >
                <div className="p-2 rounded-full group-hover:bg-orange-500/10">
                  <svg className="w-5 h-5" fill={feedback === -1 ? 'currentColor' : 'none'} stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M19 9l-7 7-7-7" />
                  </svg>
                </div>
              </button>

              {/* Views */}
              {post.views > 0 && (
                <div className="flex items-center gap-1 text-gray-500">
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" strokeWidth={1.5} viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M3 13.125C3 12.504 3.504 12 4.125 12h2.25c.621 0 1.125.504 1.125 1.125v6.75C7.5 20.496 6.996 21 6.375 21h-2.25A1.125 1.125 0 013 19.875v-6.75zM9.75 8.625c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125v11.25c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V8.625zM16.5 4.125c0-.621.504-1.125 1.125-1.125h2.25C20.496 3 21 3.504 21 4.125v15.75c0 .621-.504 1.125-1.125 1.125h-2.25a1.125 1.125 0 01-1.125-1.125V4.125z" />
                  </svg>
                  <span className="text-xs">{formatNumber(post.views)}</span>
                </div>
              )}

              {/* Share */}
              <a
                href={post.link}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => e.stopPropagation()}
                className="group flex items-center gap-1 text-gray-500 hover:text-blue-500 transition-colors"
              >
                <div className="p-2 rounded-full group-hover:bg-blue-500/10">
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" strokeWidth={1.5} viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" d="M13.5 6H5.25A2.25 2.25 0 003 8.25v10.5A2.25 2.25 0 005.25 21h10.5A2.25 2.25 0 0018 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25" />
                  </svg>
                </div>
              </a>
            </div>
          </div>
        </div>
      </article>
    </>
  );
}

