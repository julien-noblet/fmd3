pub const CLIENT_SCRIPT: &str = r#"        // ==========================================
        // FMD3 San (三) — Multilingual i18n Engine
        // ==========================================
        let currentLocale = 'en';
        let ws = null;
        let wsReconnectTimer = null;
        let allModules = [];
        let allFavorites = [];
        let currentInspectResult = null;
        let allDownloads = [];
        let allUsersList = [];
        let allCachePolicies = [];
        let catalogModule = 'all';
        let catalogSearchQuery = '';
        let catalogOffset = 0;
        let catalogLimit = 50;
        let catalogTotal = 0;
        let catalogSearchTimeout = null;
        let moduleSyncStatuses = {};
        let activeSyncInfo = null;
        let currentUser = null;
        let currentPermissions = null;
        let demoMode = false;
        let demoEnded = false;
        let demoPolling = false;
        let demoLastActivity = 0;

        function demoSetEnded(value) {
            demoEnded = value;
            try {
                sessionStorage.setItem('fmd_demo_ended', value ? '1' : '0');
                if (value) sessionStorage.removeItem('fmd_demo_active');
            } catch (e) {}
        }

        function demoSetActive() {
            demoSetEnded(false);
            try { sessionStorage.setItem('fmd_demo_active', '1'); } catch (e) {}
        }

        function demoMessage(key, params) {
            const el = document.getElementById('demo-status');
            if (el) el.textContent = t(key, params);
        }

        function endDemo(expired) {
            demoSetEnded(true);
            const dialog = document.getElementById('demo-dialog');
            if (dialog && dialog.open) dialog.close();
            currentUser = null;
            currentPermissions = null;
            clearTimeout(wsReconnectTimer);
            wsReconnectTimer = null;
            if (ws) {
                ws.onclose = null;
                ws.close();
                ws = null;
            }
            showAuthGate('login');
            const credentials = document.getElementById('demo-credentials');
            const restart = document.getElementById('demo-restart');
            const form = document.getElementById('login-form');
            if (credentials) credentials.classList.add('hidden');
            if (restart) restart.classList.remove('hidden');
            if (form) form.classList.add('hidden');
            demoMessage(expired ? 'demo.expired' : 'demo.ended');
        }

        async function pollDemoReservation() {
            if (!demoMode || demoEnded || currentUser || demoPolling) return;
            demoPolling = true;
            try {
                const res = await fetch('/api/demo/reservation', { method: 'POST' });
                if (!res.ok) throw new Error(await res.text());
                const data = await res.json();
                if (demoEnded || currentUser) return;
                const credentials = document.getElementById('demo-credentials');
                const form = document.getElementById('login-form');
                if (data.status === 'reserved') {
                    demoMessage('demo.available');
                    if (credentials) {
                        credentials.textContent = t('demo.credentials', { username: data.username, password: data.password });
                        credentials.classList.remove('hidden');
                    }
                    document.getElementById('login-username').value = data.username;
                    document.getElementById('login-password').value = data.password;
                    if (form) form.classList.remove('hidden');
                } else if (data.status === 'waiting') {
                    demoMessage('demo.waiting', { position: data.position });
                    if (credentials) credentials.classList.add('hidden');
                    if (form) form.classList.add('hidden');
                } else if (data.status === 'active') {
                    if (await checkAuthStatus()) loadAppData();
                    else endDemo(true);
                }
            } catch (e) {
                console.error('Demo reservation error:', e);
                demoMessage('demo.unavailable');
            } finally {
                demoPolling = false;
                if (!demoEnded && !currentUser) setTimeout(pollDemoReservation, 30000);
            }
        }

        function restartDemo() {
            demoSetEnded(false);
            const restart = document.getElementById('demo-restart');
            if (restart) restart.classList.add('hidden');
            pollDemoReservation();
        }

        async function demoActivity(event) {
            if (!demoMode || demoEnded || !event.isTrusted || Date.now() - demoLastActivity < 15000) return;
            demoLastActivity = Date.now();
            try {
                const res = await fetch('/api/demo/activity', { method: 'POST' });
                if (res.status === 401 && currentUser) endDemo(true);
            } catch (e) {
                console.error('Demo activity error:', e);
            }
        }

        async function checkDemoSession() {
            if (!demoMode) return;
            if (currentUser && !demoEnded) {
                try {
                    const res = await fetch('/api/auth/status');
                    if (!res.ok) throw new Error(await res.text());
                    const data = await res.json();
                    if (!data.authenticated) endDemo(true);
                } catch (e) {
                    console.error('Demo session check failed:', e);
                }
            }
            setTimeout(checkDemoSession, 30000);
        }

        window.addEventListener('pointerdown', demoActivity);
        window.addEventListener('pointermove', demoActivity);
        window.addEventListener('wheel', demoActivity);
        window.addEventListener('keydown', demoActivity);

        function getNestedValue(obj, keyPath) {
            if (!obj || !keyPath) return undefined;
            const parts = keyPath.split('.');
            let curr = obj;
            for (let i = 0; i < parts.length; i++) {
                if (curr === undefined || curr === null) return undefined;
                curr = curr[parts[i]];
            }
            return curr;
        }

        function t(key, params) {
            if (!key) return '';
            let val = undefined;
            // 1. Current locale lookup
            if (typeof LOCALES !== 'undefined' && LOCALES[currentLocale]) {
                val = getNestedValue(LOCALES[currentLocale], key);
            }
            // 2. Fallback to English
            if ((val === undefined || val === null) && typeof LOCALES !== 'undefined' && LOCALES['en']) {
                val = getNestedValue(LOCALES['en'], key);
            }
            // 3. Fallback to key itself
            if (val === undefined || val === null) {
                return key;
            }
            if (params && typeof params === 'object') {
                return String(val).replace(/\{(\w+)\}/g, (match, pName) => {
                    return params[pName] !== undefined ? params[pName] : match;
                });
            }
            return String(val);
        }

        function detectUserLocale() {
            try {
                const saved = localStorage.getItem('fmd_locale');
                if (saved && typeof LOCALES !== 'undefined' && LOCALES[saved]) {
                    return saved;
                }
            } catch (e) {}

            if (typeof navigator !== 'undefined' && navigator.language) {
                const navLang = navigator.language.replace('-', '_');
                if (typeof LOCALES !== 'undefined' && LOCALES[navLang]) {
                    return navLang;
                }
                const baseLang = navigator.language.split('-')[0].toLowerCase();
                if (typeof LOCALES !== 'undefined' && LOCALES[baseLang]) {
                    return baseLang;
                }
            }

            return 'en';
        }

        function setLocale(localeCode) {
            if (typeof LOCALES === 'undefined') return;
            if (!LOCALES[localeCode]) {
                const base = localeCode ? localeCode.split('_')[0].split('-')[0] : 'en';
                if (LOCALES[base]) {
                    localeCode = base;
                } else {
                    localeCode = 'en';
                }
            }

            currentLocale = localeCode;
            try {
                localStorage.setItem('fmd_locale', localeCode);
            } catch (e) {}

            if (document.documentElement) {
                document.documentElement.lang = localeCode;
            }

            const selectors = ['nav-lang-select', 'settings-lang-select', 'auth-lang-select'];
            selectors.forEach(id => {
                const el = document.getElementById(id);
                if (el && el.value !== localeCode) {
                    el.value = localeCode;
                }
            });

            const codeBadge = document.getElementById('current-lang-code');
            if (codeBadge) {
                codeBadge.textContent = localeCode;
            }

            applyTranslations();
        }

        function populateLanguageSelectors() {
            if (typeof LANGUAGES === 'undefined' || !Array.isArray(LANGUAGES)) return;
            const selectors = ['nav-lang-select', 'settings-lang-select', 'auth-lang-select'];
            selectors.forEach(id => {
                const el = document.getElementById(id);
                if (!el) return;
                el.innerHTML = '';
                LANGUAGES.forEach(lang => {
                    const opt = document.createElement('option');
                    opt.value = lang.code;
                    opt.textContent = `${lang.flag} ${lang.native} (${lang.name})`;
                    el.appendChild(opt);
                });
                el.value = currentLocale;
            });
        }

        function applyTranslations() {
            document.querySelectorAll('[data-i18n]').forEach(el => {
                const key = el.getAttribute('data-i18n');
                if (key) {
                    el.textContent = t(key);
                }
            });

            document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
                const key = el.getAttribute('data-i18n-placeholder');
                if (key) {
                    el.placeholder = t(key);
                }
            });

            document.querySelectorAll('[data-i18n-title]').forEach(el => {
                const key = el.getAttribute('data-i18n-title');
                if (key) {
                    const translated = t(key);
                    el.title = translated;
                    if (el.hasAttribute('aria-label')) {
                        el.setAttribute('aria-label', translated);
                    }
                }
            });

            const wsBadge = document.getElementById('ws-badge');
            if (wsBadge) {
                const isOnline = (typeof ws !== 'undefined') && ws && ws.readyState === WebSocket.OPEN;
                if (isOnline) {
                    wsBadge.innerHTML = '<span class="w-1.5 h-1.5 mr-1.5 rounded-full bg-emerald-400 animate-pulse"></span> ' + t('nav.ws_connected');
                } else {
                    wsBadge.innerHTML = '<span class="w-1.5 h-1.5 mr-1.5 rounded-full bg-amber-400"></span> ' + t('nav.ws_reconnecting');
                }
            }

            if (currentUser) {
                updateUserNav(currentUser);
            }
            if (allModules && allModules.length > 0) {
                populateModuleSelect(allModules);
                populateCachePolicyModules(allModules);
                filterModules();
            }
            if (allFavorites && allFavorites.length > 0) {
                renderFavorites(allFavorites);
            }
            if (allDownloads) {
                renderDownloads(allDownloads);
            }
            if (typeof updateSyncButton === 'function') {
                updateSyncButton();
            }
            if (activeSyncInfo && typeof updateSyncBanner === 'function') {
                updateSyncBanner(activeSyncInfo);
            }
            if (allUsersList && allUsersList.length > 0 && typeof renderUsersTable === 'function') {
                renderUsersTable(allUsersList);
            }
            if (allCachePolicies && allCachePolicies.length > 0 && typeof renderCachePolicies === 'function') {
                renderCachePolicies(allCachePolicies);
            }
        }

        function initI18n() {
            currentLocale = detectUserLocale();
            if (document.documentElement) {
                document.documentElement.lang = currentLocale;
            }
            populateLanguageSelectors();
            applyTranslations();
        }

        window.t = t;
        window.setLocale = setLocale;
        window.initI18n = initI18n;
        window.applyTranslations = applyTranslations;

        function setElemText(id, text) {
            const el = document.getElementById(id);
            if (el) {
                el.textContent = (text !== undefined && text !== null) ? text : '';
            }
        }

        const VALID_TABS = ['favorites', 'catalog', 'search', 'merger', 'downloads', 'modules', 'settings', 'users'];
        let currentActiveTab = 'favorites';

        function switchTab(tabId, pushHistory = true) {
            if (!VALID_TABS.includes(tabId)) {
                tabId = 'favorites';
            }

            closeAlternativeSourcesModal();

            document.querySelectorAll('.tab-content').forEach(el => el.classList.add('hidden'));
            document.querySelectorAll('.tab-btn').forEach(el => {
                el.classList.remove('bg-zinc-800', 'text-white');
                el.classList.add('text-zinc-400');
            });

            const tabEl = document.getElementById('tab-' + tabId);
            if (tabEl) tabEl.classList.remove('hidden');
            const activeBtn = document.getElementById('tab-btn-' + tabId);
            if (activeBtn) {
                activeBtn.classList.add('bg-zinc-800', 'text-white');
                activeBtn.classList.remove('text-zinc-400');
            }

            currentActiveTab = tabId;

            if (pushHistory) {
                const targetHash = '#' + tabId;
                if (window.location.hash !== targetHash) {
                    try {
                        history.pushState({ tab: tabId }, '', targetHash);
                    } catch (e) {}
                }
            }

            if (tabId === 'favorites') {
                loadFavorites();
            } else if (tabId === 'downloads') {
                loadDownloads();
            } else if (tabId === 'catalog') {
                loadCatalogStatus();
                if (catalogTotal === 0 && (!catalogSearchQuery || catalogSearchQuery.length === 0)) {
                    loadCatalog();
                }
            } else if (tabId === 'settings') {
                loadSettings();
                if (!demoMode) loadCachePolicies();
            } else if (tabId === 'users') {
                loadUsers();
            }
        }

        async function loadSettings() {
            try {
                const res = await fetch('/api/settings');
                if (!res.ok) return;
                const s = await res.json();
                const setVal = (id, val) => {
                    const el = document.getElementById(id);
                    if (el) el.value = val;
                };
                setVal('setting-download-dir', s.download_dir || 'downloads');
                setVal('setting-default-format', s.default_format || 'CBZ');
                setVal('setting-naming-template', s.naming_template || '{manga}/{chapter}');
                setVal('setting-flaresolverr-url', s.flaresolverr_url || '');
                setVal('setting-check-interval', s.check_interval_hours || 1);
                setVal('setting-catalog-resync', s.catalog_resync_days || 7);
                updateNamingPreview();
            } catch (e) {
                console.error("Error loading settings:", e);
            }
        }

        async function saveSettings() {
            const btn = document.getElementById('btn-save-settings');
            btn.disabled = true;
            btn.textContent = t('common.loading');

            const payload = {
                download_dir: document.getElementById('setting-download-dir').value.trim() || 'downloads',
                default_format: document.getElementById('setting-default-format').value,
                naming_template: document.getElementById('setting-naming-template').value.trim() || '{manga}/{chapter}',
                flaresolverr_url: document.getElementById('setting-flaresolverr-url').value.trim(),
                check_interval_hours: parseInt(document.getElementById('setting-check-interval').value) || 1,
                catalog_resync_days: parseInt(document.getElementById('setting-catalog-resync').value) || 7,
            };

            try {
                const res = await fetch('/api/settings', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload)
                });
                if (!res.ok) throw new Error(await res.text());

                const alertEl = document.getElementById('settings-alert');
                alertEl.classList.remove('hidden');
                setTimeout(() => alertEl.classList.add('hidden'), 4000);
            } catch (e) {
                alert(t("settings.error_save") + e.message);
            } finally {
                btn.disabled = false;
                btn.innerHTML = '<span>💾</span> ' + t('settings.save_btn');
            }
        }

        function insertNamingTag(tag) {
            const input = document.getElementById('setting-naming-template');
            input.value += (input.value.endsWith('/') ? '' : ' ') + tag;
            updateNamingPreview();
        }

        function updateNamingPreview() {
            const dirInput = document.getElementById('setting-download-dir');
            const fmtInput = document.getElementById('setting-default-format');
            const tplInput = document.getElementById('setting-naming-template');
            if (!dirInput || !fmtInput || !tplInput) return;

            const dir = (dirInput.value.trim() || 'downloads').replace(/\/+$/, '');
            const fmt = fmtInput.value;
            const tpl = tplInput.value.trim() || '{manga}/{chapter}';
            const ext = fmt === 'Directory' ? '' : '.' + fmt.toLowerCase();

            const resolved = tpl
                .replace('{manga}', 'One Piece')
                .replace('{chapter}', 'Chapitre 1180')
                .replace('{number}', '1180')
                .replace('{module}', 'SushiScan');

            setElemText('naming-preview', `${dir}/${resolved}${ext}`);
        }

        async function loadModules() {
            try {
                const res = await fetch('/api/modules');
                allModules = await res.json();
                setElemText('stat-modules', allModules.length);
                renderModules(allModules);
                populateModuleSelect(allModules);
                populateCachePolicyModules(allModules);
            } catch (e) {
                console.error("Erreur chargement modules:", e);
            }
        }

        function populateModuleSelect(modules) {
            const select = document.getElementById('fetch-module-select');
            if (select) {
                select.innerHTML = '<option value="">' + escapeHtml(t('search.select_module_placeholder')) + '</option>' +
                    modules.map(m => `<option value="${escapeHtml(m.id)}">${escapeHtml(m.name)} (${escapeHtml(m.category || t('common.category'))}) - ${escapeHtml(m.root_url)}</option>`).join('');
            }

            const catSelect = document.getElementById('catalog-module-select');
            if (catSelect) {
                catSelect.innerHTML = '<option value="all">🔍 ' + escapeHtml(t('catalog.all_modules_synced')) + '</option>' +
                    modules.map(m => `<option value="${escapeHtml(m.id)}">${escapeHtml(m.name)} (${escapeHtml(m.category || t('common.category'))})</option>`).join('');
            }
        }

        async function loadFavorites() {
            try {
                const res = await fetch('/api/favorites');
                const favs = await res.json();
                allFavorites = favs;
                setElemText('stat-favorites', favs.length);
                renderFavorites(favs);
            } catch (e) {
                console.error("Erreur chargement favoris:", e);
            }
        }

        function renderFavorites(favs) {
            const container = document.getElementById('favorites-list');
            if (!container) return;
            if (favs.length === 0) {
                container.innerHTML = `
                    <div class="col-span-full text-center py-16 text-zinc-500 bg-zinc-900/50 border border-zinc-800 rounded-2xl">
                        <p class="text-base mb-2">${escapeHtml(t('favorites.empty'))}</p>
                        <p class="text-sm text-zinc-400 mb-4">${escapeHtml(t('favorites.empty_desc'))}</p>
                        <button type="button" data-action="switch-tab" data-tab="search" class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-white rounded-lg text-xs font-medium transition">
                            ${escapeHtml(t('nav.search'))} &rarr;
                        </button>
                    </div>`;
                return;
            }

            container.innerHTML = favs.map(f => {
                const autoDlActive = f.auto_download;
                const dlCount = f.downloaded_chapters_count || 0;
                const totCount = f.total_chapters || 0;

                return `
                <div class="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden flex flex-col justify-between hover:border-zinc-700 transition shadow-lg">
                    <div class="relative h-48 bg-zinc-800 overflow-hidden">
                        ${f.cover_url ? `<img src="${proxyCover(f.cover_url)}" class="w-full h-full object-cover">` : `<div class="w-full h-full flex items-center justify-center text-zinc-600 font-bold text-2xl">Manga</div>`}
                        <div class="absolute top-2 left-2 flex gap-1">
                            <span class="px-2 py-0.5 rounded text-[10px] font-bold ${f.enabled ? 'bg-emerald-950 text-emerald-400 border border-emerald-800' : 'bg-zinc-950 text-zinc-500 border border-zinc-800'}">
                                ${f.enabled ? escapeHtml(t('common.active')) : escapeHtml(t('common.inactive'))}
                            </span>
                        </div>
                        <div class="absolute top-2 right-2 ${demoMode ? 'hidden' : ''}">
                            <button type="button" data-action="toggle-auto-dl" data-manga-id="${escapeHtml(f.manga_id)}" data-auto-dl="${!autoDlActive ? 'true' : 'false'}" title="${autoDlActive ? escapeHtml(t('favorites.disable_auto_dl')) : escapeHtml(t('favorites.enable_auto_dl'))}" class="px-2 py-0.5 rounded text-[10px] font-bold transition flex items-center gap-1 ${autoDlActive ? 'bg-red-600 text-white shadow' : 'bg-zinc-900/80 text-zinc-400 border border-zinc-700 hover:text-white'}">
                                <span>⚡</span> ${escapeHtml(t('favorites.auto_dl'))}: ${autoDlActive ? escapeHtml(t('common.on')) : escapeHtml(t('common.off'))}
                            </button>
                        </div>
                        <div class="absolute bottom-2 right-2 px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-black/70 text-zinc-300 backdrop-blur">
                            ${dlCount} / ${totCount} ${escapeHtml(t('favorites.chap_short'))}
                        </div>
                    </div>
                    <div class="p-4 flex-1 flex flex-col justify-between">
                        <div>
                            <h3 class="font-bold text-white text-sm line-clamp-1 mb-1">${escapeHtml(f.title)}</h3>
                            <a href="${escapeHtml(safeHttpUrl(f.original_url))}" target="_blank" rel="noopener noreferrer" class="text-xs text-red-400 hover:underline line-clamp-1 font-mono mb-3 block">${escapeHtml(f.original_url)}</a>
                            
                            <div class="flex items-center justify-between text-xs text-zinc-400 bg-zinc-950/60 p-2 rounded-lg border border-zinc-800/80 mb-2">
                                <span class="text-[11px]">${escapeHtml(t('favorites.dl_format'))}</span>
                                <select data-action="update-fav-format" data-manga-id="${escapeHtml(f.manga_id)}" class="bg-zinc-900 border border-zinc-700 text-xs text-zinc-200 rounded px-2 py-0.5 outline-none font-mono">
                                    <option value="CBZ" ${f.download_format === 'CBZ' ? 'selected' : ''}>CBZ</option>
                                    <option value="PDF" ${f.download_format === 'PDF' ? 'selected' : ''}>PDF</option>
                                    <option value="EPUB" ${f.download_format === 'EPUB' ? 'selected' : ''}>EPUB</option>
                                    <option value="Directory" ${f.download_format === 'Directory' ? 'selected' : ''}>${escapeHtml(t('favorites.format_folder'))}</option>
                                </select>
                            </div>
                        </div>
                        <div class="pt-3 border-t border-zinc-800 flex items-center justify-between text-xs gap-1">
                            <button type="button" data-action="check-fav" data-manga-id="${escapeHtml(f.manga_id)}" class="px-2.5 py-1.5 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded text-xs font-medium transition flex items-center gap-1 border border-zinc-700">
                                <span>🔄</span> ${escapeHtml(t('favorites.check'))}
                            </button>
                            <button type="button" data-action="open-fav-dl" data-manga-id="${escapeHtml(f.manga_id)}" data-url="${escapeHtml(f.original_url)}" class="px-2.5 py-1.5 bg-red-600/90 hover:bg-red-500 text-white rounded text-xs font-semibold transition flex items-center gap-1">
                                <span>📥</span> ${escapeHtml(t('favorites.chapters'))}
                            </button>
                            <button type="button" data-action="delete-fav" data-manga-id="${escapeHtml(f.manga_id)}" class="text-zinc-500 hover:text-red-400 p-1 text-xs" title="${escapeHtml(t('common.delete'))}">🗑️</button>
                        </div>
                    </div>
                </div>
                `;
            }).join('');
        }

        async function checkSingleFavorite(mangaId) {
            const btn = Array.from(document.querySelectorAll('[data-action="check-fav"]'))
                .find(el => el.dataset.mangaId === mangaId);
            if (btn) {
                btn.disabled = true;
                btn.innerHTML = '<span class="animate-spin">⏳</span> ...';
            }
            try {
                const res = await fetch('/api/favorites/' + encodeURIComponent(mangaId) + '/check', { method: 'POST' });
                if (!res.ok) throw new Error(await res.text());
                const data = await res.json();
                if (data.new_chapters_count > 0) {
                    alert(t("favorites.new_chapters_detected", { count: data.new_chapters_count }));
                } else {
                    alert(t("favorites.up_to_date"));
                }
                loadFavorites();
                loadDownloads();
            } catch (e) {
                alert(t("favorites.check_error") + e.message);
            } finally {
                if (btn) {
                    btn.disabled = false;
                    btn.innerHTML = '<span>🔄</span> ' + escapeHtml(t('favorites.check'));
                }
            }
        }

        async function checkAllFavorites() {
            const btn = document.getElementById('btn-check-all-favs');
            btn.disabled = true;
            btn.innerHTML = '<span class="animate-spin">⏳</span> ' + escapeHtml(t('favorites.checking_all'));
            try {
                const res = await fetch('/api/favorites/check-all', { method: 'POST' });
                if (!res.ok) throw new Error(await res.text());
                const data = await res.json();
                alert(t("favorites.check_all_finished", { checked: data.checked_count, new_count: data.total_new_chapters }));
                loadFavorites();
                loadDownloads();
            } catch (e) {
                alert(t("common.error") + ': ' + e.message);
            } finally {
                btn.disabled = false;
                btn.innerHTML = '<span>🔄</span> ' + escapeHtml(t('favorites.check_all'));
            }
        }

        async function toggleAutoDownload(mangaId, newVal) {
            try {
                await fetch('/api/favorites/' + encodeURIComponent(mangaId), {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ auto_download: newVal })
                });
                loadFavorites();
            } catch (e) {
                alert(t("common.error") + ": " + e.message);
            }
        }

        async function updateFavoriteFormat(mangaId, newFormat) {
            try {
                await fetch('/api/favorites/' + encodeURIComponent(mangaId), {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ download_format: newFormat })
                });
            } catch (e) {
                alert(t("common.error") + ": " + e.message);
            }
        }

        function openFavoriteForDownload(mangaId, url) {
            const parts = mangaId.split(':');
            const moduleId = parts[0];
            switchTab('search');
            document.getElementById('fetch-module-select').value = moduleId;
            document.getElementById('fetch-manga-url').value = url;
            fetchMangaDetails();
        }

        async function deleteFavorite(mangaId) {
            if (!confirm(t("favorites.confirm_remove"))) return;
            try {
                await fetch('/api/favorites/' + encodeURIComponent(mangaId), { method: 'DELETE' });
                loadFavorites();
            } catch (e) {
                alert(t("favorites.remove_error") + e);
            }
        }

        function renderModules(modules) {
            const tbody = document.getElementById('modules-list');
            if (!tbody) return;
            if (modules.length === 0) {
                tbody.innerHTML = '<tr><td colspan="4" class="px-5 py-6 text-center text-zinc-500">' + escapeHtml(t('modules.no_modules_found')) + '</td></tr>';
                return;
            }

            tbody.innerHTML = modules.map(m => `
                <tr class="hover:bg-zinc-800/40 transition">
                    <td class="px-5 py-3 font-semibold text-white">${escapeHtml(m.name)}</td>
                    <td class="px-5 py-3"><span class="px-2 py-0.5 rounded text-[11px] bg-zinc-800 text-zinc-300">${escapeHtml(m.category || t('common.category'))}</span></td>
                    <td class="px-5 py-3"><a href="${escapeHtml(safeHttpUrl(m.root_url))}" target="_blank" rel="noopener noreferrer" class="text-red-400 hover:underline">${escapeHtml(m.root_url)}</a></td>
                    <td class="px-5 py-3 text-right space-x-2">
                        <button type="button" data-action="open-catalog" data-module-id="${escapeHtml(m.id)}" class="px-2.5 py-1 bg-red-950 text-red-300 border border-red-800 hover:bg-red-900 rounded text-xs transition">
                            📖 ${escapeHtml(t('modules.explore'))}
                        </button>
                        <button type="button" data-action="select-module-search" data-module-id="${escapeHtml(m.id)}" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded text-xs transition">
                            ${escapeHtml(t('modules.url_btn'))} &rarr;
                        </button>
                    </td>
                </tr>
            `).join('');
        }

        function openCatalogForModule(moduleId) {
            switchTab('catalog');
            const select = document.getElementById('catalog-module-select');
            if (select) {
                select.value = moduleId;
                onCatalogModuleChanged();
            }
        }

        async function loadCatalogStatus() {
            try {
                const res = await fetch('/api/catalog/status');
                const data = await res.json();
                
                const statCat = document.getElementById('stat-catalog');
                if (statCat) statCat.textContent = data.total_catalog_items.toLocaleString();
                
                moduleSyncStatuses = {};
                for (const st of data.modules) {
                    moduleSyncStatuses[st.module_id] = st;
                }

                activeSyncInfo = data.active;
                updateSyncBanner(activeSyncInfo);
                updateSyncButton();
            } catch (e) {
                console.error("Erreur loadCatalogStatus:", e);
            }
        }

        function updateSyncButton() {
            const btn = document.getElementById('btn-sync-catalog');
            const icon = document.getElementById('sync-icon');
            const label = document.getElementById('sync-label');
            if (!btn || !icon || !label) return;
            btn.classList.remove('hidden');

            if (activeSyncInfo) {
                btn.className = "px-4 py-2 bg-amber-600 hover:bg-amber-700 text-white font-medium rounded-lg text-sm transition flex items-center gap-2 shadow-lg shadow-amber-600/20";
                icon.textContent = demoMode ? "🔄" : "⏹️";
                label.textContent = t(demoMode ? "catalog.syncing" : "catalog.sync_stop");
                btn.disabled = demoMode;
                btn.classList.toggle('opacity-50', demoMode);
                btn.classList.toggle('cursor-not-allowed', demoMode);
                return;
            }

            btn.className = "px-4 py-2 bg-red-600 hover:bg-red-700 text-white font-medium rounded-lg text-sm transition flex items-center gap-2 shadow-lg shadow-red-600/20";
            icon.textContent = "🔄";

            if (catalogModule === 'all') {
                label.textContent = t("catalog.sync_select_prompt");
                btn.disabled = true;
                btn.classList.add('opacity-50', 'cursor-not-allowed');
            } else {
                btn.disabled = false;
                btn.classList.remove('opacity-50', 'cursor-not-allowed');
                const mod = allModules.find(m => m.id === catalogModule);
                const modName = mod ? mod.name : catalogModule;
                const st = moduleSyncStatuses[catalogModule];
                if (st && st.total_mangas > 0) {
                    label.textContent = t("catalog.resync_module", { module: modName, count: st.total_mangas.toLocaleString() });
                } else {
                    label.textContent = t("catalog.sync_module", { module: modName });
                }
            }
        }

        function updateSyncBanner(info) {
            const banner = document.getElementById('catalog-sync-banner');
            const badge = document.getElementById('badge-catalog-sync');
            if (!banner || !badge) return;

            if (!info) {
                banner.classList.add('hidden');
                badge.classList.add('hidden');
                return;
            }

            banner.classList.remove('hidden');
            badge.classList.remove('hidden');

            setElemText('sync-banner-text', t('catalog.syncing_module', { module: info.module_name || info.module_id }));
            
            const pageStr = info.total_pages
                ? t('catalog.page_of', { current: info.current_page + 1, total: info.total_pages })
                : t('catalog.page_num', { current: info.current_page + 1 });
            setElemText('sync-banner-stats', `${pageStr} • ${t('catalog.mangas_found', { count: info.items_found.toLocaleString() })}`);

            const percent = info.total_pages ? Math.min(100, Math.round(((info.current_page + 1) / info.total_pages) * 100)) : 40;
            const bar = document.getElementById('sync-banner-bar');
            if (bar) bar.style.width = percent + '%';
        }

        function onCatalogModuleChanged() {
            catalogModule = document.getElementById('catalog-module-select').value;
            catalogOffset = 0;
            updateSyncButton();
            loadCatalog(true);
        }

        async function toggleCatalogSync() {
            if (demoMode && activeSyncInfo) return;
            if (activeSyncInfo) {
                try {
                    const response = await fetch('/api/catalog/sync/cancel', { method: 'POST' });
                    if (!response.ok) throw new Error(await response.text());
                    activeSyncInfo = null;
                    updateSyncBanner(null);
                    updateSyncButton();
                } catch (e) {
                    alert(t("catalog.cancel_error") + e);
                }
                return;
            }

            if (catalogModule === 'all') {
                alert(t("catalog.select_module_alert"));
                return;
            }

            try {
                const res = await fetch(`/api/catalog/sync/${encodeURIComponent(catalogModule)}`, { method: 'POST' });
                if (!res.ok) {
                    const err = await res.text();
                    throw new Error(err);
                }
                const result = await res.json();
                if (result.status === 'cached') {
                    await loadCatalogStatus();
                    await loadCatalog(true);
                    return;
                }
                activeSyncInfo = {
                    module_id: catalogModule,
                    module_name: catalogModule,
                    current_page: 0,
                    total_pages: null,
                    items_found: 0
                };
                updateSyncBanner(activeSyncInfo);
                updateSyncButton();
            } catch (e) {
                alert(t("catalog.sync_launch_error") + e.message);
            }
        }

        function debounceCatalogSearch() {
            clearTimeout(catalogSearchTimeout);
            catalogSearchTimeout = setTimeout(() => {
                catalogSearchQuery = document.getElementById('catalog-search-input').value.trim();
                catalogOffset = 0;
                loadCatalog(true);
            }, 250);
        }

        function changeCatalogPage(delta) {
            const newOffset = catalogOffset + (delta * catalogLimit);
            if (newOffset >= 0 && newOffset < catalogTotal) {
                catalogOffset = newOffset;
                loadCatalog(false);
            }
        }

        async function loadCatalog(resetOffset = false) {
            if (resetOffset) catalogOffset = 0;

            const tbody = document.getElementById('catalog-table-body');
            if (!tbody) return;
            tbody.innerHTML = '<tr><td colspan="4" class="px-5 py-8 text-center text-zinc-500">' + escapeHtml(t('catalog.searching')) + '</td></tr>';

            const params = new URLSearchParams({
                q: catalogSearchQuery,
                limit: catalogLimit.toString(),
                offset: catalogOffset.toString()
            });
            if (catalogModule && catalogModule !== 'all') {
                params.set('module_id', catalogModule);
            }

            try {
                const res = await fetch(`/api/catalog/search?${params.toString()}`);
                const data = await res.json();
                catalogTotal = data.total;

                setElemText('catalog-results-count', t('catalog.results_count', { count: data.total.toLocaleString() }));
                
                const curPage = Math.floor(catalogOffset / catalogLimit) + 1;
                const maxPage = Math.max(1, Math.ceil(data.total / catalogLimit));
                setElemText('catalog-page-indicator', `${curPage} / ${maxPage}`);

                document.getElementById('btn-cat-prev').disabled = (curPage <= 1);
                document.getElementById('btn-cat-next').disabled = (curPage >= maxPage);

                renderCatalogTable(data.items);
            } catch (e) {
                console.error("Erreur loadCatalog:", e);
                tbody.innerHTML = '<tr><td colspan="4" class="px-5 py-8 text-center text-red-400">' + escapeHtml(t('catalog.fetch_error')) + '</td></tr>';
            }
        }

        function renderCatalogTable(items) {
            const tbody = document.getElementById('catalog-table-body');
            if (!tbody) return;
            if (!items || items.length === 0) {
                tbody.innerHTML = `
                    <tr>
                        <td colspan="4" class="px-5 py-12 text-center text-zinc-500">
                            ${escapeHtml(t('catalog.no_mangas_found'))}
                            ${catalogModule !== 'all' ? '<br><span class="text-xs text-zinc-400 mt-1 inline-block">' + escapeHtml(t('catalog.empty_hint_prompt')) + '</span>' : ''}
                        </td>
                    </tr>`;
                return;
            }

            tbody.innerHTML = items.map(item => {
                const mod = allModules.find(m => m.id === item.module_id);
                const modName = mod ? mod.name : item.module_id;
                const fullLink = item.link.startsWith('http') ? item.link : ((mod ? mod.root_url.replace(/\/+$/, '') : '') + '/' + item.link.replace(/^\/+/, ''));
                const dateStr = item.updated_at ? new Date(item.updated_at).toLocaleDateString() : '-';

                return `
                    <tr class="hover:bg-zinc-800/40 transition">
                        <td class="px-5 py-3 font-semibold text-white">
                            <span class="hover:text-red-400 cursor-pointer" data-action="open-manga-direct" data-module-id="${escapeHtml(item.module_id)}" data-url="${escapeHtml(fullLink)}">
                                ${escapeHtml(item.title)}
                            </span>
                        </td>
                        <td class="px-5 py-3">
                            <span class="px-2 py-0.5 rounded text-[11px] bg-zinc-800 text-zinc-300 font-mono">${escapeHtml(modName)}</span>
                        </td>
                        <td class="px-5 py-3 text-zinc-400 text-xs">${escapeHtml(dateStr)}</td>
                        <td class="px-5 py-3 text-right space-x-2">
                            <button type="button" data-action="open-manga-direct" data-module-id="${escapeHtml(item.module_id)}" data-url="${escapeHtml(fullLink)}" class="px-3 py-1 bg-red-600/90 hover:bg-red-500 text-white rounded text-xs font-medium transition">
                                📥 ${escapeHtml(t('catalog.view_chapters'))}
                            </button>
                        </td>
                    </tr>
                `;
            }).join('');
        }

        function openMangaDirect(moduleId, url) {
            switchTab('search');
            document.getElementById('fetch-module-select').value = moduleId;
            document.getElementById('fetch-manga-url').value = url;
            fetchMangaDetails();
        }

        function selectModuleForSearch(moduleId) {
            switchTab('search');
            document.getElementById('fetch-module-select').value = moduleId;
            document.getElementById('fetch-manga-url').focus();
        }

        function filterModules() {
            const query = document.getElementById('module-search').value.toLowerCase();
            const filtered = allModules.filter(m => 
                m.name.toLowerCase().includes(query) || 
                (m.category && m.category.toLowerCase().includes(query)) ||
                (m.root_url && m.root_url.toLowerCase().includes(query))
            );
            renderModules(filtered);
        }

        async function fetchMangaDetails(force = false) {
            const moduleId = document.getElementById('fetch-module-select').value;
            const url = document.getElementById('fetch-manga-url').value.trim();

            if (!moduleId) {
                alert(t("search.alert_select_module"));
                return;
            }
            if (!url) {
                alert(t("search.alert_enter_url"));
                return;
            }

            const btn = document.getElementById('btn-inspect-manga');
            const forceBtn = document.getElementById('btn-force-refresh');
            if (btn) {
                btn.disabled = true;
                btn.innerHTML = '<span class="animate-spin">⏳</span> ' + escapeHtml(t('search.scraping_in_progress'));
            }
            if (forceBtn) {
                forceBtn.disabled = true;
                forceBtn.innerHTML = '<span class="animate-spin">⏳</span> ' + escapeHtml(t('search.refreshing'));
            }

            try {
                const res = await fetch('/api/manga/fetch', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ module_id: moduleId, url: url, force_refresh: force })
                });

                if (!res.ok) {
                    const err = await res.text();
                    throw new Error(err || t("search.fetch_failed"));
                }

                currentInspectResult = await res.json();
                currentInspectResult.module_id = moduleId;
                currentInspectResult.url = url;
                currentInspectResult.is_merged = false;
                currentInspectResult.original_chapters = null;

                // Cache status badge
                const cacheBadge = document.getElementById('manga-cache-badge');
                if (cacheBadge) {
                    if (currentInspectResult.is_cached) {
                        let text = '⚡ ' + t('search.cached_data');
                        if (currentInspectResult.expires_at) {
                            const exp = new Date(currentInspectResult.expires_at);
                            const diffMin = Math.round((exp - new Date()) / 60000);
                            if (diffMin > 60) {
                                text += ' (' + t('search.expires_in_hours', { count: Math.round(diffMin / 60) }) + ')';
                            } else if (diffMin > 0) {
                                text += ' (' + t('search.expires_in_mins', { count: diffMin }) + ')';
                            }
                        }
                        cacheBadge.textContent = text;
                        cacheBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono border bg-emerald-950/80 text-emerald-400 border-emerald-800';
                        cacheBadge.classList.remove('hidden');
                    } else {
                        cacheBadge.textContent = '🔄 ' + t('search.fresh_data');
                        cacheBadge.className = 'px-2 py-0.5 rounded text-[10px] font-mono border bg-blue-950/80 text-blue-400 border-blue-800';
                        cacheBadge.classList.remove('hidden');
                    }
                }

                // Reset hybrid source UI
                const hybridBar = document.getElementById('hybrid-sources-bar');
                if (hybridBar) hybridBar.classList.add('hidden');
                const gapFillBtn = document.getElementById('btn-select-gap-fill');
                if (gapFillBtn) gapFillBtn.classList.add('hidden');

                // Display preview
                setElemText('prev-title', currentInspectResult.title || t('search.unknown_title'));
                setElemText('prev-status', currentInspectResult.status || t('search.status_ongoing'));
                setElemText('prev-authors', currentInspectResult.authors ? t('search.authors_prefix') + ' ' + currentInspectResult.authors : '');
                setElemText('prev-summary', currentInspectResult.summary || t('search.no_summary'));
                setElemText('prev-chapters-count', currentInspectResult.chapters ? currentInspectResult.chapters.length : 0);
                
                const cover = document.getElementById('prev-cover');
                if (cover) {
                    if (currentInspectResult.cover_url) {
                        cover.src = proxyCover(currentInspectResult.cover_url);
                        cover.classList.remove('hidden');
                    } else {
                        cover.classList.add('hidden');
                    }
                }

                // Fetch already downloaded chapters if favorite is tracked
                let downloadedMap = {};
                try {
                    const chRes = await fetch('/api/favorites/' + encodeURIComponent(`${moduleId}:${url}`) + '/chapters');
                    if (chRes.ok) {
                        const saved = await chRes.json();
                        for (const item of saved) {
                            if (item.is_downloaded) {
                                downloadedMap[item.url] = true;
                            }
                        }
                    }
                } catch(e) {}

                // Render Alternate Language Versions if available
                const langBox = document.getElementById('prev-languages-box');
                const langList = document.getElementById('prev-languages-list');
                if (langBox && langList) {
                    langList.innerHTML = '';
                    const alts = currentInspectResult.alternate_versions || [];
                    if (alts.length > 1) {
                        alts.forEach(alt => {
                            const isCurrent = alt.url === currentInspectResult.url;
                            const badge = document.createElement('button');
                            badge.type = 'button';
                            badge.className = isCurrent
                                ? 'px-2.5 py-1 text-xs font-bold rounded bg-red-600 text-white shadow-sm ring-1 ring-red-400'
                                : 'px-2.5 py-1 text-xs font-medium rounded bg-zinc-800 text-zinc-300 hover:bg-zinc-700 hover:text-white border border-zinc-700 transition cursor-pointer';
                            badge.textContent = alt.language || alt.title;
                            badge.title = alt.title + ' (' + alt.url + ')';
                            if (!isCurrent) {
                                badge.onclick = () => openMangaDirect(moduleId, alt.url);
                            }
                            langList.appendChild(badge);
                        });
                        langBox.classList.remove('hidden');
                    } else {
                        langBox.classList.add('hidden');
                    }
                }

                // Render Chapters List
                renderChaptersCheckboxes(currentInspectResult.chapters || [], downloadedMap);

                // Gap detection & Multi-Source alert
                checkAndDisplayGaps(currentInspectResult.chapters || []);

                const previewBox = document.getElementById('manga-preview');
                if (previewBox) previewBox.classList.remove('hidden');
            } catch (e) {
                alert(t("search.fetch_error") + e.message);
            } finally {
                if (btn) {
                    btn.disabled = false;
                    btn.innerHTML = '<span>🔍</span> ' + escapeHtml(t('search.fetch_btn'));
                }
                if (forceBtn) {
                    forceBtn.disabled = false;
                    forceBtn.innerHTML = '<span>🔄</span> ' + escapeHtml(t('search.btn_force_refresh'));
                }
            }
        }

        function parseChapterNum(title) {
            if (!title) return null;
            const m = title.match(/(?:chapitre|chapter|ch\.?|ep\.?|volume|vol\.?|tome)?\s*(\d+(?:\.\d+)?)/i);
            return m ? parseFloat(m[1]) : null;
        }

        function checkAndDisplayGaps(chapters) {
            const banner = document.getElementById('multi-source-banner');
            if (!banner) return;
            if (!chapters || chapters.length === 0) {
                banner.classList.add('hidden');
                return;
            }

            const nums = [];
            for (const ch of chapters) {
                const n = parseChapterNum(ch.title);
                if (n !== null) nums.push(n);
            }

            nums.sort((a, b) => a - b);
            const gaps = [];
            for (let i = 0; i < nums.length - 1; i++) {
                const a = nums[i];
                const b = nums[i + 1];
                if (b - a > 1.0001) {
                    const start = Math.floor(a) + 1;
                    const end = Math.floor(b) - 1;
                    if (start <= end) {
                        gaps.push({ start, end, count: end - start + 1 });
                    }
                }
            }

            if (gaps.length > 0) {
                const totalMissing = gaps.reduce((acc, g) => acc + g.count, 0);
                const gapDescriptions = gaps.map(g => g.start === g.end ? `ch. ${g.start}` : `ch. ${g.start} - ${g.end}`).join(', ');
                setElemText('gap-alert-title', t('merger.gaps_detected', { count: totalMissing }));
                setElemText('gap-alert-desc', gapDescriptions);
                banner.classList.remove('hidden');
            } else {
                banner.classList.add('hidden');
            }
        }

        function openAlternativeSourcesModal() {
            if (!currentInspectResult) return;
            const modal = document.getElementById('modal-alternative-sources');
            if (!modal) return;
            modal.classList.remove('hidden');
            modal.classList.add('flex');

            const select = document.getElementById('alt-custom-module');
            if (select && allModules) {
                select.innerHTML = allModules
                    .filter(m => m.id !== currentInspectResult.module_id)
                    .map(m => `<option value="${escapeHtml(m.id)}">${escapeHtml(m.name)}</option>`)
                    .join('');
            }

            searchAlternativeSources();
        }

        function closeAlternativeSourcesModal() {
            const modal = document.getElementById('modal-alternative-sources');
            if (modal) {
                modal.classList.add('hidden');
                modal.classList.remove('flex');
            }
        }

        async function searchAlternativeSources() {
            if (!currentInspectResult) return;
            const loading = document.getElementById('alt-sources-loading');
            const list = document.getElementById('alt-sources-list');
            if (loading) loading.classList.remove('hidden');
            if (list) list.innerHTML = '';

            try {
                const res = await fetch(`/api/manga/alternatives?q=${encodeURIComponent(currentInspectResult.title)}&exclude_module=${encodeURIComponent(currentInspectResult.module_id)}`);
                if (!res.ok) throw new Error(await res.text());
                const alts = await res.json();
                if (loading) loading.classList.add('hidden');

                if (alts.length === 0) {
                    list.innerHTML = `
                        <div class="p-4 text-center text-zinc-500 bg-zinc-950/40 rounded-xl border border-zinc-800">
                            ${escapeHtml(t('merger.no_matches'))}<br>
                            <span class="text-[11px] text-zinc-400">${escapeHtml(t('merger.direct_url_tip'))}</span>
                        </div>`;
                    return;
                }

                list.innerHTML = alts.map(alt => `
                    <div class="p-3 bg-zinc-950/70 border border-zinc-800 hover:border-zinc-700 rounded-xl flex items-center justify-between gap-3 transition">
                        <div class="min-w-0">
                            <div class="font-bold text-white text-xs truncate">${escapeHtml(alt.title)}</div>
                            <div class="text-[11px] text-zinc-400 flex items-center gap-2 mt-0.5">
                                <span class="px-2 py-0.5 rounded text-[10px] bg-zinc-800 text-zinc-300 font-mono">${escapeHtml(alt.module_name || alt.module_id)}</span>
                                <span class="text-zinc-500 font-mono truncate max-w-xs">${escapeHtml(alt.url)}</span>
                            </div>
                        </div>
                        <button type="button" data-action="merge-with-source" data-module-id="${escapeHtml(alt.module_id)}" data-url="${escapeHtml(alt.url)}" data-module-name="${escapeHtml(alt.module_name || alt.module_id)}" class="flex-shrink-0 px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white font-medium rounded-lg text-xs transition flex items-center gap-1 shadow-sm">
                            <span>🔗</span> ${escapeHtml(t('merger.btn_merge'))}
                        </button>
                    </div>
                `).join('');
            } catch (e) {
                if (loading) loading.classList.add('hidden');
                list.innerHTML = `<div class="p-3 text-red-400 text-xs text-center">${escapeHtml(t('common.error'))}: ${escapeHtml(e.message)}</div>`;
            }
        }

        function mergeCustomSource() {
            const select = document.getElementById('alt-custom-module');
            const input = document.getElementById('alt-custom-url');
            if (!select || !input) return;
            const moduleId = select.value;
            const url = input.value.trim();
            if (!url) {
                alert(t("merger.alert_enter_alt_url"));
                return;
            }
            const mod = allModules.find(m => m.id === moduleId);
            const modName = mod ? mod.name : moduleId;
            mergeWithSource(moduleId, url, modName);
        }

        async function mergeWithSource(secondaryModuleId, secondaryUrl, secondaryModuleName) {
            if (!currentInspectResult) return;
            closeAlternativeSourcesModal();

            const banner = document.getElementById('multi-source-banner');
            if (banner) banner.classList.add('hidden');

            const countEl = document.getElementById('prev-chapters-count');
            const prevCount = countEl ? countEl.textContent : '';
            if (countEl) countEl.textContent = t('merger.merging');

            try {
                const primaryChapters = (currentInspectResult.original_chapters || currentInspectResult.chapters).map(c => ({
                    title: c.title,
                    url: c.url
                }));

                const payload = {
                    primary: {
                        module_id: currentInspectResult.module_id,
                        url: currentInspectResult.url,
                        chapters: primaryChapters
                    },
                    secondary: {
                        module_id: secondaryModuleId,
                        url: secondaryUrl
                    }
                };

                const res = await fetch('/api/manga/merge', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload)
                });

                if (!res.ok) {
                    const errText = await res.text();
                    throw new Error(errText || t('merger.merge_error'));
                }

                const merged = await res.json();

                if (!currentInspectResult.original_chapters) {
                    currentInspectResult.original_chapters = currentInspectResult.chapters.slice();
                }

                currentInspectResult.is_merged = true;
                currentInspectResult.chapters = merged.chapters.map(mc => ({
                    title: mc.title,
                    url: mc.selected_source.chapter_url,
                    module_id: mc.selected_source.module_id,
                    module_name: mc.selected_source.module_name,
                    is_gap_fill: mc.is_gap_fill,
                    display_number: mc.display_number,
                    available_sources: mc.available_sources
                }));

                setElemText('prev-chapters-count', currentInspectResult.chapters.length);

                // Update Hybrid Sources Bar
                const hybridBar = document.getElementById('hybrid-sources-bar');
                const badgesSpan = document.getElementById('hybrid-sources-badges');
                if (hybridBar && badgesSpan) {
                    const primaryCount = currentInspectResult.chapters.filter(c => !c.is_gap_fill).length;
                    const fillCount = currentInspectResult.chapters.filter(c => c.is_gap_fill).length;
                    const primName = allModules.find(m => m.id === currentInspectResult.module_id)?.name || currentInspectResult.module_id;

                    badgesSpan.innerHTML = `
                        <span class="px-2 py-0.5 rounded text-[11px] font-semibold bg-emerald-950 text-emerald-300 border border-emerald-800">
                            ${escapeHtml(primName)} (${primaryCount})
                        </span>
                        <span class="px-2 py-0.5 rounded text-[11px] font-semibold bg-blue-950 text-blue-300 border border-blue-800">
                            + ${escapeHtml(secondaryModuleName)} (${t('merger.filled_count', { count: fillCount })})
                        </span>
                    `;
                    hybridBar.classList.remove('hidden');
                }

                // Show gap fill button
                const gapFillBtn = document.getElementById('btn-select-gap-fill');
                if (gapFillBtn) gapFillBtn.classList.remove('hidden');

                // Re-render chapters
                renderChaptersCheckboxes(currentInspectResult.chapters);

                alert(t("merger.merge_success", { total: currentInspectResult.chapters.length, filled: currentInspectResult.chapters.filter(c => c.is_gap_fill).length }));
            } catch (e) {
                alert(t("merger.merge_error") + e.message);
                if (countEl) countEl.textContent = prevCount;
                checkAndDisplayGaps(currentInspectResult.chapters || []);
            }
        }

        function revertToSingleSource() {
            if (!currentInspectResult || !currentInspectResult.original_chapters) return;
            currentInspectResult.chapters = currentInspectResult.original_chapters.slice();
            currentInspectResult.original_chapters = null;
            currentInspectResult.is_merged = false;

            const hybridBar = document.getElementById('hybrid-sources-bar');
            if (hybridBar) hybridBar.classList.add('hidden');

            const gapFillBtn = document.getElementById('btn-select-gap-fill');
            if (gapFillBtn) gapFillBtn.classList.add('hidden');

            setElemText('prev-chapters-count', currentInspectResult.chapters.length);
            renderChaptersCheckboxes(currentInspectResult.chapters);
            checkAndDisplayGaps(currentInspectResult.chapters);
        }

        function selectOnlyGapFillChapters() {
            selectAllChapters(false);
            document.querySelectorAll('.chapter-cb[data-gap-fill="true"]').forEach(cb => cb.checked = true);
            updateSelectedCount();
        }

        function renderChaptersCheckboxes(chapters, downloadedMap = {}) {
            const container = document.getElementById('chapters-list-container');
            if (!container) return;
            if (chapters.length === 0) {
                container.innerHTML = '<div class="p-4 text-center text-zinc-500">' + escapeHtml(t('search.no_chapters')) + '</div>';
                updateSelectedCount();
                return;
            }

            container.innerHTML = chapters.map((ch, idx) => {
                const isDl = downloadedMap[ch.url];
                const isGap = !!ch.is_gap_fill;
                const sourceBadge = ch.module_name
                    ? (isGap
                        ? `<span class="px-1.5 py-0.5 rounded text-[10px] font-mono bg-blue-950 text-blue-300 border border-blue-800" title="${escapeHtml(t('merger.tag_smart_hybrid'))}">➕ ${escapeHtml(ch.module_name)}</span>`
                        : `<span class="px-1.5 py-0.5 rounded text-[10px] font-mono bg-zinc-800 text-zinc-400 border border-zinc-700">${escapeHtml(ch.module_name)}</span>`)
                    : '';

                return `
                <label class="flex items-center justify-between px-4 py-2 hover:bg-zinc-800/40 cursor-pointer transition select-none ${isGap ? 'bg-blue-950/20 border-l-2 border-blue-500' : ''}">
                    <div class="flex items-center gap-3">
                        <input type="checkbox" data-index="${idx}" data-gap-fill="${isGap ? 'true' : 'false'}" ${isDl ? '' : 'checked'} class="chapter-cb w-4 h-4 rounded bg-zinc-950 border-zinc-700 text-red-600 focus:ring-0">
                        <span class="font-medium text-white">${escapeHtml(ch.title)}</span>
                        ${sourceBadge}
                        ${isDl ? `<span class="px-2 py-0.5 rounded text-[10px] font-bold bg-emerald-950 text-emerald-400 border border-emerald-800">${escapeHtml(t('downloads.status_downloaded'))}</span>` : ''}
                    </div>
                    <a href="${escapeHtml(safeHttpUrl(ch.url))}" target="_blank" rel="noopener noreferrer" class="chapter-external-link text-zinc-500 hover:text-red-400 font-mono text-[11px] truncate max-w-xs">
                        ${escapeHtml(ch.url)}
                    </a>
                </label>
            `;}).join('');

            updateSelectedCount();
        }

        function updateSelectedCount() {
            const cbs = document.querySelectorAll('.chapter-cb:checked');
            setElemText('selected-chap-count', cbs.length);
        }

        function selectAllChapters(checked) {
            document.querySelectorAll('.chapter-cb').forEach(cb => cb.checked = checked);
            updateSelectedCount();
        }

        function invertChapterSelection() {
            document.querySelectorAll('.chapter-cb').forEach(cb => cb.checked = !cb.checked);
            updateSelectedCount();
        }

        function selectLastNChapters(n) {
            selectAllChapters(false);
            const cbs = Array.from(document.querySelectorAll('.chapter-cb'));
            // Chapters are typically oldest to newest or newest to oldest; select first or last n
            const slice = cbs.slice(-n);
            slice.forEach(cb => cb.checked = true);
            updateSelectedCount();
        }

        async function queueSelectedChapters() {
            if (!currentInspectResult || !currentInspectResult.chapters) return;

            const selectedIndices = Array.from(document.querySelectorAll('.chapter-cb:checked')).map(cb => parseInt(cb.getAttribute('data-index')));
            if (selectedIndices.length === 0) {
                alert(t("search.alert_select_chapter"));
                return;
            }

            const chaptersToQueue = selectedIndices.map(idx => {
                const ch = currentInspectResult.chapters[idx];
                return {
                    title: ch.title,
                    url: ch.url,
                    module_id: ch.module_id || currentInspectResult.module_id
                };
            });
            const format = document.getElementById('dl-format-select').value;

            const btn = document.getElementById('btn-dl-queue');
            btn.disabled = true;
            btn.innerHTML = '<span class="animate-spin">⏳</span> ' + escapeHtml(t('downloads.queuing'));

            try {
                const res = await fetch('/api/downloads/queue', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        module_id: currentInspectResult.module_id,
                        manga_title: currentInspectResult.title,
                        manga_url: currentInspectResult.url,
                        author: currentInspectResult.authors,
                        genres: currentInspectResult.genres,
                        description: currentInspectResult.summary,
                        cover_url: currentInspectResult.cover_url,
                        format: format,
                        chapters: chaptersToQueue
                    })
                });

                if (!res.ok) throw new Error(t('downloads.queue_error'));
                const data = await res.json();
                alert(t("downloads.queued_success", { count: data.queued, format: format }));
                switchTab('downloads');
            } catch (e) {
                alert(t("downloads.queue_error") + e.message);
            } finally {
                btn.disabled = false;
                btn.innerHTML = '<span>📥</span> <span data-i18n="search.btn_download_selected">' + escapeHtml(t('search.btn_download_selected')) + '</span><span id="selected-chap-count">0</span><span>)</span>';
                updateSelectedCount();
            }
        }

        async function loadDownloads() {
            try {
                const res = await fetch('/api/downloads');
                allDownloads = await res.json();
                renderDownloads(allDownloads);
            } catch (e) {
                console.error("Error loading downloads:", e);
            }
        }

        function renderDownloads(dls) {
            const tbody = document.getElementById('downloads-table-body');
            setElemText('stat-downloads', dls.length);
            
            const activeCount = dls.filter(d => d.status === 'Downloading' || d.status === 'Pending').length;
            setElemText('stat-downloads-active', t('stats.active_count', { count: activeCount }));

            const failedCount = dls.filter(d => d.status === 'Failed').length;
            const retryFailedBtn = document.getElementById('btn-retry-failed');
            if (retryFailedBtn) {
                if (failedCount > 0) {
                    retryFailedBtn.classList.remove('hidden');
                    retryFailedBtn.classList.add('flex');
                    setElemText('failed-downloads-count', failedCount);
                } else {
                    retryFailedBtn.classList.remove('flex');
                    retryFailedBtn.classList.add('hidden');
                }
            }

            const badge = document.getElementById('badge-dl-count');
            if (badge) {
                badge.textContent = activeCount;
                if (activeCount > 0) {
                    badge.className = 'px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-red-600 text-white animate-pulse';
                } else {
                    badge.className = 'px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-zinc-800 text-zinc-400';
                }
            }

            if (!tbody) return;

            if (dls.length === 0) {
                tbody.innerHTML = '<tr><td colspan="5" class="px-5 py-8 text-center text-zinc-500">' + escapeHtml(t('downloads.empty_queue')) + '</td></tr>';
                return;
            }

            tbody.innerHTML = dls.map(d => {
                let statusBadge = '';
                let progressDisplay = '';

                if (d.status === 'Completed') {
                    statusBadge = '<span class="px-2.5 py-1 rounded-full text-[11px] font-bold bg-emerald-950 text-emerald-400 border border-emerald-800">' + escapeHtml(t('downloads.status_completed')) + '</span>';
                    progressDisplay = `<div class="text-zinc-400 font-mono">${d.downloaded_pages} / ${d.total_pages} ${escapeHtml(t('downloads.pages'))} (100%)</div>`;
                } else if (d.status === 'Downloading') {
                    const retryInfo = d.retry_count > 0 ? ` <span class="text-[10px] text-amber-300 font-mono">(${d.retry_count + 1}/${d.max_retries || 3})</span>` : '';
                    statusBadge = `<span class="px-2.5 py-1 rounded-full text-[11px] font-bold bg-blue-950 text-blue-400 border border-blue-800 animate-pulse">${escapeHtml(t('downloads.status_downloading'))}${retryInfo}</span>`;
                    const pct = d.total_pages > 0 ? Math.round((d.downloaded_pages / d.total_pages) * 100) : 0;
                    progressDisplay = `
                        <div class="w-48">
                            <div class="flex justify-between text-[11px] text-zinc-400 mb-1 font-mono">
                                <span>${d.downloaded_pages} / ${d.total_pages} ${escapeHtml(t('downloads.pages'))}</span>
                                <span>${pct}%</span>
                            </div>
                            <div class="w-full bg-zinc-800 rounded-full h-1.5 overflow-hidden">
                                <div class="bg-red-500 h-1.5 rounded-full transition-all duration-300" style="width: ${pct}%"></div>
                            </div>
                        </div>`;
                } else if (d.status === 'Failed') {
                    const retriesInfo = d.retry_count > 0 ? ` <span class="text-[10px] text-zinc-500 font-mono">(${d.retry_count}/${d.max_retries || 3})</span>` : '';
                    statusBadge = `<span class="px-2.5 py-1 rounded-full text-[11px] font-bold bg-red-950 text-red-400 border border-red-800">${escapeHtml(t('downloads.status_failed'))}${retriesInfo}</span>`;
                    progressDisplay = `<span class="text-red-400 text-[11px] line-clamp-1" title="${escapeHtml(d.error_message || '')}">⚠️ ${escapeHtml(d.error_message || t('common.unknown_error'))}</span>`;
                } else if (d.status === 'Pending' && d.retry_count > 0) {
                    statusBadge = `<span class="px-2.5 py-1 rounded-full text-[11px] font-bold bg-amber-950 text-amber-300 border border-amber-800 animate-pulse">${escapeHtml(t('downloads.status_retry'))} (${d.retry_count}/${d.max_retries || 3})</span>`;
                    progressDisplay = `<span class="text-amber-400/90 text-[11px] italic line-clamp-1" title="${escapeHtml(d.error_message || '')}">${escapeHtml(t('downloads.status_queued'))}</span>`;
                } else {
                    statusBadge = '<span class="px-2.5 py-1 rounded-full text-[11px] font-bold bg-zinc-800 text-amber-400 border border-zinc-700">' + escapeHtml(t('downloads.status_queued')) + '</span>';
                    progressDisplay = '<span class="text-zinc-500 italic">' + escapeHtml(t('downloads.status_queued')) + '</span>';
                }

                return `
                    <tr class="hover:bg-zinc-800/30 transition">
                        <td class="px-5 py-3.5">
                            <div class="font-bold text-white text-sm">${escapeHtml(d.manga_title)}</div>
                            <div class="text-xs text-zinc-400">${escapeHtml(d.chapter_title)}</div>
                        </td>
                        <td class="px-5 py-3.5">
                            <span class="px-2 py-0.5 rounded text-[11px] font-mono bg-zinc-800 text-zinc-300 border border-zinc-700 uppercase">
                                ${escapeHtml(d.format)}
                            </span>
                        </td>
                        <td class="px-5 py-3.5">${progressDisplay}</td>
                        <td class="px-5 py-3.5">${statusBadge}</td>
                        <td class="px-5 py-3.5 text-right space-x-2">
                            ${d.status === 'Failed' ? `<button type="button" data-action="retry-download" data-download-id="${escapeHtml(d.id)}" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-white rounded text-[11px] transition">${escapeHtml(t('downloads.retry_failed'))}</button>` : ''}
                            <button type="button" data-action="remove-download" data-download-id="${escapeHtml(d.id)}" class="text-zinc-500 hover:text-red-400 text-xs transition">${escapeHtml(t('common.delete'))}</button>
                        </td>
                    </tr>
                `;
            }).join('');
        }

        async function retryAllFailedDownloads() {
            const btn = document.getElementById('btn-retry-failed');
            if (btn) btn.disabled = true;
            try {
                const res = await fetch('/api/downloads/retry-failed', { method: 'POST' });
                const json = await res.json();
                if (json.success) {
                    loadDownloads();
                } else {
                    alert(t('common.error') + ': ' + (json.error || t('downloads.retry_error')));
                }
            } catch (e) {
                alert(t('common.error') + ': ' + e);
            } finally {
                if (btn) btn.disabled = false;
            }
        }

        async function retryDownload(id) {
            try {
                await fetch('/api/downloads/' + encodeURIComponent(id) + '/retry', { method: 'POST' });
                loadDownloads();
            } catch (e) {
                alert(t('common.error') + ': ' + e);
            }
        }

        async function removeDownload(id) {
            try {
                await fetch('/api/downloads/' + encodeURIComponent(id), { method: 'DELETE' });
                loadDownloads();
            } catch (e) {
                alert(t('common.error') + ': ' + e);
            }
        }

        async function confirmAddFavorite() {
            if (!currentInspectResult) return;
            const btn = document.getElementById('btn-add-favorite');
            btn.disabled = true;
            btn.textContent = t('common.loading');

            try {
                const res = await fetch('/api/favorites', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        module_id: currentInspectResult.module_id,
                        url: currentInspectResult.url,
                        title: currentInspectResult.title,
                        cover_url: currentInspectResult.cover_url,
                        author: currentInspectResult.authors,
                        genres: currentInspectResult.genres,
                        description: currentInspectResult.summary
                    })
                });

                if (!res.ok) throw new Error(t("common.error"));
                alert(t("favorites.added_success", { title: currentInspectResult.title }));
                switchTab('favorites');
            } catch (e) {
                alert(t("favorites.add_error") + e.message);
            } finally {
                btn.disabled = false;
                btn.innerHTML = '<span>⭐ </span><span data-i18n="search.btn_add_fav">' + escapeHtml(t("search.btn_add_fav")) + '</span>';
            }
        }

        function escapeHtml(str) {
            if (!str) return '';
            return String(str)
                .replace(/&/g, "&amp;")
                .replace(/</g, "&lt;")
                .replace(/>/g, "&gt;")
                .replace(/"/g, "&quot;")
                .replace(/'/g, "&#39;");
        }

        function safeHttpUrl(rawUrl) {
            if (!rawUrl || typeof rawUrl !== 'string') return '#';
            try {
                const parsed = new URL(rawUrl, window.location.origin);
                if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
                    return parsed.href;
                }
            } catch (_) {}
            return '#';
        }

        function proxyCover(url) {
            if (!url) return '';
            return BASE_PATH + '/api/proxy/image?url=' + encodeURIComponent(url);
        }

        function connectWs() {
            clearTimeout(wsReconnectTimer);
            wsReconnectTimer = null;
            if (!currentUser || (demoMode && demoEnded)) return;
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            ws = new WebSocket(`${protocol}//${window.location.host}${BASE_PATH}/api/ws`);

            ws.onopen = () => {
                const badge = document.getElementById('ws-badge');
                if (badge) {
                    badge.className = 'inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-950 text-emerald-400 border border-emerald-800';
                    badge.innerHTML = '<span class="w-1.5 h-1.5 mr-1.5 rounded-full bg-emerald-400 animate-pulse"></span> ' + t('nav.ws_connected');
                }
            };

            ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    if (data.event === 'favorite_added' || data.event === 'favorite_removed' || data.event === 'favorite_updated' || data.event === 'favorite_settings_updated') {
                        loadFavorites();
                    } else if (data.event.startsWith('download_')) {
                        loadDownloads();
                    } else if (data.event === 'catalog_sync_started') {
                        activeSyncInfo = {
                            module_id: data.payload.module_id,
                            module_name: data.payload.module_name,
                            current_page: 0,
                            total_pages: null,
                            items_found: 0
                        };
                        updateSyncBanner(activeSyncInfo);
                        updateSyncButton();
                    } else if (data.event === 'catalog_sync_progress') {
                        activeSyncInfo = {
                            module_id: data.payload.module_id,
                            module_name: data.payload.module_name,
                            current_page: data.payload.current_page,
                            total_pages: data.payload.total_pages,
                            items_found: data.payload.items_found
                        };
                        updateSyncBanner(activeSyncInfo);
                        if (catalogModule === data.payload.module_id || catalogModule === 'all') {
                            loadCatalog(false);
                        }
                    } else if (data.event === 'catalog_sync_completed') {
                        activeSyncInfo = null;
                        updateSyncBanner(null);
                        loadCatalogStatus();
                        loadCatalog(false);
                    } else if (data.event === 'catalog_sync_failed') {
                        activeSyncInfo = null;
                        updateSyncBanner(null);
                        loadCatalogStatus();
                        alert(t("catalog.sync_error") + data.payload.error);
                    }
                } catch(e) {}
            };

            ws.onclose = () => {
                const badge = document.getElementById('ws-badge');
                if (badge) {
                    badge.className = 'inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium bg-amber-950 text-amber-400 border border-amber-800';
                    badge.innerHTML = '<span class="w-1.5 h-1.5 mr-1.5 rounded-full bg-amber-400"></span> ' + t('nav.ws_reconnecting');
                }
                if (currentUser && !(demoMode && demoEnded)) {
                    clearTimeout(wsReconnectTimer);
                    wsReconnectTimer = setTimeout(connectWs, 3000);
                }
            };
        }

        // Browser History & Navigation Support
        window.addEventListener('popstate', (e) => {
            const modal = document.getElementById('modal-alternative-sources');
            if (modal && !modal.classList.contains('hidden')) {
                closeAlternativeSourcesModal();
                return;
            }

            let targetTab = 'favorites';
            if (e.state && e.state.tab && VALID_TABS.includes(e.state.tab)) {
                targetTab = e.state.tab;
            } else if (window.location.hash) {
                const cleanHash = window.location.hash.replace(/^#/, '').split('?')[0];
                if (VALID_TABS.includes(cleanHash)) {
                    targetTab = cleanHash;
                }
            }

            switchTab(targetTab, false);
        });

        window.addEventListener('hashchange', () => {
            if (window.location.hash) {
                const cleanHash = window.location.hash.replace(/^#/, '').split('?')[0];
                if (VALID_TABS.includes(cleanHash) && cleanHash !== currentActiveTab) {
                    switchTab(cleanHash, false);
                }
            }
        });

        function initRouting() {
            let initialTab = 'favorites';
            if (window.location.hash) {
                const cleanHash = window.location.hash.replace(/^#/, '').split('?')[0];
                if (VALID_TABS.includes(cleanHash)) {
                    initialTab = cleanHash;
                }
            }

            try {
                history.replaceState({ tab: initialTab }, '', '#' + initialTab);
            } catch (e) {}

            switchTab(initialTab, false);
        }

        // ==========================================
        // AUTHENTICATION & RBAC
        // ==========================================

        // Global fetch interceptor to catch 401 Unauthorized responses
        function withBasePath(url) {
            if (BASE_PATH && typeof url === 'string' && url.startsWith('/') && !url.startsWith('//')) {
                return BASE_PATH + url;
            }
            return url;
        }

        const originalFetch = window.fetch;
        window.fetch = async function(...args) {
            args[0] = withBasePath(args[0]);
            const response = await originalFetch.apply(this, args);
            if (response.status === 401) {
                const url = (args[0] && args[0].url) || (typeof args[0] === 'string' ? args[0] : '');
                if (!url.includes('/api/auth/login') && !url.includes('/api/auth/status') && !url.includes('/api/auth/setup')) {
                    handleSessionExpired();
                }
            }
            return response;
        };

        function handleSessionExpired() {
            if (demoMode) {
                if (currentUser) endDemo(true);
                return;
            }
            currentUser = null;
            currentPermissions = null;
            if (ws) {
                try { ws.close(); } catch (e) {}
            }
            showAuthGate('login', t('auth.session_expired'));
        }

        function showAuthGate(screen, errorMsg = '') {
            const authGate = document.getElementById('auth-gate-container');
            const mainApp = document.getElementById('main-app-container');
            const screenSetup = document.getElementById('screen-setup-wizard');
            const screenLogin = document.getElementById('screen-login');

            if (mainApp) {
                mainApp.classList.add('hidden');
                mainApp.classList.remove('flex');
                mainApp.style.display = 'none';
            }
            if (authGate) {
                authGate.classList.remove('hidden');
                authGate.classList.add('flex');
                authGate.style.display = 'flex';
            }

            populateLanguageSelectors();
            applyTranslations();

            if (screen === 'setup') {
                if (screenSetup) {
                    screenSetup.classList.remove('hidden');
                    screenSetup.style.display = 'block';
                }
                if (screenLogin) {
                    screenLogin.classList.add('hidden');
                    screenLogin.style.display = 'none';
                }
                const errEl = document.getElementById('setup-error');
                if (errEl) {
                    if (errorMsg) {
                        errEl.textContent = errorMsg;
                        errEl.classList.remove('hidden');
                    } else {
                        errEl.classList.add('hidden');
                    }
                }
            } else {
                if (screenSetup) {
                    screenSetup.classList.add('hidden');
                    screenSetup.style.display = 'none';
                }
                if (screenLogin) {
                    screenLogin.classList.remove('hidden');
                    screenLogin.style.display = 'block';
                }
                const errEl = document.getElementById('login-error');
                if (errEl) {
                    if (errorMsg) {
                        errEl.textContent = errorMsg;
                        errEl.classList.remove('hidden');
                    } else {
                        errEl.classList.add('hidden');
                    }
                }
            }
        }

        function showMainApp(user) {
            const authGate = document.getElementById('auth-gate-container');
            const mainApp = document.getElementById('main-app-container');

            if (authGate) {
                authGate.classList.add('hidden');
                authGate.classList.remove('flex');
                authGate.style.display = 'none';
            }
            if (mainApp) {
                mainApp.classList.remove('hidden');
                mainApp.classList.add('flex');
                mainApp.style.display = 'flex';
            }

            updateUserNav(user);
            populateLanguageSelectors();
            applyTranslations();
            const usersBtn = document.getElementById('tab-btn-users');
            if (usersBtn) {
                if (user && user.role === 'admin') {
                    usersBtn.classList.remove('hidden');
                } else {
                    usersBtn.classList.add('hidden');
                }
                if (demoMode) {
                    ['btn-dl-queue', 'btn-queue-merged', 'btn-retry-failed', 'cache-policy-settings'].forEach(id => {
                        const element = document.getElementById(id);
                        if (element) element.style.display = 'none';
                    });
                }
            }
        }

        async function checkAuthStatus() {
            try {
                const res = await fetch('/api/auth/status');
                const data = await res.json();
                if (data.setup_needed) {
                    showAuthGate('setup');
                    return false;
                }

                if (data.authenticated && data.user) {
                    currentUser = data.user;
                    currentPermissions = data.permissions;
                    if (demoMode) demoSetActive();
                    showMainApp(data.user);
                    return true;
                } else {
                    currentUser = null;
                    currentPermissions = null;
                    showAuthGate('login');
                    return false;
                }
            } catch (e) {
                console.error("Error checking auth:", e);
                showAuthGate('login', t('auth.server_unreachable'));
                return false;
            }
        }

        async function submitSetupWizard(event) {
            event.preventDefault();
            const username = document.getElementById('setup-username').value.trim();
            const password = document.getElementById('setup-password').value.trim();
            const confirm = document.getElementById('setup-confirm-password').value.trim();
            const errEl = document.getElementById('setup-error');
            if (errEl) errEl.classList.add('hidden');

            if (password !== confirm) {
                if (errEl) {
                    errEl.textContent = t('auth.password_mismatch');
                    errEl.classList.remove('hidden');
                }
                return;
            }

            try {
                const res = await fetch('/api/auth/setup', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username, password })
                });
                const data = await res.json();
                if (!res.ok) {
                    if (errEl) {
                        errEl.textContent = data.error || t('auth.setup_error');
                        errEl.classList.remove('hidden');
                    }
                    return;
                }

                currentUser = data.user;
                currentPermissions = data.permissions;
                showMainApp(data.user);
                loadAppData();
            } catch (e) {
                if (errEl) {
                    errEl.textContent = t('common.error') + ': ' + e;
                    errEl.classList.remove('hidden');
                }
            }
        }

        async function submitLogin(event) {
            event.preventDefault();
            const username = document.getElementById('login-username').value.trim();
            const password = document.getElementById('login-password').value.trim();
            const errEl = document.getElementById('login-error');
            if (errEl) errEl.classList.add('hidden');

            try {
                const res = await fetch('/api/auth/login', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ username, password })
                });
                const data = await res.json();
                if (!res.ok) {
                    if (errEl) {
                        errEl.textContent = data.error || t('auth.invalid_credentials');
                        errEl.classList.remove('hidden');
                    }
                    return;
                }

                currentUser = data.user;
                currentPermissions = data.permissions;
                showMainApp(data.user);
                if (demoMode) {
                    demoLastActivity = 0;
                    demoSetActive();
                    const dialog = document.getElementById('demo-dialog');
                    if (dialog && !dialog.open) {
                        document.getElementById('tab-btn-favorites')?.focus();
                        dialog.showModal();
                    }
                }
                loadAppData();
            } catch (e) {
                if (errEl) {
                    errEl.textContent = t('common.error') + ': ' + e;
                    errEl.classList.remove('hidden');
                }
            }
        }

        async function logout() {
            try {
                const res = await fetch('/api/auth/logout', { method: 'POST' });
                if (!res.ok) throw new Error(await res.text());
            } catch (e) {
                console.error('Logout error:', e);
                if (demoMode) {
                    alert(t('demo.logout_error'));
                    return;
                }
            }
            if (demoMode) {
                endDemo(false);
                return;
            }
            currentUser = null;
            currentPermissions = null;
            if (ws) {
                try {
                    ws.onclose = null;
                    ws.close();
                } catch (e) {}
            }
            const usersBtn = document.getElementById('tab-btn-users');
            if (usersBtn) usersBtn.classList.add('hidden');
            if (currentActiveTab === 'users') {
                currentActiveTab = 'favorites';
            }
            showAuthGate('login');
            try {
                history.replaceState({ tab: 'login' }, '', BASE_PATH + '/');
            } catch (e) {}
        }
        window.logout = logout;

        function updateUserNav(user) {
            const badge = document.getElementById('user-info-badge');
            const nameEl = document.getElementById('nav-username');
            const roleEl = document.getElementById('nav-role-badge');

            if (user) {
                if (badge) {
                    badge.classList.remove('hidden');
                    badge.classList.add('flex');
                }
                if (nameEl) nameEl.textContent = user.username;
                if (roleEl) {
                    roleEl.textContent = user.role === 'admin' ? t('users.role_admin') : t('users.role_user');
                    if (user.role === 'admin') {
                        roleEl.className = 'px-1.5 py-0.5 rounded text-[10px] font-mono font-bold uppercase bg-red-950 text-red-400 border border-red-800';
                    } else {
                        roleEl.className = 'px-1.5 py-0.5 rounded text-[10px] font-mono font-bold uppercase bg-blue-950 text-blue-400 border border-blue-800';
                    }
                }
            } else {
                if (badge) {
                    badge.classList.add('hidden');
                    badge.classList.remove('flex');
                }
            }
        }

        // ==========================================
        // USER MANAGEMENT (ADMIN)
        // ==========================================

        async function loadUsers() {
            const tbody = document.getElementById('users-table-body');
            if (!tbody) return;

            try {
                const res = await fetch('/api/admin/users');
                if (!res.ok) {
                    tbody.innerHTML = '<tr><td colspan="6" class="p-8 text-center text-red-400">' + escapeHtml(t('common.admin_only')) + '</td></tr>';
                    return;
                }
                allUsersList = await res.json();
                renderUsersTable(allUsersList);
            } catch (e) {
                tbody.innerHTML = '<tr><td colspan="6" class="p-8 text-center text-red-400">' + escapeHtml(t('common.error')) + ': ' + escapeHtml(String(e)) + '</td></tr>';
            }
        }

        function renderUsersTable(users) {
            const tbody = document.getElementById('users-table-body');
            if (!tbody) return;

            if (users.length === 0) {
                tbody.innerHTML = '<tr><td colspan="6" class="p-8 text-center text-zinc-500">' + escapeHtml(t('users.empty')) + '</td></tr>';
                return;
            }

            let html = '';
            for (const item of users) {
                const u = item.user;
                const p = item.permissions || {};
                const isAdmin = u.role === 'admin';
                const roleBadge = isAdmin
                    ? `<span class="px-2 py-0.5 rounded-full text-xs font-mono font-bold bg-red-950 text-red-400 border border-red-800">${escapeHtml(t('users.role_admin'))}</span>`
                    : `<span class="px-2 py-0.5 rounded-full text-xs font-mono font-medium bg-blue-950 text-blue-400 border border-blue-800">${escapeHtml(t('users.role_user'))}</span>`;

                const statusBadge = u.is_active
                    ? `<span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium bg-emerald-950 text-emerald-400 border border-emerald-800"><span class="w-1.5 h-1.5 rounded-full bg-emerald-400"></span>${escapeHtml(t('users.status_active'))}</span>`
                    : `<span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium bg-zinc-800 text-zinc-400 border border-zinc-700">${escapeHtml(t('users.status_inactive'))}</span>`;

                const permTags = [
                    p.can_download ? `<span class="text-emerald-400" title="${escapeHtml(t('users.perm_download'))}">📥</span>` : `<span class="text-zinc-600 line-through" title="${escapeHtml(t('users.perm_download'))}">📥</span>`,
                    p.can_manage_favorites ? `<span class="text-amber-400" title="${escapeHtml(t('users.perm_favorites'))}">⭐</span>` : `<span class="text-zinc-600 line-through" title="${escapeHtml(t('users.perm_favorites'))}">⭐</span>`,
                    p.can_trigger_catalog_sync ? `<span class="text-blue-400" title="${escapeHtml(t('users.perm_sync'))}">🔄</span>` : `<span class="text-zinc-600 line-through" title="${escapeHtml(t('users.perm_sync'))}">🔄</span>`,
                    p.can_force_refresh ? `<span class="text-purple-400" title="${escapeHtml(t('users.perm_force_refresh'))}">⚡</span>` : `<span class="text-zinc-600 line-through" title="${escapeHtml(t('users.perm_force_refresh'))}">⚡</span>`,
                    p.can_view_modules ? `<span class="text-cyan-400" title="${escapeHtml(t('users.perm_view_modules'))}">🧩</span>` : `<span class="text-zinc-600 line-through" title="${escapeHtml(t('users.perm_view_modules'))}">🧩</span>`,
                ].join(' ');

                const dateStr = u.created_at ? new Date(u.created_at).toLocaleDateString() : '-';

                html += `
                <tr class="hover:bg-zinc-800/40 transition">
                    <td class="p-4 font-semibold text-white flex items-center gap-2">
                        <div class="w-8 h-8 rounded-full bg-zinc-800 flex items-center justify-center font-bold text-xs text-zinc-300">
                            ${escapeHtml(u.username.substring(0, 2).toUpperCase())}
                        </div>
                        <div>
                            <div>${escapeHtml(u.username)}</div>
                            <div class="text-[11px] text-zinc-500 font-mono">${escapeHtml(u.id.substring(0, 8))}...</div>
                        </div>
                    </td>
                    <td class="p-4">${roleBadge}</td>
                    <td class="p-4">${statusBadge}</td>
                    <td class="p-4 font-mono text-sm">${permTags}</td>
                    <td class="p-4 text-zinc-400 text-xs">${dateStr}</td>
                    <td class="p-4 text-right space-x-2">
                        <button type="button" data-action="edit-user" data-user-id="${escapeHtml(u.id)}" class="px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded text-xs transition">
                            ${escapeHtml(t('common.edit'))}
                        </button>
                        <button type="button" data-action="delete-user" data-user-id="${escapeHtml(u.id)}" data-username="${escapeHtml(u.username)}" class="px-2.5 py-1 bg-red-950/60 hover:bg-red-900 text-red-400 rounded text-xs border border-red-900 transition">
                            ${escapeHtml(t('common.delete'))}
                        </button>
                    </td>
                </tr>`;
            }
            tbody.innerHTML = html;
        }

        function openCreateUserModal() {
            document.getElementById('user-modal-title').innerHTML = '<span>👤</span> <span id="user-modal-title-text" data-i18n="users.modal_add_title">' + escapeHtml(t('users.modal_add_title')) + '</span>';
            document.getElementById('user-form-id').value = '';
            document.getElementById('user-form-username').value = '';
            document.getElementById('user-form-username').disabled = false;
            document.getElementById('user-form-password').value = '';
            document.getElementById('user-form-password').required = true;
            document.getElementById('user-pwd-hint').textContent = t('users.pwd_hint_create');
            document.getElementById('user-form-role').value = 'user';
            document.getElementById('user-form-active').value = '1';

            document.getElementById('perm-download').checked = true;
            document.getElementById('perm-favorites').checked = true;
            document.getElementById('perm-sync').checked = false;
            document.getElementById('perm-force-refresh').checked = false;
            document.getElementById('perm-view-modules').checked = true;

            document.getElementById('user-modal').classList.remove('hidden');
        }

        function openEditUserModal(userId) {
            const item = allUsersList.find(x => x.user.id === userId);
            if (!item) return;
            const u = item.user;
            const p = item.permissions || {};

            document.getElementById('user-modal-title').innerHTML = '<span>✏️</span> <span>' + escapeHtml(t('users.modal_edit_title')) + ' — ' + escapeHtml(u.username) + '</span>';
            document.getElementById('user-form-id').value = u.id;
            document.getElementById('user-form-username').value = u.username;
            document.getElementById('user-form-username').disabled = true;
            document.getElementById('user-form-password').value = '';
            document.getElementById('user-form-password').required = false;
            document.getElementById('user-pwd-hint').textContent = t('users.pwd_hint_edit');
            document.getElementById('user-form-role').value = u.role;
            document.getElementById('user-form-active').value = u.is_active ? '1' : '0';

            document.getElementById('perm-download').checked = !!p.can_download;
            document.getElementById('perm-favorites').checked = !!p.can_manage_favorites;
            document.getElementById('perm-sync').checked = !!p.can_trigger_catalog_sync;
            document.getElementById('perm-force-refresh').checked = !!p.can_force_refresh;
            document.getElementById('perm-view-modules').checked = !!p.can_view_modules;

            document.getElementById('user-modal').classList.remove('hidden');
        }

        function closeUserModal() {
            document.getElementById('user-modal').classList.add('hidden');
        }

        async function saveUserForm(event) {
            event.preventDefault();
            const id = document.getElementById('user-form-id').value;
            const isEdit = !!id;
            const username = document.getElementById('user-form-username').value.trim();
            const password = document.getElementById('user-form-password').value.trim();
            const role = document.getElementById('user-form-role').value;
            const is_active = document.getElementById('user-form-active').value === '1';

            const permissions = {
                user_id: id || '',
                can_download: document.getElementById('perm-download').checked,
                can_manage_favorites: document.getElementById('perm-favorites').checked,
                can_trigger_catalog_sync: document.getElementById('perm-sync').checked,
                can_force_refresh: document.getElementById('perm-force-refresh').checked,
                can_view_modules: document.getElementById('perm-view-modules').checked,
            };

            try {
                if (isEdit) {
                    const payload = { role, is_active, permissions };
                    if (password) payload.password = password;
                    const res = await fetch(`/api/admin/users/${encodeURIComponent(id)}`, {
                        method: 'PUT',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(payload)
                    });
                    if (!res.ok) {
                        const err = await res.text();
                        alert(t('common.error') + ': ' + err);
                        return;
                    }
                } else {
                    const payload = { username, password, role, permissions };
                    const res = await fetch('/api/admin/users', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(payload)
                    });
                    if (!res.ok) {
                        const err = await res.text();
                        alert(t('common.error') + ': ' + err);
                        return;
                    }
                }

                closeUserModal();
                loadUsers();
            } catch (e) {
                alert(t('common.error') + ': ' + e);
            }
        }

        async function deleteUser(userId, username) {
            if (!confirm(t('users.confirm_delete', { username: username }))) {
                return;
            }

            try {
                const res = await fetch(`/api/admin/users/${encodeURIComponent(userId)}`, {
                    method: 'DELETE'
                });
                if (!res.ok) {
                    const err = await res.text();
                    alert(t('common.error') + ': ' + err);
                    return;
                }
                loadUsers();
            } catch (e) {
                alert(t('common.error') + ': ' + e);
            }
        }

        // ==========================================
        // CACHE POLICIES (ADMIN)
        // ==========================================
        function populateCachePolicyModules(modules) {
            const sel = document.getElementById('cache-policy-module');
            if (sel) {
                let opts = '<option value="*">* ' + escapeHtml(t('settings.cache_module_global')) + '</option>';
                modules.forEach(m => {
                    opts += `<option value="${escapeHtml(m.id)}">${escapeHtml(m.name)} (${escapeHtml(m.id)})</option>`;
                });
                sel.innerHTML = opts;
            }
        }

        async function loadCachePolicies() {
            const tbody = document.getElementById('cache-policies-tbody');
            if (!tbody) return;

            try {
                const res = await fetch('/api/admin/cache/policies');
                if (!res.ok) {
                    tbody.innerHTML = '<tr><td colspan="4" class="py-4 text-center text-zinc-500">' + escapeHtml(t('common.admin_only')) + '</td></tr>';
                    return;
                }
                const policies = await res.json();
                allCachePolicies = policies;
                renderCachePolicies(policies);
            } catch (e) {
                tbody.innerHTML = '<tr><td colspan="4" class="py-4 text-center text-red-400">' + escapeHtml(t('common.error')) + ': ' + escapeHtml(String(e)) + '</td></tr>';
            }
        }

        function renderCachePolicies(policies) {
            const tbody = document.getElementById('cache-policies-tbody');
            if (!tbody) return;

            if (policies.length === 0) {
                tbody.innerHTML = '<tr><td colspan="4" class="py-4 text-center text-zinc-500">' + escapeHtml(t('settings.cache_empty')) + '</td></tr>';
                return;
            }

            let html = '';
            for (const p of policies) {
                const isGlobal = p.module_id === '*';
                const label = isGlobal ? '<span class="text-amber-400 font-bold">* ' + escapeHtml(t('settings.cache_module_global')) + '</span>' : `<span class="text-white">${escapeHtml(p.module_id)}</span>`;
                const unitKey = 'settings.unit_' + p.ttl_unit;
                const unitLabel = t(unitKey) !== unitKey ? t(unitKey) : p.ttl_unit;

                html += `
                <tr class="hover:bg-zinc-800/30">
                    <td class="py-2.5 px-3">${label}</td>
                    <td class="py-2.5 px-3 text-emerald-400">${p.ttl_value} ${escapeHtml(unitLabel)}</td>
                    <td class="py-2.5 px-3 text-cyan-400">${p.min_delay_ms} ms</td>
                    <td class="py-2.5 px-3 text-right">
                        <button type="button" data-action="edit-cache-policy" data-module-id="${escapeHtml(p.module_id)}" data-ttl-value="${Number(p.ttl_value) || 0}" data-ttl-unit="${escapeHtml(p.ttl_unit)}" data-min-delay-ms="${Number(p.min_delay_ms) || 0}" class="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 rounded text-[11px]">
                            ${escapeHtml(t('common.edit'))}
                        </button>
                    </td>
                </tr>`;
            }
            tbody.innerHTML = html;
        }

        function editCachePolicy(moduleId, val, unit, delay) {
            const modSel = document.getElementById('cache-policy-module');
            if (modSel) modSel.value = moduleId;
            const valInput = document.getElementById('cache-policy-ttl-val');
            if (valInput) valInput.value = val;
            const unitSel = document.getElementById('cache-policy-ttl-unit');
            if (unitSel) unitSel.value = unit;
            const delayInput = document.getElementById('cache-policy-delay');
            if (delayInput) delayInput.value = delay;
        }

        async function saveCachePolicy() {
            const module_id = document.getElementById('cache-policy-module').value;
            const ttl_value = parseInt(document.getElementById('cache-policy-ttl-val').value) || 24;
            const ttl_unit = document.getElementById('cache-policy-ttl-unit').value;
            const min_delay_ms = parseInt(document.getElementById('cache-policy-delay').value) || 1000;

            try {
                const res = await fetch('/api/admin/cache/policies', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ module_id, ttl_value, ttl_unit, min_delay_ms })
                });

                if (!res.ok) {
                    const err = await res.text();
                    alert(t('common.error') + ': ' + err);
                    return;
                }

                loadCachePolicies();
            } catch (e) {
                alert(t('common.error') + ': ' + e);
            }
        }

        
        // ==========================================
        // DEDICATED MERGER VIEW LOGIC
        // ==========================================
        let currentMergerPlan = null;

        async function searchMergerAlternatives() {
            const input = document.getElementById('merger-title-input');
            const title = input ? input.value.trim() : '';
            if (!title) {
                alert(t('merger.canonical_placeholder'));
                return;
            }

            const btn = document.getElementById('btn-merger-search');
            if (btn) {
                btn.disabled = true;
                btn.innerHTML = '<span>⏳</span> ' + escapeHtml(t('common.loading'));
            }

            try {
                const res = await fetch('/api/manga/alternatives?q=' + encodeURIComponent(title));
                if (!res.ok) throw new Error(await res.text());
                const alts = await res.json();

                const box = document.getElementById('merger-sources-box');
                const primSel = document.getElementById('merger-primary-select');
                const secBox = document.getElementById('merger-secondary-checkboxes');

                if (alts.length === 0) {
                    alert(t('merger.no_matches'));
                    if (box) box.classList.add('hidden');
                    return;
                }

                if (primSel) {
                    primSel.innerHTML = alts.map(a => `<option value="${escapeHtml(a.module_id)}|${escapeHtml(a.url)}">${escapeHtml(a.title)} [${escapeHtml(a.module_name || a.module_id)}]</option>`).join('');
                }

                if (secBox) {
                    secBox.innerHTML = alts.map((a, i) => `
                        <label class="flex items-center gap-2 p-1 hover:bg-zinc-900 rounded cursor-pointer">
                            <input type="checkbox" class="merger-sec-cb rounded bg-zinc-900 border-zinc-700 text-amber-500 focus:ring-0" value="${escapeHtml(a.module_id)}|${escapeHtml(a.url)}" ${i > 0 ? 'checked' : ''}>
                            <span class="truncate">${escapeHtml(a.title)} <span class="text-zinc-500 font-mono text-[10px]">(${escapeHtml(a.module_name || a.module_id)})</span></span>
                        </label>
                    `).join('');
                }

                if (box) box.classList.remove('hidden');
            } catch (e) {
                alert(t('common.error') + ': ' + e.message);
            } finally {
                if (btn) {
                    btn.disabled = false;
                    btn.innerHTML = '<span>🔍</span> ' + escapeHtml(t('merger.btn_search_sources'));
                }
            }
        }

        async function executeMergeAnalysis() {
            const primSel = document.getElementById('merger-primary-select');
            if (!primSel || !primSel.value) return;
            const [pMod, pUrl] = primSel.value.split('|');

            const secCbs = Array.from(document.querySelectorAll('.merger-sec-cb:checked')).map(cb => cb.value);
            if (secCbs.length === 0) {
                alert(t('merger.alert_enter_alt_url'));
                return;
            }

            const btn = document.getElementById('btn-execute-merge');
            if (btn) {
                btn.disabled = true;
                btn.innerHTML = '<span>⚡</span> ' + escapeHtml(t('common.loading'));
            }

            try {
                // Fetch primary chapters first
                const pRes = await fetch('/api/manga/fetch', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ module_id: pMod, url: pUrl })
                });
                if (!pRes.ok) throw new Error(await pRes.text());
                const pData = await pRes.json();

                const [sMod, sUrl] = secCbs[0].split('|');
                const mergeRes = await fetch('/api/manga/merge', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        primary: {
                            module_id: pMod,
                            url: pUrl,
                            chapters: pData.chapters.map(c => ({ title: c.title, url: c.url }))
                        },
                        secondary: {
                            module_id: sMod,
                            url: sUrl
                        }
                    })
                });
                if (!mergeRes.ok) throw new Error(await mergeRes.text());
                const mergeData = await mergeRes.json();
                currentMergerPlan = {
                    title: pData.title,
                    cover_url: pData.cover_url,
                    primary_module: pMod,
                    primary_url: pUrl,
                    chapters: mergeData.chapters
                };

                const resContainer = document.getElementById('merger-results');
                if (resContainer) resContainer.classList.remove('hidden');

                const tbody = document.getElementById('merger-chapters-tbody');
                if (tbody) {
                    tbody.innerHTML = mergeData.chapters.map((ch, idx) => `
                        <tr class="hover:bg-zinc-800/30">
                            <td class="p-3 text-center text-zinc-500 font-mono">${idx + 1}</td>
                            <td class="p-3 font-semibold text-white">${escapeHtml(ch.title)}</td>
                            <td class="p-3">
                                <span class="px-2 py-0.5 rounded text-[11px] font-mono ${ch.is_gap_fill ? 'bg-blue-950 text-blue-300 border border-blue-800' : 'bg-emerald-950 text-emerald-400 border border-emerald-800'}">
                                    ${escapeHtml(ch.selected_source.module_name || ch.selected_source.module_id)}
                                </span>
                            </td>
                            <td class="p-3 text-zinc-400 text-xs">${ch.is_gap_fill ? escapeHtml(t('merger.tag_smart_hybrid')) : escapeHtml(t('merger.label_primary'))}</td>
                            <td class="p-3 text-right text-zinc-500 text-xs">${ch.available_sources ? ch.available_sources.length : 1}</td>
                        </tr>
                    `).join('');
                }

                alert(t('merger.merge_success', { total: mergeData.chapters.length, filled: mergeData.chapters.filter(c => c.is_gap_fill).length }));
            } catch (e) {
                alert(t('merger.merge_error') + e.message);
            } finally {
                if (btn) {
                    btn.disabled = false;
                    btn.innerHTML = '<span>⚡</span> ' + escapeHtml(t('merger.btn_analyze'));
                }
            }
        }

        async function queueAllMergedChapters() {
            if (!currentMergerPlan || !currentMergerPlan.chapters) return;
            const fmt = document.getElementById('merger-format-select') ? document.getElementById('merger-format-select').value : 'Cbz';
            const btn = document.getElementById('btn-queue-merged');
            if (btn) btn.disabled = true;

            try {
                const chaptersToQueue = currentMergerPlan.chapters.map(ch => ({
                    title: ch.title,
                    url: ch.selected_source.chapter_url,
                    module_id: ch.selected_source.module_id
                }));

                const res = await fetch('/api/downloads/queue', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        module_id: currentMergerPlan.primary_module,
                        manga_title: currentMergerPlan.title,
                        manga_url: currentMergerPlan.primary_url,
                        cover_url: currentMergerPlan.cover_url,
                        format: fmt,
                        chapters: chaptersToQueue
                    })
                });
                if (!res.ok) throw new Error(await res.text());
                const data = await res.json();
                alert(t('downloads.queued_success', { count: data.queued, format: fmt }));
                switchTab('downloads');
            } catch (e) {
                alert(t('downloads.queue_error') + e.message);
            } finally {
                if (btn) btn.disabled = false;
            }
        }

        // Init & Data Loading
        let appDataLoaded = false;
        function loadAppData() {
            loadModules();
            loadFavorites();
            loadDownloads();
            loadCatalogStatus();
            loadSettings();
            if (!ws || ws.readyState > 1) {
                connectWs();
            }
            if (!appDataLoaded) {
                appDataLoaded = true;
                initRouting();
            }
        }

        let delegatedHandlersInitialized = false;
        function setupDelegatedEventHandlers() {
            if (delegatedHandlersInitialized) return;
            delegatedHandlersInitialized = true;

            const bind = (id, event, handler) => {
                const el = document.getElementById(id);
                if (el && typeof el.addEventListener === 'function') {
                    el.addEventListener(event, handler);
                }
            };

            bind('favorites-list', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action]') : null;
                if (!target) return;
                const action = target.dataset.action;
                if (action === 'switch-tab') switchTab(target.dataset.tab);
                else if (action === 'toggle-auto-dl') toggleAutoDownload(target.dataset.mangaId, target.dataset.autoDl === 'true');
                else if (action === 'check-fav') checkSingleFavorite(target.dataset.mangaId);
                else if (action === 'open-fav-dl') openFavoriteForDownload(target.dataset.mangaId, target.dataset.url);
                else if (action === 'delete-fav') deleteFavorite(target.dataset.mangaId);
            });

            bind('favorites-list', 'change', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action="update-fav-format"]') : null;
                if (target) updateFavoriteFormat(target.dataset.mangaId, target.value);
            });

            bind('modules-list', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action]') : null;
                if (!target) return;
                if (target.dataset.action === 'open-catalog') openCatalogForModule(target.dataset.moduleId);
                else if (target.dataset.action === 'select-module-search') selectModuleForSearch(target.dataset.moduleId);
            });

            bind('catalog-table-body', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action="open-manga-direct"]') : null;
                if (target) openMangaDirect(target.dataset.moduleId, target.dataset.url);
            });

            bind('alt-sources-list', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action="merge-with-source"]') : null;
                if (target) mergeWithSource(target.dataset.moduleId, target.dataset.url, target.dataset.moduleName);
            });

            bind('chapters-list-container', 'change', (e) => {
                if (e.target && e.target.classList && e.target.classList.contains('chapter-cb')) {
                    updateSelectedCount();
                }
            });

            bind('chapters-list-container', 'click', (e) => {
                if (e.target && e.target.closest && e.target.closest('.chapter-external-link')) {
                    e.stopPropagation();
                }
            });

            bind('downloads-table-body', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action]') : null;
                if (!target) return;
                if (target.dataset.action === 'retry-download') retryDownload(target.dataset.downloadId);
                else if (target.dataset.action === 'remove-download') removeDownload(target.dataset.downloadId);
            });

            bind('users-table-body', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action]') : null;
                if (!target) return;
                if (target.dataset.action === 'edit-user') openEditUserModal(target.dataset.userId);
                else if (target.dataset.action === 'delete-user') deleteUser(target.dataset.userId, target.dataset.username);
            });

            bind('cache-policies-tbody', 'click', (e) => {
                const target = e.target && e.target.closest ? e.target.closest('[data-action="edit-cache-policy"]') : null;
                if (target) {
                    editCachePolicy(
                        target.dataset.moduleId,
                        Number(target.dataset.ttlValue) || 0,
                        target.dataset.ttlUnit,
                        Number(target.dataset.minDelayMs) || 0
                    );
                }
            });
        }

        async function initApp() {
            setupDelegatedEventHandlers();
            initI18n();
            let demoWasActive = false;
            try {
                const res = await fetch('/api/demo/info');
                if (!res.ok) throw new Error(await res.text());
                demoMode = (await res.json()).demo === true;
            } catch (e) {
                console.error('Demo mode detection failed:', e);
            }
            if (demoMode) {
                try {
                    demoEnded = sessionStorage.getItem('fmd_demo_ended') === '1';
                    demoWasActive = sessionStorage.getItem('fmd_demo_active') === '1';
                } catch (e) {}
                const info = document.getElementById('demo-login-info');
                if (info) info.classList.remove('hidden');
                checkDemoSession();
            }
            const isAuth = await checkAuthStatus();
            if (isAuth) {
                if (demoMode) demoSetActive();
                loadAppData();
            } else if (demoMode) {
                if (demoWasActive) endDemo(true);
                else if (demoEnded) endDemo(false);
                else pollDemoReservation();
            }
        }

        initApp();
"#;

pub fn render(base_path: &str) -> String {
    format!(
        "const BASE_PATH = {};\n{}",
        serde_json::to_string(base_path).expect("serializing a string cannot fail"),
        CLIENT_SCRIPT
    )
}
