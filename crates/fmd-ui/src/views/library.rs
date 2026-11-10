use leptos::prelude::*;

#[component]
pub fn LibraryView() -> impl IntoView {
    view! {
        <section id="tab-favorites" class="tab-content block">
            <div class="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 mb-6">
                <div>
                    <h2 class="text-xl font-bold text-white flex items-center gap-2">
                        <span>"⭐"</span> <span data-i18n="favorites.title">"Bibliothèque de Suivi & Nouveautés"</span>
                    </h2>
                    <p class="text-sm text-zinc-400" data-i18n="favorites.subtitle">"Suivi des nouveaux chapitres, téléchargement automatique et gestion des favoris."</p>
                </div>
                <div class="flex items-center gap-3">
                    <button onclick="checkAllFavorites()" id="btn-check-all-favs" class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-white rounded-lg text-sm font-semibold transition border border-zinc-700 flex items-center gap-2 shadow-sm">
                        <span>"🔄"</span> <span data-i18n="favorites.check_all">"Vérifier tous les favoris"</span>
                    </button>
                    <button onclick="switchTab('search')" class="px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-lg text-sm font-semibold transition shadow-lg shadow-red-600/20 flex items-center gap-2">
                        <span>"+"</span> <span data-i18n="nav.search">"Ajouter / Suivre un manga"</span>
                    </button>
                </div>
            </div>

            <div id="favorites-list" class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
                <div class="col-span-full text-center py-16 text-zinc-500 bg-zinc-900/50 border border-zinc-800 rounded-2xl">
                    <p class="text-base mb-2" data-i18n="common.loading">"Chargement des favoris..."</p>
                </div>
            </div>
        </section>
    }
}
