use leptos::prelude::*;

#[component]
pub fn UsersView() -> impl IntoView {
    view! {
        <section id="tab-users" class="tab-content hidden">
            <div class="space-y-6">
                // Header
                <div class="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 bg-zinc-900 border border-zinc-800 rounded-2xl p-6 shadow-xl">
                    <div>
                        <h2 class="text-xl font-bold text-white flex items-center gap-2">
                            <span>"👥"</span> <span data-i18n="users.title">"Gestion des Utilisateurs & Droits d'Accès (RBAC)"</span>
                        </h2>
                        <p class="text-sm text-zinc-400 mt-1" data-i18n="users.subtitle">
                            "Manage local user accounts, administrative roles, and granular permissions."
                        </p>
                    </div>
                    <button onclick="openCreateUserModal()" class="px-4 py-2.5 bg-red-600 hover:bg-red-500 text-white rounded-lg text-sm font-semibold transition shadow-lg shadow-red-600/30 flex items-center gap-2">
                        <span>"+"</span> <span data-i18n="users.create">"Nouvel Utilisateur"</span>
                    </button>
                </div>

                // Users Table
                <div class="bg-zinc-900 border border-zinc-800 rounded-2xl shadow-xl overflow-hidden">
                    <div class="overflow-x-auto">
                        <table class="w-full text-left border-collapse text-xs sm:text-sm">
                            <thead>
                                <tr class="border-b border-zinc-800 bg-zinc-950/60 text-zinc-400 uppercase text-[11px] tracking-wider font-semibold">
                                    <th class="p-4" data-i18n="users.col_user">"Utilisateur"</th>
                                    <th class="p-4" data-i18n="users.col_role">"Rôle"</th>
                                    <th class="p-4" data-i18n="users.col_status">"Statut"</th>
                                    <th class="p-4" data-i18n="users.col_perms">"Permissions"</th>
                                    <th class="p-4" data-i18n="users.col_created">"Créé le"</th>
                                    <th class="p-4 text-right" data-i18n="users.col_actions">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody id="users-table-body" class="divide-y divide-zinc-800 text-zinc-300">
                                <tr>
                                    <td colspan="6" class="p-8 text-center text-zinc-500 font-mono" data-i18n="users.loading">
                                        "Chargement des comptes utilisateurs..."
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </div>

                // User Modal (Create / Edit)
                <div id="user-modal" class="hidden fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm">
                    <div class="bg-zinc-900 border border-zinc-700 rounded-2xl p-6 max-w-lg w-full shadow-2xl space-y-5">
                        <div class="flex items-center justify-between border-b border-zinc-800 pb-3">
                            <h3 id="user-modal-title" class="text-lg font-bold text-white flex items-center gap-2">
                                <span>"👤"</span> <span id="user-modal-title-text" data-i18n="users.modal_add_title">"Nouvel Utilisateur"</span>
                            </h3>
                            <button onclick="closeUserModal()" class="text-zinc-400 hover:text-white text-lg">"✕"</button>
                        </div>

                        <form id="user-form" onsubmit="saveUserForm(event)" class="space-y-4">
                            <input type="hidden" id="user-form-id" />

                            <div>
                                <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="users.field_username">"Nom d'utilisateur"</label>
                                <input type="text" id="user-form-username" required=true class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-sm text-white focus:outline-none focus:border-red-500" />
                            </div>

                            <div>
                                <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5">
                                    <span data-i18n="users.field_password">"Mot de passe"</span>
                                    " "
                                    <span id="user-pwd-hint" class="text-zinc-500 font-normal lowercase" data-i18n="users.pwd_hint_edit">"(laisser vide pour ne pas modifier)"</span>
                                </label>
                                <input type="password" id="user-form-password" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3.5 py-2 text-sm text-white focus:outline-none focus:border-red-500" />
                            </div>

                            <div class="grid grid-cols-2 gap-4">
                                <div>
                                    <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="users.field_role">"Rôle"</label>
                                    <select id="user-form-role" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-red-500">
                                        <option value="user" data-i18n="users.role_user">"Utilisateur"</option>
                                        <option value="admin" data-i18n="users.role_admin">"Administrateur"</option>
                                    </select>
                                </div>
                                <div>
                                    <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-1.5" data-i18n="users.field_status">"Statut du compte"</label>
                                    <select id="user-form-active" class="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-red-500">
                                        <option value="1" data-i18n="users.status_active">"Actif"</option>
                                        <option value="0" data-i18n="users.status_inactive">"Désactivé"</option>
                                    </select>
                                </div>
                            </div>

                            // Permissions section
                            <div class="pt-3 border-t border-zinc-800">
                                <label class="block text-xs font-semibold text-zinc-400 uppercase tracking-wider mb-2" data-i18n="users.col_perms">"Permissions Détaillées"</label>
                                <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs">
                                    <label class="flex items-center gap-2 p-2 rounded bg-zinc-950 border border-zinc-800 cursor-pointer hover:border-zinc-700">
                                        <input type="checkbox" id="perm-download" class="rounded bg-zinc-900 border-zinc-700 text-red-600 focus:ring-0" />
                                        <span>"📥 "</span><span data-i18n="users.perm_download">"Téléchargement"</span>
                                    </label>
                                    <label class="flex items-center gap-2 p-2 rounded bg-zinc-950 border border-zinc-800 cursor-pointer hover:border-zinc-700">
                                        <input type="checkbox" id="perm-favorites" class="rounded bg-zinc-900 border-zinc-700 text-red-600 focus:ring-0" />
                                        <span>"⭐ "</span><span data-i18n="users.perm_favorites">"Gérer favoris"</span>
                                    </label>
                                    <label class="flex items-center gap-2 p-2 rounded bg-zinc-950 border border-zinc-800 cursor-pointer hover:border-zinc-700">
                                        <input type="checkbox" id="perm-sync" class="rounded bg-zinc-900 border-zinc-700 text-red-600 focus:ring-0" />
                                        <span>"🔄 "</span><span data-i18n="users.perm_sync">"Sync catalogue"</span>
                                    </label>
                                    <label class="flex items-center gap-2 p-2 rounded bg-zinc-950 border border-zinc-800 cursor-pointer hover:border-zinc-700">
                                        <input type="checkbox" id="perm-force-refresh" class="rounded bg-zinc-900 border-zinc-700 text-red-600 focus:ring-0" />
                                        <span>"⚡ "</span><span data-i18n="users.perm_force_refresh">"Forcer refresh"</span>
                                    </label>
                                    <label class="flex items-center gap-2 p-2 rounded bg-zinc-950 border border-zinc-800 cursor-pointer hover:border-zinc-700">
                                        <input type="checkbox" id="perm-view-modules" class="rounded bg-zinc-900 border-zinc-700 text-red-600 focus:ring-0" />
                                        <span>"🧩 "</span><span data-i18n="users.perm_view_modules">"Voir modules"</span>
                                    </label>
                                </div>
                            </div>

                            <div class="flex justify-end gap-3 pt-3 border-t border-zinc-800">
                                <button type="button" onclick="closeUserModal()" class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded-lg text-xs font-semibold" data-i18n="common.cancel">
                                    "Annuler"
                                </button>
                                <button type="submit" class="px-5 py-2 bg-red-600 hover:bg-red-500 text-white rounded-lg text-xs font-semibold shadow-lg shadow-red-600/30" data-i18n="common.save">
                                    "Enregistrer"
                                </button>
                            </div>
                        </form>
                    </div>
                </div>
            </div>
        </section>
    }
}
