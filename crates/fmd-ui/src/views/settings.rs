use leptos::prelude::*;

#[component]
pub fn SettingsView() -> impl IntoView {
    view! {
        <section id="tab-settings" class="tab-content hidden">
            <div class="max-w-4xl mx-auto space-y-6">
                <div class="flex items-center justify-between">
                    <div>
                        <h2 class="text-xl font-bold text-white flex items-center gap-2">
                            <span>"⚙️"</span> <span data-i18n="settings.title">"Réglages & Configuration"</span>
                        </h2>
                        <p class="text-sm text-zinc-400" data-i18n="settings.subtitle">"Dossiers de téléchargement, formats de sortie, métadonnées ComicInfo.xml et automatisation."</p>
                    </div>
                    <button onclick="saveSettings()" id="btn-save-settings" class="px-5 py-2.5 bg-red-600 hover:bg-red-500 text-white rounded-lg text-sm font-semibold transition shadow-lg shadow-red-600/20 flex items-center gap-2">
                        <span>"💾"</span> <span data-i18n="settings.save_btn">"Enregistrer les réglages"</span>
                    </button>
                </div>

                <div id="settings-alert" class="hidden p-4 rounded-xl bg-emerald-950/80 border border-emerald-800 text-emerald-400 text-sm font-medium flex items-center gap-2">
                    <span>"✓"</span> <span data-i18n="settings.saved_alert">"Réglages enregistrés et appliqués à chaud avec succès !"</span>
                </div>

                // Card: Langue de l'interface / Interface Language
                <div class="bg-zinc-900 border border-zinc-800 rounded-2xl p-6 shadow-xl space-y-4">
                    <div class="flex items-center justify-between border-b border-zinc-800 pb-3">
                        <h3 class="text-base font-bold text-white flex items-center gap-2">
                            <span>"🌐"</span> <span data-i18n="settings.language">"Langue de l'interface"</span>
                        </h3>
                        <span class="text-xs text-zinc-400 font-mono uppercase bg-zinc-800 px-2 py-0.5 rounded border border-zinc-700" id="current-lang-code">"en"</span>
                    </div>
                    <p class="text-xs text-zinc-400" data-i18n="settings.language_desc">
                        "Choose your display language among the 25 supported languages. Changes take effect immediately."
                    </p>
                    <div class="max-w-md">
                        <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.language">"Langue"</label>
                        <select id="settings-lang-select" onchange="setLocale(this.value)" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-medium">
                        </select>
                    </div>
                </div>

                // Card 1: Dossiers et Format
                <div class="bg-zinc-900 border border-zinc-800 rounded-2xl p-6 shadow-xl space-y-5">
                    <h3 class="text-base font-bold text-white flex items-center gap-2 border-b border-zinc-800 pb-3">
                        <span>"📁"</span> <span data-i18n="settings.storage_format_title">"Dossier de Stockage & Format par défaut"</span>
                    </h3>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.download_dir_label">"Dossier de téléchargement"</label>
                            <input type="text" id="setting-download-dir" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-mono" />
                            <p class="text-[11px] text-zinc-500 mt-1.5" data-i18n="settings.download_dir_help">"Chemin relatif ou absolu sur le système hôte (ex: downloads ou /data/mangas)."</p>
                        </div>
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.default_format_label">"Format de compression par défaut"</label>
                            <select id="setting-default-format" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-mono">
                                <option value="CBZ" data-i18n="settings.format_cbz">"CBZ (Archive ZIP avec ComicInfo.xml - Recommandé)"</option>
                                <option value="PDF" data-i18n="settings.format_pdf">"PDF (Document PDF multipages)"</option>
                                <option value="EPUB" data-i18n="settings.format_epub">"EPUB (Livre numérique e-reader)"</option>
                                <option value="Directory" data-i18n="settings.format_dir">"Dossier brut (Fichiers images numérotés)"</option>
                            </select>
                            <p class="text-[11px] text-zinc-500 mt-1.5" data-i18n="settings.default_format_help">"Format appliqué par défaut lors de l'ajout automatique de nouveaux chapitres."</p>
                        </div>
                    </div>
                </div>

                // Card 2: Modèle de Nommage
                <div class="bg-zinc-900 border border-zinc-800 rounded-2xl p-6 shadow-xl space-y-5">
                    <h3 class="text-base font-bold text-white flex items-center gap-2 border-b border-zinc-800 pb-3">
                        <span>"🏷️"</span> <span data-i18n="settings.naming_pattern_title">"Modèle de Nommage des Fichiers"</span>
                    </h3>

                    <div>
                        <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.naming_pattern_label">"Structure des dossiers et noms de fichiers"</label>
                        <input type="text" id="setting-naming-template" oninput="updateNamingPreview()" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-mono" />

                        <div class="flex flex-wrap items-center gap-2 mt-3 text-xs">
                            <span class="text-zinc-500 text-[11px]" data-i18n="settings.naming_vars_available">"Variables disponibles :"</span>
                            <button type="button" onclick="insertNamingTag('{manga}')" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded font-mono text-[11px] transition">"{manga}"</button>
                            <button type="button" onclick="insertNamingTag('{chapter}')" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded font-mono text-[11px] transition">"{chapter}"</button>
                            <button type="button" onclick="insertNamingTag('{title}')" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded font-mono text-[11px] transition">"{title}"</button>
                            <button type="button" onclick="insertNamingTag('{ext}')" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded font-mono text-[11px] transition">"{ext}"</button>
                            <button type="button" onclick="insertNamingTag('{format}')" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded font-mono text-[11px] transition">"{format}"</button>
                        </div>

                        <div class="mt-4 p-3.5 bg-zinc-950/70 border border-zinc-800/80 rounded-lg text-xs">
                            <span class="text-zinc-500 text-[11px] block mb-1" data-i18n="settings.naming_preview_label">"Aperçu dynamique du chemin généré :"</span>
                            <div class="font-mono text-emerald-400 flex items-center gap-2">
                                <span>"↳"</span>
                                <span id="naming-preview">"downloads/One Piece/Chapitre 1050.cbz"</span>
                            </div>
                        </div>
                    </div>
                </div>

                // Card 3: Réseau & Automatisation
                <div class="bg-zinc-900 border border-zinc-800 rounded-2xl p-6 shadow-xl space-y-5">
                    <h3 class="text-base font-bold text-white flex items-center gap-2 border-b border-zinc-800 pb-3">
                        <span>"🌐"</span> <span data-i18n="settings.network_title">"Réseau, FlareSolverr & Automatisation"</span>
                    </h3>

                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.flaresolverr_label">"URL FlareSolverr (optionnel)"</label>
                            <input type="text" id="setting-flaresolverr-url" placeholder="http://localhost:8191/v1" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-mono" />
                            <p class="text-[11px] text-zinc-500 mt-1.5" data-i18n="settings.flaresolverr_help">"Pour le contournement des protections Cloudflare exigeantes."</p>
                        </div>
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.check_interval_label">"Vérification des favoris"</label>
                            <select id="setting-check-interval" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-mono">
                                <option value="1" data-i18n="settings.interval_1h">"Toutes les heures"</option>
                                <option value="3" data-i18n="settings.interval_3h">"Toutes les 3 heures"</option>
                                <option value="6" data-i18n="settings.interval_6h">"Toutes les 6 heures"</option>
                                <option value="12" data-i18n="settings.interval_12h">"Toutes les 12 heures"</option>
                                <option value="24" data-i18n="settings.interval_24h">"1 fois par jour"</option>
                            </select>
                            <p class="text-[11px] text-zinc-500 mt-1.5" data-i18n="settings.check_interval_help">"Intervalle d'analyse des nouveaux chapitres."</p>
                        </div>
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="settings.resync_interval_label">"Resynchronisation catalogue"</label>
                            <select id="setting-catalog-resync" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2.5 text-sm text-white focus:outline-none focus:border-red-500 font-mono">
                                <option value="7" data-i18n="settings.resync_7d">"Tous les 7 jours"</option>
                                <option value="14" data-i18n="settings.resync_14d">"Tous les 14 jours"</option>
                                <option value="30" data-i18n="settings.resync_30d">"Tous les 30 jours"</option>
                            </select>
                            <p class="text-[11px] text-zinc-500 mt-1.5" data-i18n="settings.resync_interval_help">"Fréquence de rafraîchissement des modules."</p>
                        </div>
                    </div>
                </div>

                // Card 4: Cache & Anti-ban Throttling
                <div id="cache-policy-settings" class="bg-zinc-900 border border-zinc-800 rounded-2xl p-6 shadow-xl space-y-5">
                    <div class="flex items-center justify-between border-b border-zinc-800 pb-3">
                        <h3 class="text-base font-bold text-white flex items-center gap-2">
                            <span>"🛡️"</span> <span data-i18n="settings.cache_title">"Cache Persistent & Délais Anti-Ban (Par Module)"</span>
                        </h3>
                    </div>
                    <p class="text-xs text-zinc-400" data-i18n="settings.cache_desc">
                        "Prevent IP bans by pacing website requests and caching metadata and chapters (minutes, hours, days, weeks)."
                    </p>

                    <div class="bg-zinc-950 border border-zinc-800 rounded-xl p-4 space-y-4">
                        <h4 class="text-xs font-bold text-zinc-300 uppercase tracking-wider" data-i18n="settings.cache_rule_title">"Ajouter ou modifier une règle"</h4>
                        <div class="grid grid-cols-1 sm:grid-cols-4 gap-4">
                            <div>
                                <label class="block text-[11px] font-semibold text-zinc-400 uppercase mb-1" data-i18n="settings.cache_target_module">"Module cible"</label>
                                <select id="cache-policy-module" class="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-white focus:outline-none focus:border-red-500 font-mono">
                                    <option value="*" data-i18n="settings.cache_module_global">"* Global (Par défaut)"</option>
                                </select>
                            </div>
                            <div>
                                <label class="block text-[11px] font-semibold text-zinc-400 uppercase mb-1" data-i18n="settings.cache_ttl_label">"Durée TTL"</label>
                                <input type="number" id="cache-policy-ttl-val" min="1" value="24" class="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-white focus:outline-none focus:border-red-500 font-mono" />
                            </div>
                            <div>
                                <label class="block text-[11px] font-semibold text-zinc-400 uppercase mb-1" data-i18n="settings.cache_unit_label">"Unité"</label>
                                <select id="cache-policy-ttl-unit" class="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-white focus:outline-none focus:border-red-500 font-mono">
                                    <option value="minutes" data-i18n="settings.unit_minutes">"Minutes"</option>
                                    <option value="hours" selected=true data-i18n="settings.unit_hours">"Heures"</option>
                                    <option value="days" data-i18n="settings.unit_days">"Jours"</option>
                                    <option value="weeks" data-i18n="settings.unit_weeks">"Semaines"</option>
                                </select>
                            </div>
                            <div>
                                <label class="block text-[11px] font-semibold text-zinc-400 uppercase mb-1" data-i18n="settings.cache_delay_label">"Délai anti-ban (ms)"</label>
                                <input type="number" id="cache-policy-delay" min="0" step="100" value="1000" class="w-full bg-zinc-900 border border-zinc-700 rounded-lg px-3 py-2 text-xs text-white focus:outline-none focus:border-red-500 font-mono" />
                            </div>
                        </div>
                        <div class="flex justify-end">
                            <button type="button" onclick="saveCachePolicy()" class="px-4 py-2 bg-red-600 hover:bg-red-500 text-white rounded-lg text-xs font-semibold shadow-md transition" data-i18n="settings.cache_btn_apply">
                                "Apply Policy"
                            </button>
                        </div>
                    </div>

                    // Active Policies Table
                    <div class="overflow-x-auto">
                        <table class="w-full text-left text-xs border-collapse">
                            <thead>
                                <tr class="border-b border-zinc-800 text-zinc-400 uppercase text-[10px] tracking-wider font-semibold">
                                    <th class="py-2.5 px-3" data-i18n="settings.cache_col_module">"Module"</th>
                                    <th class="py-2.5 px-3" data-i18n="settings.cache_col_ttl">"Durée de validité (TTL)"</th>
                                    <th class="py-2.5 px-3" data-i18n="settings.cache_col_delay">"Délai anti-ban"</th>
                                    <th class="py-2.5 px-3 text-right" data-i18n="settings.cache_col_action">"Action"</th>
                                </tr>
                            </thead>
                            <tbody id="cache-policies-tbody" class="divide-y divide-zinc-800/60 font-mono text-zinc-300">
                                <tr>
                                    <td colspan="4" class="py-4 text-center text-zinc-500" data-i18n="settings.cache_loading">"Chargement des politiques..."</td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </section>
    }
}
