use leptos::prelude::*;

#[component]
pub fn SearchView() -> impl IntoView {
    view! {
        <section id="tab-search" class="tab-content hidden">
            <div class="max-w-4xl mx-auto bg-zinc-900 border border-zinc-800 rounded-2xl p-6 sm:p-8 shadow-xl">
                <h2 class="text-xl font-bold text-white mb-1" data-i18n="search.title">"Rechercher et Télécharger des Chapitres"</h2>
                <p class="text-sm text-zinc-400 mb-6" data-i18n="search.subtitle">"Sélectionnez le module du site source et saisissez l'URL de la série manga pour choisir les chapitres à télécharger."</p>

                <div class="space-y-5">
                    <div>
                        <label class="block text-xs font-semibold uppercase tracking-wider text-zinc-400 mb-2" data-i18n="search.select_module">"1. Module de site source"</label>
                        <select id="fetch-module-select" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-4 py-2.5 text-sm text-white focus:outline-none focus:border-red-500">
                            <option value="" data-i18n="search.select_module_placeholder">"Sélectionnez un module parmi les 428 disponibles..."</option>
                        </select>
                    </div>

                    <div>
                        <label class="block text-xs font-semibold uppercase tracking-wider text-zinc-400 mb-2" data-i18n="search.manga_url">"2. URL directe du manga sur le site"</label>
                        <input
                            type="url"
                            id="fetch-manga-url"
                            data-i18n-placeholder="search.url_placeholder"
                            placeholder="Exemple : https://fr.novelcool.com/novel/Jujutsu-Kaisen.html"
                            class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-4 py-2.5 text-sm text-white placeholder-zinc-500 focus:outline-none focus:border-red-500"
                        />
                    </div>

                    <div class="pt-2">
                        <button onclick="fetchMangaDetails()" id="btn-inspect-manga" class="w-full py-3 bg-red-600 hover:bg-red-500 text-white font-semibold rounded-lg text-sm transition shadow-lg shadow-red-600/30 flex items-center justify-center gap-2">
                            <span>"🔍"</span> <span data-i18n="search.fetch_btn">"Charger les chapitres et infos du manga"</span>
                        </button>
                    </div>
                </div>

                // Inspect Result Box
                <div id="manga-preview" class="hidden mt-8 pt-8 border-t border-zinc-800">
                    // Manga Card Header
                    <div class="flex flex-col sm:flex-row gap-6 bg-zinc-950/70 border border-zinc-800 rounded-xl p-5 mb-6">
                        <img id="prev-cover" src="" alt="Cover" class="w-32 h-44 object-cover rounded-lg bg-zinc-800 shrink-0 shadow" />
                        <div class="flex-1 flex flex-col justify-between">
                            <div>
                                <div class="flex items-center gap-2 mb-1">
                                    <h3 id="prev-title" class="text-lg font-bold text-white"></h3>
                                    <span id="prev-status" class="px-2 py-0.5 rounded text-[11px] font-medium bg-emerald-950 text-emerald-400 border border-emerald-800"></span>
                                </div>
                                <p id="prev-authors" class="text-xs text-zinc-400 mb-2"></p>
                                <p id="prev-summary" class="text-xs text-zinc-300 line-clamp-2 mb-3"></p>
                                <div class="text-xs text-zinc-400 font-mono">
                                    <span data-i18n="search.chapters_detected">"Chapitres détectés : "</span>
                                    <span id="prev-chapters-count" class="font-bold text-white">"0"</span>
                                </div>
                            </div>

                            <div class="pt-3 border-t border-zinc-800/80 flex flex-wrap items-center justify-between gap-3">
                                <div class="flex items-center gap-2">
                                    <span class="text-xs text-zinc-500" data-i18n="search.save_to_library">"Enregistrer dans la bibliothèque"</span>
                                    <span id="manga-cache-badge" class="hidden px-2 py-0.5 rounded text-[10px] font-mono border"></span>
                                </div>
                                <div class="flex items-center gap-2">
                                    <button onclick="fetchMangaDetails(true)" id="btn-force-refresh" class="px-3 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white text-xs font-semibold rounded-lg transition border border-zinc-700 flex items-center gap-1.5" title="Bypass cache et re-scraper" data-i18n-title="search.btn_force_refresh_title">
                                        <span>"🔄"</span> <span data-i18n="search.btn_force_refresh">"Forcer l'actualisation"</span>
                                    </button>
                                    <button onclick="confirmAddFavorite()" id="btn-add-favorite" class="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-bold rounded-lg transition shadow-lg shadow-emerald-600/30">
                                        <span>"⭐ "</span><span data-i18n="search.btn_add_fav">"Ajouter à mes favoris / suivi"</span>
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>

                    // Chapter Download Selection Section
                    <div class="bg-zinc-950/70 border border-zinc-800 rounded-xl p-5">
                        <div class="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 mb-4 pb-4 border-b border-zinc-800">
                            <div>
                                <h4 class="font-bold text-white text-base flex items-center gap-2">
                                    <span>"📥"</span> <span data-i18n="search.dl_selection_title">"Sélection des Téléchargements"</span>
                                </h4>
                                <p class="text-xs text-zinc-400" data-i18n="search.dl_selection_subtitle">"Cochez les chapitres à télécharger, choisissez le format et lancez la file."</p>
                            </div>

                            <div class="flex flex-wrap items-center gap-3">
                                <div>
                                    <label class="text-[11px] uppercase tracking-wider text-zinc-400 mr-2 font-semibold" data-i18n="search.format_label">"Format :"</label>
                                    <select id="dl-format-select" class="bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-red-500">
                                        <option value="Cbz" selected=true data-i18n="settings.format_cbz">"CBZ (Comic Book / Liseuse)"</option>
                                        <option value="Pdf" data-i18n="settings.format_pdf">"PDF (Document)"</option>
                                        <option value="Epub" data-i18n="settings.format_epub">"EPUB (Livre électronique)"</option>
                                        <option value="Directory" data-i18n="settings.format_dir">"Dossier d'images brutes"</option>
                                    </select>
                                </div>

                                <button onclick="queueSelectedChapters()" id="btn-dl-queue" class="px-5 py-2.5 bg-red-600 hover:bg-red-500 text-white text-xs font-bold rounded-lg transition shadow-lg shadow-red-600/30 flex items-center gap-2">
                                    <span>"📥"</span> <span data-i18n="search.btn_download_selected">"Télécharger ("</span>
                                    <span id="selected-chap-count">"0"</span>
                                    <span>")"</span>
                                </button>
                            </div>
                        </div>

                        // Selection Quick Controls
                        <div class="flex flex-wrap items-center justify-between gap-3 text-xs mb-3 text-zinc-400">
                            <div class="flex items-center gap-2">
                                <button onclick="selectAllChapters(true)" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded" data-i18n="search.select_all">"Tout cocher"</button>
                                <button onclick="selectAllChapters(false)" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded" data-i18n="search.deselect_all">"Tout décocher"</button>
                                <button onclick="invertChapterSelection()" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded" data-i18n="search.invert_selection">"Inverser"</button>
                            </div>

                            <div class="flex items-center gap-2">
                                <span data-i18n="search.quick_select">"Sélection rapide :"</span>
                                <button onclick="selectLastNChapters(10)" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded" data-i18n="search.last_10">"10 derniers"</button>
                                <button onclick="selectLastNChapters(25)" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded" data-i18n="search.last_25">"25 derniers"</button>
                            </div>
                        </div>

                        // Scrollable Chapters List
                        <div class="max-h-80 overflow-y-auto border border-zinc-800/80 rounded-lg bg-zinc-900/60 divide-y divide-zinc-800/60 text-xs">
                            <div id="chapters-list-container">
                                // Populated dynamically
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </section>
    }
}
