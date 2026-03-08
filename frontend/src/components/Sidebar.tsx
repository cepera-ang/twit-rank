import { useEffect, useMemo, useState } from 'react';
import { fetchBuildInfo } from '../api';
import type { BuildInfo } from '../types';

interface NavItemProps {
  icon: React.ReactNode;
  label: string;
  active?: boolean;
  onClick?: () => void;
}

function NavItem({ icon, label, active, onClick }: NavItemProps) {
  return (
    <button
      onClick={onClick}
      className={`flex items-center gap-4 px-4 py-3 rounded-full hover:bg-gray-100 dark:hover:bg-gray-900 transition-colors w-full text-left ${
        active ? 'font-bold' : ''
      }`}
    >
      <span className="w-7 h-7">{icon}</span>
      <span className="text-xl text-gray-900 dark:text-white hidden xl:inline">{label}</span>
    </button>
  );
}

interface SidebarProps {
  mode: 'home' | 'search' | 'setup';
  onModeChange: (mode: 'home' | 'search' | 'setup') => void;
  onOpenTetris: () => void;
}

export function Sidebar({ mode, onModeChange, onOpenTetris }: SidebarProps) {
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);
  const clientBuildId = useMemo(() => import.meta.env.VITE_TWIT_RANK_BUILD_ID ?? 'dev', []);
  const shortClientBuild =
    clientBuildId.length > 20 ? `${clientBuildId.slice(0, 20)}…` : clientBuildId;
  const shortServerBuild =
    !buildInfo
      ? 'loading…'
      : buildInfo.build_id.length > 20
        ? `${buildInfo.build_id.slice(0, 20)}…`
        : buildInfo.build_id;
  const isBuildMismatch = buildInfo != null && buildInfo.build_id !== clientBuildId;

  useEffect(() => {
    fetchBuildInfo()
      .then(setBuildInfo)
      .catch((err) => console.error('Failed to fetch build info:', err));
  }, []);

  return (
    <div className="sticky top-0 h-screen flex flex-col py-2 px-2 w-20 xl:w-64">
      <div className="px-4 py-3">
        <div className="w-8 h-8 rounded-full bg-gradient-to-br from-blue-400 to-purple-600 flex items-center justify-center">
          <span className="text-white font-bold text-sm">TR</span>
        </div>
      </div>

      <nav className="flex-1 mt-2">
        <NavItem
          active={mode === 'home'}
          onClick={() => onModeChange('home')}
          icon={
            <svg fill="currentColor" viewBox="0 0 24 24" className="w-7 h-7">
              <path d="M21.591 7.146L12.52 1.157c-.316-.21-.724-.21-1.04 0l-9.071 5.99c-.26.173-.409.456-.409.757v13.183c0 .502.418.913.929.913H9.14c.51 0 .929-.41.929-.913v-7.075h3.909v7.075c0 .502.417.913.928.913h6.165c.511 0 .929-.41.929-.913V7.904c0-.301-.158-.584-.409-.758z" />
            </svg>
          }
          label="Home"
        />
        <NavItem
          active={mode === 'search'}
          onClick={() => onModeChange('search')}
          icon={
            <svg fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24" className="w-7 h-7">
              <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
            </svg>
          }
          label="Search"
        />
        <NavItem
          active={mode === 'setup'}
          onClick={() => onModeChange('setup')}
          icon={
            <svg fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24" className="w-7 h-7">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12a7.5 7.5 0 0113.27-4.77m1.23-1.73v5h-5m5.5 1.5a7.5 7.5 0 01-13.27 4.77M5 18.5v-5h5" />
            </svg>
          }
          label="Setup"
        />
        <NavItem
          onClick={onOpenTetris}
          icon={
            <svg fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24" className="w-7 h-7">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4 6h6v6H4zM14 6h6v6h-6zM9 12h6v6H9zM4 18h6v2H4zM14 18h6v2h-6z" />
            </svg>
          }
          label="Tetris"
        />
      </nav>

      <div
        className={`hidden xl:block px-4 pt-4 text-[11px] font-mono ${
          isBuildMismatch ? 'text-orange-600 dark:text-orange-400' : 'text-gray-500'
        }`}
      >
        <div>client {shortClientBuild}</div>
        <div>server {shortServerBuild}</div>
      </div>
    </div>
  );
}
