import React from 'react';

const NAV_ITEMS = [
  { id: 'library', label: 'Library', icon: 'library_music' },
  { id: 'playlists', label: 'Playlists', icon: 'playlist_play' },
  { id: 'now-playing', label: 'Now Playing', icon: 'graphic_eq' },
];

export const Sidebar = ({ view, onViewChange, onCreatePlaylist, status }) => (
  <nav className="hidden md:flex flex-col bg-[#0e0e0e] w-[280px] h-screen border-r border-white/10 shadow-2xl shadow-violet-500/5 fixed left-0 top-0 pt-8 pb-32 z-40">
    <div className="px-6 mb-8 flex items-center gap-3">
      <button 
        type="button" 
        onClick={() => onViewChange('library')}
        className="text-2xl font-black italic tracking-tighter text-[#8B5CF6] hover:opacity-80 transition-opacity"
      >
        SpotiCry
      </button>
    </div>
    <div className="flex flex-col gap-2 px-2 mt-4 flex-grow">
      {NAV_ITEMS.map((item) => {
        const active = view === item.id;
        return (
          <button
            key={item.id}
            type="button"
            onClick={() => onViewChange(item.id)}
            className={`flex items-center gap-4 py-3 rounded-lg pl-4 transition-all duration-200 text-left ${
              active
                ? 'text-white font-bold border-l-2 border-violet-500 bg-gradient-to-r from-violet-500/10 to-transparent'
                : 'text-neutral-400 hover:text-white hover:bg-white/5 border-l-2 border-transparent'
            }`}
          >
            <span className={`material-symbols-outlined text-[24px] ${active ? 'fill' : ''}`}>{item.icon}</span>
            <span className="text-base">{item.label}</span>
          </button>
        );
      })}
    </div>
    <div className="px-6 mt-auto flex flex-col gap-3">
      <button
        type="button"
        onClick={onCreatePlaylist}
        className="w-full bg-surface-container-high hover:bg-surface-bright text-on-surface text-xs font-semibold uppercase tracking-wider py-3 rounded-full flex items-center justify-center gap-2 border border-outline-variant transition-all hover:border-outline"
      >
        <span className="material-symbols-outlined text-[18px]">add</span>
        Create Playlist
      </button>
    </div>
  </nav>
);
