import { useEffect, useMemo, useState } from 'react';
import { fetchFeeds, fetchSettingsStatus } from './api';
import { Feed } from './components/Feed';
import { SearchControls } from './components/SearchControls';
import { SearchFeed } from './components/SearchFeed';
import { SetupPage } from './components/SetupPage';
import { Sidebar } from './components/Sidebar';
import type { SearchRequest, SettingsStatus } from './types';

type AppMode = 'home' | 'search' | 'setup';

const DEFAULT_SEARCH: SearchRequest = {
  mode: 'literal',
  kind: 'any',
};

function parseUrlState(): { mode: AppMode; search: SearchRequest } {
  const params = new URLSearchParams(window.location.search);
  const rawMode = params.get('mode');
  const mode: AppMode =
    rawMode === 'search' || rawMode === 'setup' ? rawMode : 'home';
  return {
    mode,
    search: {
      q: params.get('q') || undefined,
      mode: params.get('search_mode') === 'regex' ? 'regex' : 'literal',
      author: params.get('author') || undefined,
      feed: params.get('feed') || undefined,
      created_from: params.get('created_from') || undefined,
      created_to: params.get('created_to') || undefined,
      min_likes: params.get('min_likes') || undefined,
      min_retweets: params.get('min_retweets') || undefined,
      min_replies: params.get('min_replies') || undefined,
      min_views: params.get('min_views') || undefined,
      has_photos: params.get('has_photos') === 'true',
      has_videos: params.get('has_videos') === 'true',
      has_media: params.get('has_media') === 'true',
      kind: (params.get('kind') as SearchRequest['kind']) || 'any',
    },
  };
}

function writeUrlState(mode: AppMode, search: SearchRequest) {
  const params = new URLSearchParams();
  if (mode !== 'home') {
    params.set('mode', mode);
  }
  if (mode === 'search') {
    const entries = Object.entries(search) as Array<[keyof SearchRequest, SearchRequest[keyof SearchRequest]]>;
    for (const [key, value] of entries) {
      if (value === undefined || value === null || value === '' || value === false) continue;
      if (key === 'mode') {
        params.set('search_mode', String(value));
      } else {
        params.set(key, String(value));
      }
    }
  }
  const query = params.toString();
  const url = query ? `${window.location.pathname}?${query}` : window.location.pathname;
  window.history.replaceState(null, '', url);
}

function SetupNotice({ onOpenSetup }: { onOpenSetup: () => void }) {
  return (
    <div className="px-4 py-4 bg-amber-50 text-amber-900 dark:bg-amber-950/30 dark:text-amber-100">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <div className="font-semibold">Setup is incomplete</div>
          <div className="text-sm opacity-80">
            Add X session cookies and list settings before using the archiver.
          </div>
        </div>
        <button
          onClick={onOpenSetup}
          className="rounded-full bg-amber-500 px-4 py-2 text-sm font-semibold text-white hover:bg-amber-600"
        >
          Open setup
        </button>
      </div>
    </div>
  );
}

function App() {
  const initial = useMemo(() => parseUrlState(), []);
  const [mode, setMode] = useState<AppMode>(initial.mode);
  const [searchDraft, setSearchDraft] = useState<SearchRequest>({ ...DEFAULT_SEARCH, ...initial.search });
  const [submittedSearch, setSubmittedSearch] = useState<SearchRequest>({ ...DEFAULT_SEARCH, ...initial.search });
  const [feeds, setFeeds] = useState<string[]>([]);
  const [settingsStatus, setSettingsStatus] = useState<SettingsStatus | null>(null);

  useEffect(() => {
    fetchFeeds().then(setFeeds).catch((err) => console.error('Failed to load feeds', err));
    fetchSettingsStatus()
      .then(setSettingsStatus)
      .catch((err) => console.error('Failed to load settings status', err));
  }, []);

  useEffect(() => {
    writeUrlState(mode, submittedSearch);
  }, [mode, submittedSearch]);

  const applySearch = () => {
    setMode('search');
    setSubmittedSearch({ ...DEFAULT_SEARCH, ...searchDraft });
  };

  const refreshSettingsStatus = () => {
    fetchSettingsStatus()
      .then(setSettingsStatus)
      .catch((err) => console.error('Failed to refresh settings status', err));
  };

  const topNotice =
    settingsStatus?.needs_setup ? <SetupNotice onOpenSetup={() => setMode('setup')} /> : undefined;

  const rightRail =
    mode === 'search' ? (
      <div className="hidden lg:block w-80 py-2 px-4">
        <div className="sticky top-0 pt-2 space-y-4">
          <SearchControls value={searchDraft} onChange={setSearchDraft} onApply={applySearch} feeds={feeds} />
        </div>
      </div>
    ) : mode === 'setup' ? (
      <div className="hidden lg:block w-80 py-2 px-4">
        <div className="sticky top-0 pt-2 rounded-2xl bg-gray-100 p-4 text-sm text-gray-600 dark:bg-gray-900 dark:text-gray-300">
          Saving setup writes one merged settings file. Restart the app after saving to start the archiver with the new sessions.
        </div>
      </div>
    ) : (
      <div className="hidden lg:block w-80 py-2 px-4">
        <div className="sticky top-0 pt-2 space-y-4">
          {settingsStatus?.needs_setup ? (
            <div className="rounded-2xl bg-amber-50 p-4 text-sm text-amber-900 dark:bg-amber-950/30 dark:text-amber-100">
              No usable settings found. Open Setup to save archive options and X session cookies.
            </div>
          ) : null}
          <div className="bg-gray-100 dark:bg-gray-900 rounded-2xl p-4">
            <h2 className="text-xl font-bold mb-2">About twit-rank</h2>
            <p className="text-gray-600 dark:text-gray-400 text-sm leading-relaxed">
              Current focus: archive X timelines locally and search them well. Ranking stays in the codebase as future work.
            </p>
          </div>
        </div>
      </div>
    );

  return (
    <div className="min-h-screen bg-white dark:bg-black text-gray-900 dark:text-white">
      <div className="flex justify-center">
        <Sidebar mode={mode} onModeChange={setMode} />
        {mode === 'search' ? (
          <SearchFeed
            draft={searchDraft}
            onDraftChange={setSearchDraft}
            submitted={submittedSearch}
            onApply={applySearch}
          />
        ) : mode === 'setup' ? (
          <SetupPage onSaved={refreshSettingsStatus} />
        ) : (
          <Feed topNotice={topNotice} />
        )}
        {rightRail}
      </div>
    </div>
  );
}

export default App;
