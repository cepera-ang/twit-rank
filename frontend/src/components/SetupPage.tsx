import { useEffect, useState } from 'react';
import { fetchSettings, saveSettings } from '../api';
import type { SettingsPayload, SessionSettings } from '../types';

interface SetupPageProps {
  onSaved: () => void;
}

interface ListSettings {
  id: string;
  slug: string;
}

const DEFAULT_SETTINGS: SettingsPayload = {
  archive_path: 'state/archive.sqlite',
  sessions: [{ id: '', username: '', auth_token: '', ct0: '' }],
  list_ids: [],
  poll_mins: 15,
  max_pages: 20,
  page_delay_ms: 2000,
  feed_delay_ms: 30000,
  tid_disable: false,
  tid_pairs_url:
    'https://raw.githubusercontent.com/fa0311/x-client-transaction-id-pair-dict/refs/heads/main/pair.json',
};

const DEFAULT_LIST: ListSettings = { id: '', slug: '' };

function parseListSettings(listIds: string[]): ListSettings[] {
  if (listIds.length === 0) {
    return [DEFAULT_LIST];
  }

  return listIds.map((entry) => {
    const [id, slug = ''] = entry.split(':', 2);
    return { id: id.trim(), slug: slug.trim() };
  });
}

function serializeListSettings(lists: ListSettings[]): string[] {
  return lists
    .map((list) => ({
      id: list.id.trim(),
      slug: list.slug.trim(),
    }))
    .filter((list) => list.id.length > 0)
    .map((list) => (list.slug.length > 0 ? `${list.id}:${list.slug}` : list.id));
}

export function SetupPage({ onSaved }: SetupPageProps) {
  const [settings, setSettings] = useState<SettingsPayload>(DEFAULT_SETTINGS);
  const [lists, setLists] = useState<ListSettings[]>([DEFAULT_LIST]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  useEffect(() => {
    fetchSettings()
      .then((data) => {
        setSettings({
          ...DEFAULT_SETTINGS,
          ...data,
          sessions: data.sessions.length > 0 ? data.sessions : DEFAULT_SETTINGS.sessions,
        });
        setLists(parseListSettings(data.list_ids));
      })
      .catch((err) => {
        console.error('Failed to load settings', err);
        setLists(parseListSettings(DEFAULT_SETTINGS.list_ids));
      })
      .finally(() => setLoading(false));
  }, []);

  const setList = (index: number, next: ListSettings) => {
    setLists((prev) => prev.map((list, idx) => (idx === index ? next : list)));
  };

  const addList = () => {
    setLists((prev) => [...prev, { ...DEFAULT_LIST }]);
  };

  const removeList = (index: number) => {
    setLists((prev) => {
      const next = prev.filter((_, idx) => idx !== index);
      return next.length > 0 ? next : [{ ...DEFAULT_LIST }];
    });
  };

  const setSession = (index: number, next: SessionSettings) => {
    setSettings((prev) => ({
      ...prev,
      sessions: prev.sessions.map((session, idx) => (idx === index ? next : session)),
    }));
  };

  const addSession = () => {
    setSettings((prev) => ({
      ...prev,
      sessions: [...prev.sessions, { id: '', username: '', auth_token: '', ct0: '' }],
    }));
  };

  const removeSession = (index: number) => {
    setSettings((prev) => ({
      ...prev,
      sessions: prev.sessions.filter((_, idx) => idx !== index),
    }));
  };

  const handleSave = async (event: React.FormEvent) => {
    event.preventDefault();
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const payload: SettingsPayload = {
        ...settings,
        list_ids: serializeListSettings(lists),
      };
      const result = await saveSettings(payload);
      setSaved(result.restart_required ? 'Settings saved. Restart twit-rank to apply new sessions to the archiver.' : 'Settings saved.');
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save settings');
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="flex-1 border-x border-gray-200 dark:border-gray-800 min-h-screen max-w-[600px] flex items-center justify-center">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
    );
  }

  return (
    <div className="flex-1 border-x border-gray-200 dark:border-gray-800 min-h-screen max-w-[600px]">
      <div className="sticky top-0 z-10 backdrop-blur-md bg-white/80 dark:bg-black/80 border-b border-gray-200 dark:border-gray-800 px-4 py-3">
        <h1 className="text-xl font-bold text-gray-900 dark:text-white">Setup</h1>
        <p className="mt-1 text-sm text-gray-500 dark:text-gray-400">
          Save one merged settings file with archive options, list IDs, and X session cookies.
        </p>
      </div>

      <form onSubmit={handleSave} className="space-y-6 p-4">
        <section className="space-y-3 rounded-2xl border border-gray-200 bg-gray-50 p-4 dark:border-gray-800 dark:bg-gray-900">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-gray-500">Core settings</h2>
          <label className="block space-y-2">
            <span className="text-sm text-gray-700 dark:text-gray-300">Archive path</span>
            <input
              value={settings.archive_path}
              onChange={(e) => setSettings((prev) => ({ ...prev, archive_path: e.target.value }))}
              className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
            />
          </label>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-700 dark:text-gray-300">Lists</span>
              <button
                type="button"
                onClick={addList}
                className="rounded-full border border-gray-300 px-3 py-1 text-sm hover:bg-gray-100 dark:border-gray-700 dark:hover:bg-gray-800"
              >
                Add list
              </button>
            </div>
            {lists.map((list, index) => (
              <div key={index} className="space-y-3 rounded-xl border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-black">
                <div className="flex items-center justify-between">
                  <div className="text-sm font-medium text-gray-700 dark:text-gray-300">List {index + 1}</div>
                  {lists.length > 1 && (
                    <button type="button" onClick={() => removeList(index)} className="text-sm text-red-500 hover:underline">
                      Remove
                    </button>
                  )}
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <input
                    placeholder="Numeric list ID"
                    value={list.id}
                    onChange={(e) => setList(index, { ...list, id: e.target.value })}
                    className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
                  />
                  <input
                    placeholder="Optional slug"
                    value={list.slug}
                    onChange={(e) => setList(index, { ...list, slug: e.target.value })}
                    className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black"
                  />
                </div>
              </div>
            ))}
          </div>
          <div className="grid grid-cols-2 gap-3">
            <label className="block space-y-2">
              <span className="text-sm text-gray-700 dark:text-gray-300">Poll minutes</span>
              <input type="number" min="1" value={settings.poll_mins} onChange={(e) => setSettings((prev) => ({ ...prev, poll_mins: Number(e.target.value) || 1 }))} className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black" />
            </label>
            <label className="block space-y-2">
              <span className="text-sm text-gray-700 dark:text-gray-300">Max pages</span>
              <input type="number" min="1" value={settings.max_pages} onChange={(e) => setSettings((prev) => ({ ...prev, max_pages: Number(e.target.value) || 1 }))} className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black" />
            </label>
          </div>
        </section>

        <section className="space-y-3 rounded-2xl border border-gray-200 bg-gray-50 p-4 dark:border-gray-800 dark:bg-gray-900">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold uppercase tracking-wide text-gray-500">X sessions</h2>
            <button type="button" onClick={addSession} className="rounded-full border border-gray-300 px-3 py-1 text-sm hover:bg-gray-100 dark:border-gray-700 dark:hover:bg-gray-800">
              Add session
            </button>
          </div>
          {settings.sessions.map((session, index) => (
            <div key={index} className="space-y-3 rounded-xl border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-black">
              <div className="flex items-center justify-between">
                <div className="text-sm font-medium text-gray-700 dark:text-gray-300">Session {index + 1}</div>
                {settings.sessions.length > 1 && (
                  <button type="button" onClick={() => removeSession(index)} className="text-sm text-red-500 hover:underline">
                    Remove
                  </button>
                )}
              </div>
              <div className="grid grid-cols-2 gap-3">
                <input placeholder="Username" value={session.username} onChange={(e) => setSession(index, { ...session, username: e.target.value })} className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black" />
                <input placeholder="Optional numeric ID" value={session.id} onChange={(e) => setSession(index, { ...session, id: e.target.value })} className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black" />
              </div>
              <input placeholder="auth_token" value={session.auth_token} onChange={(e) => setSession(index, { ...session, auth_token: e.target.value })} className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black" />
              <input placeholder="ct0" value={session.ct0} onChange={(e) => setSession(index, { ...session, ct0: e.target.value })} className="w-full rounded-xl border border-gray-200 bg-white px-3 py-2 text-sm outline-none focus:border-blue-500 dark:border-gray-700 dark:bg-black" />
            </div>
          ))}
        </section>

        {error ? <div className="rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">{error}</div> : null}
        {saved ? <div className="rounded-xl border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-700">{saved}</div> : null}

        <button
          type="submit"
          disabled={saving}
          className="w-full rounded-full bg-blue-500 px-4 py-3 text-sm font-semibold text-white transition-colors hover:bg-blue-600 disabled:opacity-50"
        >
          {saving ? 'Saving…' : 'Save settings'}
        </button>
      </form>
    </div>
  );
}
