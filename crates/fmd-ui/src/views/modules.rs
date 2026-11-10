use leptos::prelude::*;

#[component]
pub fn ModulesView() -> impl IntoView {
    view! {
        <section id="tab-modules" class="tab-content hidden">
            <div class="bg-zinc-900 border border-zinc-800 rounded-2xl overflow-hidden shadow-xl">
                <div class="p-5 border-b border-zinc-800 flex flex-col sm:flex-row justify-between items-center gap-4">
                    <div>
                        <h2 class="text-lg font-bold text-white" data-i18n="modules.title">"Modules de Sites Manga"</h2>
                        <p class="text-xs text-zinc-400" data-i18n="modules.subtitle">"Cliquez sur un module pour rechercher ou ajouter un manga directement."</p>
                    </div>
                    <div class="w-full sm:w-72">
                        <input
                            type="text"
                            id="module-search"
                            oninput="filterModules()"
                            data-i18n-placeholder="modules.search_placeholder"
                            placeholder="Rechercher un module ou URL..."
                            class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-sm text-white placeholder-zinc-500 focus:outline-none focus:border-red-500 focus:ring-1 focus:ring-red-500"
                        />
                    </div>
                </div>

                <div class="overflow-x-auto max-h-[550px]">
                    <table class="w-full text-left text-sm text-zinc-300">
                        <thead class="text-xs uppercase bg-zinc-950/60 text-zinc-400 border-b border-zinc-800 sticky top-0 backdrop-blur">
                            <tr>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="catalog.col_title">"Nom"</th>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="modules.col_category">"Catégorie"</th>
                                <th class="px-5 py-3.5 font-semibold" data-i18n="search.manga_url">"URL Racine"</th>
                                <th class="px-5 py-3.5 font-semibold text-right" data-i18n="catalog.col_action">"Action"</th>
                            </tr>
                        </thead>
                        <tbody id="modules-list" class="divide-y divide-zinc-800/60 font-mono text-xs">
                            <tr>
                                <td colspan="4" class="px-5 py-8 text-center text-zinc-500" data-i18n="common.loading">"Chargement des modules..."</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </section>
    }
}
