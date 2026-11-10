use leptos::prelude::*;

#[component]
pub fn DownloadsView() -> impl IntoView {
    view! {
        <section id="tab-downloads" class="tab-content hidden">
            <div class="bg-zinc-900 border border-zinc-800 rounded-2xl overflow-hidden shadow-xl">
                <div class="p-5 border-b border-zinc-800 flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4">
                    <div>
                        <h2 class="text-lg font-bold text-white flex items-center gap-2">
                            <span>"📥"</span> <span data-i18n="downloads.title">"File de Téléchargement & Progression"</span>
                        </h2>
                        <p class="text-xs text-zinc-400" data-i18n="downloads.subtitle">"Suivi en direct de l'extraction des pages, du téléchargement et du packaging (CBZ/PDF/EPUB)."</p>
                    </div>
                    <div class="flex items-center gap-2">
                        <button onclick="retryAllFailedDownloads()" id="btn-retry-failed" class="px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 text-white rounded-lg text-xs font-medium transition flex items-center gap-1.5">
                            <span>"🔄"</span> <span data-i18n="downloads.retry_failed">"Relancer les échecs"</span>
                        </button>
                        <button onclick="loadDownloads()" class="px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 text-white rounded-lg text-xs font-medium transition flex items-center gap-1.5">
                            <span>"🔄"</span> <span data-i18n="downloads.refresh">"Actualiser"</span>
                        </button>
                    </div>
                </div>

                <div class="overflow-x-auto">
                    <table class="w-full text-left text-sm text-zinc-300">
                        <thead class="text-xs uppercase bg-zinc-950/60 text-zinc-400 border-b border-zinc-800 sticky top-0 backdrop-blur">
                            <tr>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="downloads.col_manga">"Manga & Chapitre"</th>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="search.format_label">"Format"</th>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="downloads.col_progress">"Progression"</th>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="downloads.col_status">"Statut"</th>
                                <th class="px-5 py-3.5 font-semibold text-right" data-i18n="common.actions">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody id="downloads-table-body" class="divide-y divide-zinc-800/60 text-xs">
                            <tr>
                                <td colspan="5" class="px-5 py-8 text-center text-zinc-500" data-i18n="downloads.empty_queue">"Aucun téléchargement dans la file. Utilisez l'onglet « + Ajouter par URL » pour choisir des chapitres."</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </section>
    }
}
