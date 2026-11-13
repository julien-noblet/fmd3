use leptos::prelude::*;

#[component]
pub fn Navbar() -> impl IntoView {
    let base_path = use_context::<String>().unwrap_or_default();
    let logo_url = format!("{base_path}/logo.png");
    view! {
        <header class="bg-zinc-900 border-b border-zinc-800 sticky top-0 z-50">
            <div class="max-w-7xl mx-auto px-4 h-16 flex items-center justify-between gap-4">
                <div class="flex items-center space-x-3 cursor-pointer shrink-0" onclick="switchTab('favorites')">
                    <img src=logo_url alt="FMD3 San" class="w-10 h-10 rounded-xl object-contain shadow-lg shadow-red-600/20 border border-zinc-800" />
                    <div class="whitespace-nowrap">
                        <h1 class="font-bold text-lg leading-none tracking-wide text-white">
                            "FMD3"
                        </h1>
                        <p class="text-[11px] text-zinc-400 font-medium mt-0.5">"Free Manga Downloader San (三)"</p>
                    </div>
                </div>

                // Navigation Tabs
                <nav class="flex space-x-1" id="nav-tabs">
                    <button onclick="switchTab('favorites')" id="tab-btn-favorites" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition bg-zinc-800 text-white">
                        <span data-i18n="nav.favorites">"Bibliothèque & Favoris"</span>
                    </button>
                    <button onclick="switchTab('catalog')" id="tab-btn-catalog" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50 flex items-center gap-2">
                        <span>"📖"</span> <span data-i18n="nav.catalog">"Catalogue & Recherche"</span>
                        <span id="badge-catalog-sync" class="hidden px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-amber-950 text-amber-400 border border-amber-800 animate-pulse" data-i18n="nav.sync_badge">
                            "Sync"
                        </span>
                    </button>
                    <button onclick="switchTab('search')" id="tab-btn-search" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50">
                        <span data-i18n="nav.search">"+ Ajouter par URL"</span>
                    </button>
                    <button onclick="switchTab('merger')" id="tab-btn-merger" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50 flex items-center gap-2">
                        <span>"⚡"</span> <span data-i18n="nav.merger">"Fusion Multi-Sources"</span>
                    </button>
                    <button onclick="switchTab('downloads')" id="tab-btn-downloads" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50 flex items-center gap-2">
                        <span data-i18n="nav.downloads">"Téléchargements"</span>
                        <span id="badge-dl-count" class="px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-zinc-800 text-zinc-400">
                            "0"
                        </span>
                    </button>
                    <button onclick="switchTab('modules')" id="tab-btn-modules" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50">
                        <span data-i18n="nav.modules">"Modules"</span>
                    </button>
                    <button onclick="switchTab('settings')" id="tab-btn-settings" class="tab-btn px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50 flex items-center gap-1.5">
                        <span>"⚙️"</span> <span data-i18n="nav.settings">"Réglages"</span>
                    </button>
                    <button onclick="switchTab('users')" id="tab-btn-users" class="tab-btn hidden px-4 py-2 rounded-lg text-sm font-medium transition text-zinc-400 hover:text-white hover:bg-zinc-800/50 flex items-center gap-1.5">
                        <span>"👥"</span> <span data-i18n="nav.users">"Utilisateurs"</span>
                    </button>
                </nav>

                // Status Indicator, Language Picker & User Pill
                <div class="flex items-center space-x-3 shrink-0">
                    <select id="nav-lang-select" onchange="setLocale(this.value)" class="bg-zinc-800 hover:bg-zinc-700/80 border border-zinc-700 text-xs text-white rounded-lg px-2.5 py-1.5 shrink-0 cursor-pointer focus:outline-none focus:border-red-500 font-medium transition" title="Language / Langue" data-i18n-title="settings.language">
                    </select>

                    <span id="ws-badge" class="inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-950 text-emerald-400 border border-emerald-800">
                        <span class="w-1.5 h-1.5 mr-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
                        <span id="ws-badge-text" data-i18n="nav.ws_connected">"Connecté (WS)"</span>
                    </span>

                    <div id="user-auth-pill" class="flex items-center gap-2">
                        <div id="user-info-badge" class="flex items-center gap-2 bg-zinc-800/80 border border-zinc-700 px-2.5 py-1 rounded-lg text-xs">
                            <span id="nav-username" class="font-bold text-white">""</span>
                            <span id="nav-role-badge" class="px-1.5 py-0.5 rounded text-[10px] font-mono font-bold uppercase bg-red-950 text-red-400 border border-red-800">""</span>
                            <button type="button" onclick="logout()" class="p-1 rounded text-zinc-400 hover:text-red-400 hover:bg-zinc-700/60 transition ml-1 cursor-pointer flex items-center justify-center text-sm" title="Se déconnecter" aria-label="Se déconnecter" data-i18n-title="nav.logout">
                                "🚪"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </header>
    }
}
