use leptos::prelude::*;

#[component]
pub fn MergerView() -> impl IntoView {
    view! {
        <section id="tab-merger" class="tab-content hidden">
            <div class="mb-6">
                <div class="flex items-center space-x-2">
                    <h2 class="text-xl font-bold tracking-tight" data-i18n="merger.title">"Fusion Multi-Sources & Détection de Lacunes"</h2>
                    <span class="px-2 py-0.5 rounded text-[10px] font-bold bg-amber-950 text-amber-400 border border-amber-800" data-i18n="merger.tag_smart_hybrid">
                        "Smart Hybrid"
                    </span>
                </div>
                <p class="text-xs text-zinc-400" data-i18n="merger.subtitle">
                    "Combine official quality (e.g. MangaPlus) with comprehensive scan archives to fill missing chapters automatically."
                </p>
            </div>

            // Configuration Form
            <div class="bg-zinc-900 p-6 rounded-xl border border-zinc-800 mb-6 space-y-4">
                <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <div>
                        <label class="block text-xs font-semibold text-zinc-400 mb-1" data-i18n="merger.canonical_title">"Titre canonique du Manga"</label>
                        <input
                            type="text"
                            id="merger-title-input"
                            data-i18n-placeholder="merger.canonical_placeholder"
                            placeholder="ex: Jujutsu Kaisen, One Piece, Chainsaw Man"
                            class="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm focus:outline-none focus:border-red-600 font-medium"
                        />
                    </div>
                    <div class="flex items-end">
                        <button onclick="searchMergerAlternatives()" id="btn-merger-search" class="w-full px-6 py-2.5 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-sm font-semibold transition border border-zinc-700 flex items-center justify-center gap-2">
                            <span>"🔍"</span> <span data-i18n="merger.btn_search_sources">"Rechercher les sources disponibles"</span>
                        </button>
                    </div>
                </div>

                // Sources selection boxes
                <div id="merger-sources-box" class="hidden space-y-4 pt-4 border-t border-zinc-800">
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div>
                            <label class="block text-xs font-semibold text-emerald-400 mb-1" data-i18n="merger.label_primary">
                                "Primary Source (Quality / Official priority)"
                            </label>
                            <select id="merger-primary-select" class="w-full bg-zinc-950 border border-emerald-900/50 rounded-lg px-3 py-2 text-sm focus:outline-none">
                            </select>
                        </div>
                        <div>
                            <label class="block text-xs font-semibold text-amber-400 mb-1" data-i18n="merger.label_secondary">
                                "Secondary Sources (Archive gap filler)"
                            </label>
                            <div id="merger-secondary-checkboxes" class="space-y-1.5 max-h-32 overflow-y-auto p-2 bg-zinc-950 rounded-lg border border-zinc-800 text-xs">
                            </div>
                        </div>
                    </div>

                    <div class="flex justify-end pt-2">
                        <button onclick="executeMergeAnalysis()" id="btn-execute-merge" class="px-6 py-2.5 rounded-lg bg-red-600 hover:bg-red-500 text-sm font-bold shadow-lg shadow-red-600/20 transition flex items-center gap-2">
                            <span>"⚡"</span> <span data-i18n="merger.btn_analyze">"Analyser & Calculer la Fusion"</span>
                        </button>
                    </div>
                </div>
            </div>

            // Merger Results Container
            <div id="merger-results" class="hidden space-y-6">
                // Gaps analysis banner
                <div id="merger-gaps-banner" class="p-4 rounded-xl border flex items-center justify-between">
                </div>

                // Merged Chapters Table
                <div class="bg-zinc-900 rounded-xl border border-zinc-800 overflow-hidden">
                    <div class="p-4 border-b border-zinc-800 flex items-center justify-between">
                        <div class="flex items-center space-x-3">
                            <h3 class="font-bold text-sm" data-i18n="merger.plan_title">"Plan d'Exécution Fusionné"</h3>
                            <span id="merger-summary-badge" class="px-2 py-0.5 rounded text-xs bg-zinc-800 text-zinc-300"></span>
                        </div>
                        <div class="flex items-center space-x-3">
                            <select id="merger-format-select" class="bg-zinc-950 border border-zinc-800 rounded px-2.5 py-1 text-xs">
                                <option value="Cbz" data-i18n="settings.format_cbz">"Format CBZ"</option>
                                <option value="Epub" data-i18n="settings.format_epub">"Format EPUB"</option>
                                <option value="Pdf" data-i18n="settings.format_pdf">"Format PDF"</option>
                            </select>
                            <button onclick="queueAllMergedChapters()" id="btn-queue-merged" class="px-4 py-1.5 rounded-lg bg-red-600 hover:bg-red-500 text-xs font-bold transition" data-i18n="merger.btn_download_plan">
                                "Download Hybrid Queue"
                            </button>
                        </div>
                    </div>

                    <div class="overflow-x-auto">
                        <table class="w-full text-left text-xs">
                            <thead class="bg-zinc-950/50 text-zinc-400 border-b border-zinc-800">
                                <tr>
                                    <th class="p-3 w-12 text-center" data-i18n="merger.col_num">"#"</th>
                                    <th class="p-3" data-i18n="merger.col_chapter">"Chapitre"</th>
                                    <th class="p-3" data-i18n="merger.col_source">"Source Sélectionnée"</th>
                                    <th class="p-3" data-i18n="merger.col_type">"Type"</th>
                                    <th class="p-3 text-right" data-i18n="merger.col_alternatives">"Alternatives"</th>
                                </tr>
                            </thead>
                            <tbody id="merger-chapters-tbody" class="divide-y divide-zinc-800/60 font-mono">
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </section>
    }
}
