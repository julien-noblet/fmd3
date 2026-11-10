use leptos::prelude::*;
use crate::components::navbar::Navbar;
use crate::views::catalog::CatalogView;
use crate::views::downloads::DownloadsView;
use crate::views::library::LibraryView;
use crate::views::merger::MergerView;
use crate::views::modules::ModulesView;
use crate::views::search::SearchView;
use crate::views::settings::SettingsView;
use crate::views::users::UsersView;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <div class="bg-zinc-950 text-zinc-100 min-h-screen font-sans selection:bg-red-600 selection:text-white">
            // Dedicated Full-Screen Authentication Gate (Setup Wizard / Login)
            <div id="auth-gate-container" class="relative min-h-screen flex items-center justify-center p-4 bg-zinc-950 text-zinc-100">
                <div class="fixed top-4 right-4 z-50">
                    <select id="auth-lang-select" onchange="setLocale(this.value)" class="bg-zinc-900 hover:bg-zinc-800 border border-zinc-700 text-xs text-white rounded-lg px-2.5 py-1.5 cursor-pointer focus:outline-none focus:border-red-500 font-medium transition shadow-lg">
                    </select>
                </div>

                // Screen: Initial Setup Wizard
                <div id="screen-setup-wizard" class="hidden w-full max-w-md bg-zinc-900 border border-red-600/40 rounded-3xl p-8 shadow-2xl space-y-6">
                    <div class="text-center space-y-2">
                        <img src="/logo.png" alt="FMD3 San" class="w-20 h-20 rounded-2xl mx-auto shadow-xl shadow-red-600/30 object-contain border border-zinc-800" />
                        <h2 class="text-2xl font-black text-white" data-i18n="auth.setup_title">"Initialisation de FMD3 San (三)"</h2>
                        <p class="text-xs text-zinc-400" data-i18n="auth.setup_desc">
                            "Welcome! No administrator account detected. Please create the initial admin account to secure and configure FMD3 San."
                        </p>
                    </div>

                    <form id="setup-wizard-form" onsubmit="submitSetupWizard(event)" class="space-y-4">
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="auth.setup_username">"Identifiant administrateur"</label>
                            <input type="text" id="setup-username" required=true placeholder="admin" data-i18n-placeholder="auth.setup_username_placeholder" autocomplete="username" class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2.5 text-sm text-white focus:outline-none focus:border-red-500" />
                        </div>

                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="auth.setup_password">"Mot de passe (4 caractères min)"</label>
                            <input type="password" id="setup-password" required=true placeholder="••••••••" autocomplete="new-password" class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2.5 text-sm text-white focus:outline-none focus:border-red-500" />
                        </div>

                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="auth.setup_confirm">"Confirmation du mot de passe"</label>
                            <input type="password" id="setup-confirm-password" required=true placeholder="••••••••" autocomplete="new-password" class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2.5 text-sm text-white focus:outline-none focus:border-red-500" />
                        </div>

                        <div id="setup-error" class="hidden p-3 rounded-lg bg-red-950/80 border border-red-800 text-red-400 text-xs"></div>

                        <button type="submit" class="w-full py-3 bg-red-600 hover:bg-red-500 text-white rounded-xl text-sm font-bold shadow-lg shadow-red-600/30 transition transform hover:-translate-y-0.5" data-i18n="auth.setup_submit">
                            "Initialiser FMD3 San & Se Connecter 🚀"
                        </button>
                    </form>
                </div>

                // Screen: Login
                <div id="screen-login" class="hidden w-full max-w-sm bg-zinc-900 border border-zinc-800 rounded-3xl p-8 shadow-2xl space-y-6">
                    <div class="text-center space-y-2">
                        <img src="/logo.png" alt="FMD3 San" class="w-16 h-16 rounded-2xl mx-auto shadow-lg shadow-red-600/30 object-contain border border-zinc-800" />
                        <h2 class="text-2xl font-black text-white" data-i18n="auth.login_title">"Connexion à FMD3 San (三)"</h2>
                        <p class="text-xs text-zinc-400" data-i18n="auth.login_desc">
                            "Access your manga library and download queue."
                        </p>
                    </div>

                    <form id="login-form" onsubmit="submitLogin(event)" class="space-y-4">
                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="auth.login_username">"Identifiant"</label>
                            <input type="text" id="login-username" required=true placeholder="Votre identifiant" data-i18n-placeholder="auth.login_username_placeholder" autocomplete="username" class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2.5 text-sm text-white focus:outline-none focus:border-red-500" />
                        </div>

                        <div>
                            <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="auth.login_password">"Mot de passe"</label>
                            <input type="password" id="login-password" required=true placeholder="••••••••" autocomplete="current-password" class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2.5 text-sm text-white focus:outline-none focus:border-red-500" />
                        </div>

                        <div id="login-error" class="hidden p-3 rounded-lg bg-red-950/80 border border-red-800 text-red-400 text-xs"></div>

                        <button type="submit" class="w-full py-3 bg-red-600 hover:bg-red-500 text-white rounded-xl text-sm font-bold shadow-lg shadow-red-600/30 transition transform hover:-translate-y-0.5" data-i18n="auth.login_submit">
                            "Se connecter"
                        </button>
                    </form>
                </div>
            </div>

            // Main Application Container (Hidden by default until authenticated)
            <div id="main-app-container" class="hidden min-h-screen flex flex-col">
                <Navbar />

                <main class="flex-1 max-w-7xl w-full mx-auto p-4 sm:p-6 lg:p-8">
                    // Stats Summary
                    <div class="grid grid-cols-1 sm:grid-cols-4 gap-4 mb-8">
                        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5 backdrop-blur cursor-pointer hover:border-zinc-700 transition" onclick="switchTab('favorites')">
                            <div class="text-xs font-medium uppercase tracking-wider text-zinc-400 mb-1" data-i18n="stats.favorites">"Séries Suivies (Favoris)"</div>
                            <div class="text-3xl font-extrabold text-white" id="stat-favorites">"-"</div>
                        </div>
                        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5 backdrop-blur cursor-pointer hover:border-zinc-700 transition" onclick="switchTab('catalog')">
                            <div class="text-xs font-medium uppercase tracking-wider text-zinc-400 mb-1" data-i18n="stats.catalog">"Catalogue Indexé (FTS5)"</div>
                            <div class="text-3xl font-extrabold text-white flex items-baseline gap-2">
                                <span id="stat-catalog">"-"</span>
                                <span class="text-xs font-normal text-emerald-400" id="stat-catalog-sub" data-i18n="stats.mangas">"Mangas"</span>
                            </div>
                        </div>
                        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5 backdrop-blur cursor-pointer hover:border-zinc-700 transition" onclick="switchTab('downloads')">
                            <div class="text-xs font-medium uppercase tracking-wider text-zinc-400 mb-1" data-i18n="stats.downloads">"Téléchargements"</div>
                            <div class="text-3xl font-extrabold text-white flex items-baseline gap-2">
                                <span id="stat-downloads">"-"</span>
                                <span class="text-xs font-normal text-amber-400" id="stat-downloads-active">"0"</span>
                            </div>
                        </div>
                        <div class="bg-zinc-900/60 border border-zinc-800 rounded-xl p-5 backdrop-blur cursor-pointer hover:border-zinc-700 transition" onclick="switchTab('modules')">
                            <div class="text-xs font-medium uppercase tracking-wider text-zinc-400 mb-1" data-i18n="stats.modules">"Modules Actifs"</div>
                            <div class="text-3xl font-extrabold text-white flex items-baseline gap-2">
                                <span id="stat-modules">"-"</span>
                            </div>
                        </div>
                    </div>

                    <LibraryView />
                    <CatalogView />
                    <SearchView />
                    <MergerView />
                    <DownloadsView />
                    <ModulesView />
                    <SettingsView />
                    <UsersView />
                </main>

                <footer class="border-t border-zinc-900 bg-zinc-950 py-6 text-center text-xs text-zinc-500">
                    <div class="max-w-7xl mx-auto px-4 flex flex-col sm:flex-row items-center justify-between gap-2">
                        <p>"FMD3 — Free Manga Downloader San (三)"</p>
                        <p class="font-mono text-zinc-600" data-i18n="common.license">"GPLv2 Licensed"</p>
                    </div>
                </footer>
            </div>
        </div>
    }
}
