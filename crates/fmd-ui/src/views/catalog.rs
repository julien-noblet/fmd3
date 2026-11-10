use leptos::prelude::*;

#[component]
pub fn CatalogView() -> impl IntoView {
    view! {
        <section id="tab-catalog" class="tab-content hidden">
            <div class="bg-zinc-900 border border-zinc-800 rounded-2xl overflow-hidden shadow-xl mb-6">
                // Top Toolbar: Module Selector, Sync Button, Status
                <div class="p-5 border-b border-zinc-800 flex flex-col md:flex-row justify-between items-start md:items-center gap-4 bg-zinc-900/90">
                    <div>
                        <h2 class="text-lg font-bold text-white flex items-center gap-2">
                            <span>"📖"</span> <span data-i18n="catalog.title">"Catalogue des Mangas & Recherche"</span>
                        </h2>
                        <p class="text-xs text-zinc-400" data-i18n="catalog.subtitle">"Consultez et recherchez parmi les mangas disponibles par module ou dans tout le catalogue indexé."</p>
                    </div>

                    <div class="flex flex-wrap items-center gap-3 w-full md:w-auto">
                        <select id="catalog-module-select" onchange="onCatalogModuleChanged()" class="bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-red-500">
                            <option value="all" data-i18n="catalog.all_modules_synced">"🔍 Tous les modules synchronisés"</option>
                        </select>

                        <button id="btn-sync-catalog" onclick="toggleCatalogSync()" class="px-4 py-2 bg-red-600 hover:bg-red-700 text-white font-medium rounded-lg text-sm transition flex items-center gap-2 shadow-lg shadow-red-600/20">
                            <span id="sync-icon">"🔄"</span>
                            <span id="sync-label" data-i18n="catalog.sync_all">"Synchroniser le catalogue"</span>
                        </button>
                    </div>
                </div>

                // Sync Progress Bar Banner (when syncing)
                <div id="catalog-sync-banner" class="hidden p-4 bg-zinc-950/80 border-b border-zinc-800 text-xs">
                    <div class="flex items-center justify-between mb-2">
                        <span class="font-semibold text-amber-400 flex items-center gap-2">
                            <span class="w-2 h-2 rounded-full bg-amber-400 animate-ping"></span>
                            <span id="sync-banner-text" data-i18n="catalog.syncing">"Synchronisation en cours..."</span>
                        </span>
                        <span id="sync-banner-stats" class="text-zinc-400 font-mono">"Page 1 • 0 mangas"</span>
                    </div>
                    <div class="w-full bg-zinc-800 rounded-full h-2 overflow-hidden">
                        <div id="sync-banner-bar" class="bg-amber-500 h-2 rounded-full transition-all duration-300" style="width: 10%"></div>
                    </div>
                </div>

                // Search & Filter Bar
                <div class="p-4 bg-zinc-950/40 border-b border-zinc-800/80 flex flex-col sm:flex-row items-center justify-between gap-3">
                    <div class="relative w-full sm:w-96">
                        <input
                            type="text"
                            id="catalog-search-input"
                            oninput="debounceCatalogSearch()"
                            data-i18n-placeholder="catalog.search_placeholder"
                            placeholder="Rechercher par titre (ex: Jujutsu, Piece, Solo...)"
                            class="w-full bg-zinc-950 border border-zinc-700 rounded-lg pl-9 pr-4 py-2 text-sm text-white placeholder-zinc-500 focus:outline-none focus:border-red-500 focus:ring-1 focus:ring-red-500"
                        />
                        <span class="absolute left-3 top-2.5 text-zinc-500">"🔍"</span>
                    </div>

                    <div class="flex items-center gap-3 text-xs text-zinc-400">
                        <span id="catalog-results-count">"0 manga(s)"</span>
                        <div class="flex items-center gap-1">
                            <button onclick="changeCatalogPage(-1)" id="btn-cat-prev" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed rounded text-white font-medium">"←"</button>
                            <span id="catalog-page-indicator" class="px-2 font-mono">"1 / 1"</span>
                            <button onclick="changeCatalogPage(1)" id="btn-cat-next" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 disabled:opacity-40 disabled:cursor-not-allowed rounded text-white font-medium">"→"</button>
                        </div>
                    </div>
                </div>

                // Catalog Items Table / Grid
                <div class="overflow-x-auto max-h-[600px]">
                    <table class="w-full text-left text-sm text-zinc-300">
                        <thead class="text-xs uppercase bg-zinc-950/80 text-zinc-400 border-b border-zinc-800 sticky top-0 backdrop-blur z-10">
                            <tr>
                                <th class="px-5 py-3 font-semibold" data-i18n="catalog.col_title">"Titre du Manga"</th>
                                <th class="px-5 py-3 font-semibold" data-i18n="catalog.col_module">"Module"</th>
                                <th class="px-5 py-3 font-semibold" data-i18n="catalog.col_updated">"Dernière mise à jour"</th>
                                <th class="px-5 py-3 font-semibold text-right" data-i18n="catalog.col_action">"Action"</th>
                            </tr>
                        </thead>
                        <tbody id="catalog-table-body" class="divide-y divide-zinc-800/60 text-xs">
                            <tr>
                                <td colspan="4" class="px-5 py-12 text-center text-zinc-500" data-i18n="catalog.empty_hint_prompt">
                                    "Select a module and click 'Synchronize' or search."
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </section>
    }
}
