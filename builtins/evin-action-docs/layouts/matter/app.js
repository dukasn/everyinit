'use strict';

const App = (() => {
    // 1. DOM & UTILS
    const $ = (s, c = document) => c.querySelector(s);
    const $$ = (s, c = document) => [...c.querySelectorAll(s)];
    const root = document.documentElement, body = document.body;
    const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
    const escape = s => s ? String(s).replace(/[&<>"']/g, c => ESC[c]) : '';

    // 2. STATE & THEME
    let theme = localStorage.getItem('m3-docs-theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
    let drawerOpen = localStorage.getItem('m3-docs-persistent-drawer') !== 'closed';
    let navState = JSON.parse(localStorage.getItem('m3-docs-nav-groups') || '{}');
    let searchIdx = -1, searchData = null, promptTimer = null, restoreTarget = null, currentMode = null;

    const setTheme = t => {
        root.setAttribute('data-theme', theme = t);
        localStorage.setItem('m3-docs-theme', t);
        const i = $('#themeIcon');
        if (i) i.textContent = t === 'dark' ? 'light_mode' : 'dark_mode';
    };

    const isModal = () => ['compact', 'medium'].includes(root.dataset.windowSize);

    const updateTopbarMessageOverflow = () => {
        $$('.docs-topbar, .evin-topbar').forEach(tb => {
            const message = $('.topbar-message', tb);
            const track = $('.topbar-message-track', tb);
            if (!message || !track) return;
            tb.classList.remove('has-overflowing-message');
            tb.style.removeProperty('--topbar-message-scroll-distance');
            const distance = track.scrollWidth - message.clientWidth;
            if (distance > 1) {
                tb.style.setProperty('--topbar-message-scroll-distance', `-${distance}px`);
                tb.classList.add('has-overflowing-message');
            }
        });
    };

    const setDrawer = (open, restore = false) => {
        drawerOpen = !!open;
        const modal = isModal();
        const layout = $('#appLayout');
        layout?.classList.toggle('sidebar-open', drawerOpen);
        layout?.classList.toggle('sidebar-closed', !modal && !drawerOpen);
        body.classList.toggle('modal-drawer-open', modal && drawerOpen);
        $('#sidebarToggle')?.setAttribute('aria-expanded', String(drawerOpen));
        $('#sidebar')?.setAttribute('aria-hidden', String(!drawerOpen));
        if (drawerOpen) $('#sidebar')?.removeAttribute('inert');
        else $('#sidebar')?.setAttribute('inert', '');

        if (!modal && body.dataset.pageKind !== 'start') {
            localStorage.setItem('m3-docs-persistent-drawer', drawerOpen ? 'open' : 'closed');
        }

        if (modal && drawerOpen) {
            restoreTarget = document.activeElement;
            setTimeout(() => $('#sidebar')?.querySelector('[aria-current="page"], a.nav-item, button:not([disabled])')?.focus(), 320);
        } else if (restore && restoreTarget) {
            restoreTarget.focus();
            restoreTarget = null;
        }
    };

    // 3. ZERO-COPY BINARY ENGINE (MPHF + ELIAS-FANO)
    const fnv1a = (s, seed = 0) => {
        let h = (2166136261 ^ seed) >>> 0;
        for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619) >>> 0;
        return h;
    };

    let binIndex = null, loadingBin = null;
    const utf8 = new TextDecoder();

    const loadBinaryIndex = (depth) => {
        if (binIndex) return Promise.resolve(binIndex);
        if (loadingBin) return loadingBin;
        return loadingBin = (async () => {
            let buf = null;
            const script = $('#docs-search-bin');
            if (script) {
                const b64 = atob(script.textContent.trim());
                const bytes = new Uint8Array(b64.length);
                for (let i = 0; i < b64.length; i++) bytes[i] = b64.charCodeAt(i);
                buf = bytes.buffer;
            } else {
                try {
                    const r = await fetch(`${depth}assets/search.bin`);
                    if (r.ok) buf = await r.arrayBuffer();
                } catch (_) {}
            }
            if (!buf || buf.byteLength < 32) return null;
            const v = new DataView(buf);
            if (v.getUint32(0, 1) !== 0x4E495645) return null;

            const [docCount, termCount, docsOff, mphfOff, postOff, strOff] = [8, 12, 16, 20, 24, 28].map(o => v.getUint32(o, 1));
            const bucketCount = v.getUint32(mphfOff, 1);
            const disp = new Uint16Array(buf, mphfOff + 4, bucketCount);
            const fpOff = (mphfOff + 4 + bucketCount * 2 + 3) & ~3;
            const fps = new Uint8Array(buf, fpOff, termCount);
            const termTableOff = (fpOff + termCount + 3) & ~3;
            const postTableOff = termTableOff + termCount * 8;
            const postOffsets = new Uint32Array(buf, postTableOff, termCount);

            const getStr = (o, len) => len && strOff + o + len <= buf.byteLength ? utf8.decode(new Uint8Array(buf, strOff + o, len)) : '';

            const getDoc = (id) => {
                const o = docsOff + id * 40;
                if (o + 40 > buf.byteLength) return ['', '', '', '', '', '', 0];
                const fields = [0, 6, 12, 18, 24, 30].map(p => getStr(v.getUint32(o + p, 1), v.getUint16(o + p + 4, 1)));
                fields.push(v.getUint8(o + 36));
                return fields;
            };

            const getTermStr = (slot) => {
                const o = termTableOff + slot * 8;
                return o + 8 <= buf.byteLength ? getStr(v.getUint32(o, 1), v.getUint16(o + 4, 1)) : '';
            };

            const getPostings = (slot) => {
                if (slot >= termCount) return [];
                const base = postOff + postOffsets[slot];
                if (base + 6 > buf.byteLength) return [];
                const count = v.getUint16(base, 1);
                if (!count) return [];

                const [lowWidth, lowLen, highLen] = [v.getUint8(base + 2), v.getUint8(base + 3), v.getUint16(base + 4, 1)];
                if (base + 6 + lowLen + highLen + count > buf.byteLength) return [];

                const lowBytes = new Uint8Array(buf, base + 6, lowLen);
                const highBytes = new Uint8Array(buf, base + 6 + lowLen, highLen);
                const weights = new Uint8Array(buf, base + 6 + lowLen + highLen, count);

                const getLow = (i) => {
                    if (!lowWidth) return 0;
                    let val = 0, start = i * lowWidth;
                    for (let b = 0; b < lowWidth; b++) {
                        const cur = start + b;
                        if (lowBytes[cur >> 3] & (1 << (7 - (cur & 7)))) val |= (1 << (lowWidth - 1 - b));
                    }
                    return val;
                };

                let curHigh = 0, bitIdx = 0, read = 0;
                const res = [];
                while (read < count && (bitIdx >> 3) < highBytes.byteLength) {
                    const isSet = (highBytes[bitIdx >> 3] & (1 << (7 - (bitIdx & 7)))) !== 0;
                    bitIdx++;
                    if (isSet) {
                        const docId = (curHigh << lowWidth) | getLow(read);
                        const w = weights[read++];
                        res.push([docId, (w >> 4) & 15, w & 15]);
                    } else curHigh++;
                }
                return res;
            };

            return binIndex = {
                getDoc,
                getPostings,
                lookup: (t) => {
                    if (!termCount) return null;
                    const h0 = fnv1a(t, 0), b = (h0 >>> 0) % bucketCount;
                    const slot = (fnv1a(t, disp[b]) >>> 0) % termCount;
                    return (fps[slot] === (h0 & 255) && getTermStr(slot) === t) ? slot : null;
                },
                scanPrefix: (p) => {
                    const slots = [];
                    for (let s = 0; s < termCount; s++) if (getTermStr(s).startsWith(p)) slots.push(s);
                    return slots;
                }
            };
        })();
    };

    // 4. SEARCH RENDERING & SCORING
    const searchInput = () => $('#docs-search-input, #evin-search-input');
    const searchResults = () => $('#docs-search-results, #evin-search-results');
    const ICONS = ['description', 'code', 'heading'];
    let promptIdx = 0;

    const cyclePrompt = () => {
        const i = searchInput();
        if (!i || i.value) return;
        const p = searchData?.prompts || ['Search documentation...'];
        i.placeholder = matchMedia('(max-width: 639px)').matches
            ? 'Search documentation...'
            : (p[promptIdx % p.length] || 'Search documentation...');
    };

    const managePrompt = (start) => {
        clearInterval(promptTimer);
        const i = searchInput();
        if (start && (!i || !i.value)) {
            promptIdx = 0;
            cyclePrompt();
            promptTimer = setInterval(() => {
                promptIdx = (promptIdx + 1) % (searchData?.prompts?.length || 1);
                cyclePrompt();
            }, 3000);
        }
    };

    const highlightText = (text, terms) => {
        if (!text || !terms.length) return escape(text);
        const esc = terms.map(t => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
        return escape(text).replace(new RegExp(`(${esc.join('|')})`, 'gi'), '<mark class="search-highlight">$1</mark>');
    };

    const extractSnippet = (content, terms) => {
        if (!content) return '';
        const lower = content.toLowerCase();
        let idx = -1;
        for (const t of terms) {
            const p = lower.indexOf(t);
            if (~p && (!~idx || p < idx)) idx = p;
        }
        if (!~idx) return highlightText(content.slice(0, 100) + (content.length > 100 ? '...' : ''), terms);
        const s = Math.max(0, idx - 35), e = Math.min(content.length, idx + 65);
        return (s ? '...' : '') + highlightText(content.slice(s, e), terms) + (e < content.length ? '...' : '');
    };

    const SEARCH_WEIGHTS = Object.freeze({
        EXACT_REFERENCE: 240,
        PREFIX_REFERENCE: 120,
        EXACT_TITLE: 120,
        PREFIX_TITLE: 60,
        CONTENT_HIT: 8,
    });

    const calcDocScore = (flags, freq) => {
        let s = 0;
        if (flags & 1) s += SEARCH_WEIGHTS.EXACT_REFERENCE;
        if (flags & 2) s += SEARCH_WEIGHTS.EXACT_TITLE;
        if (flags & 4) s += SEARCH_WEIGHTS.PREFIX_TITLE;
        if (flags & 8) s += Math.min(freq * SEARCH_WEIGHTS.CONTENT_HIT, 40);
        return s;
    };

    const performSearch = async () => {
        const input = searchInput(), results = searchResults();
        if (!input || !results) return;
        const query = input.value.trim().toLowerCase();
        if (!query) {
            results.innerHTML = '';
            searchIdx = -1;
            return CSS.highlights?.delete('search-highlight');
        }

        const depth = $('meta[name="docs-depth-prefix"], meta[name="evin-depth-prefix"]')?.content || '';
        const bin = binIndex || await loadBinaryIndex(depth);
        const terms = query.split(/\s+/).filter(Boolean);
        const docScores = new Map();

        // Universal Search Postings Resolution
        const getPostingsForTerm = (t) => {
            const hits = new Map();
            if (bin) {
                const exact = bin.lookup(t);
                if (exact !== null) for (const [d, fl, fr] of bin.getPostings(exact)) hits.set(d, [fl, fr]);
                if (t.length >= 1) {
                    for (const s of bin.scanPrefix(t)) {
                        if (s !== exact) for (const [d, fl, fr] of bin.getPostings(s)) if (!hits.has(d)) hits.set(d, [fl | 4, fr]);
                    }
                }
            } else if (searchData?.index) {
                if (searchData.index[t]) for (const [d, fl, fr] of searchData.index[t]) hits.set(d, [fl, fr]);
                if (t.length >= 1) {
                    for (const k in searchData.index) {
                        if (k !== t && k.startsWith(t)) for (const [d, fl, fr] of searchData.index[k]) if (!hits.has(d)) hits.set(d, [fl | 4, fr]);
                    }
                }
            }
            return hits;
        };

        const resolveDoc = (docId) => {
            if (bin) return bin.getDoc(docId);
            if (searchData?.docs) {
                const [slug, pTitle, sTitle, anchor, reference_id, snippetId, kind] = searchData.docs[docId];
                return [slug, pTitle, sTitle, anchor, reference_id, searchData.snippets?.[snippetId] || '', kind];
            }
            return ['', '', '', '', '', '', 0];
        };

        for (const t of terms) {
            for (const [docId, [flags, freq]] of getPostingsForTerm(t).entries()) {
                const entry = docScores.get(docId) || { score: 0, hits: 0 };
                entry.score += calcDocScore(flags, freq);
                entry.hits += 1;
                docScores.set(docId, entry);
            }
        }

        const scored = [...docScores.entries()]
            .filter(([_, e]) => e.hits === terms.length)
            .sort((a, b) => b[1].score - a[1].score)
            .slice(0, 10);

        if (!scored.length) {
            searchIdx = -1;
            results.innerHTML = '<div class="search-empty-state">No matches found</div>';
            return CSS.highlights?.delete('search-highlight');
        }

        searchIdx = 0;
        results.innerHTML = scored.map(([docId], idx) => {
            const [slug, page_title, section_title, anchor, reference_id, snippet, kind] = resolveDoc(docId);
            const href = `${depth}${slug === 'index' ? 'index.html' : slug + '/index.html'}${anchor ? '#' + anchor : ''}`;
            const title = anchor && section_title !== page_title ? section_title : page_title;
            const source = title === page_title ? '' : `<span class="search-result-source">${escape(page_title)}</span>`;
            return `<button class="dialog-item search-result-card ${idx ? '' : 'selected'}" data-href="${href}">
                <div class="search-result-left"><svg class="docs-icon" aria-hidden="true"><use href="#icon-${ICONS[kind] || 'description'}"></use></svg></div>
                <div class="search-result-body">
                    <div class="search-result-header">
                        <span class="search-result-title">${highlightText(title, terms)}</span>
                    </div>
                    ${(reference_id || source) ? `<div class="search-result-meta">${reference_id ? `<span class="m3-reference-badge">${highlightText(reference_id, terms)}</span>` : ''}${source}</div>` : ''}
                    <div class="search-result-snippet">${extractSnippet(snippet, terms)}</div>
                </div>
            </button>`;
        }).join('');

        if ('highlights' in CSS) {
            const ranges = [];
            $$('.search-result-card', results).forEach(card => {
                const w = document.createTreeWalker(card, NodeFilter.SHOW_TEXT);
                let n;
                while ((n = w.nextNode())) {
                    const text = n.textContent.toLowerCase();
                    terms.forEach(t => {
                        let pos = 0;
                        while ((pos = text.indexOf(t, pos)) !== -1) {
                            const r = new Range();
                            r.setStart(n, pos);
                            r.setEnd(n, pos + t.length);
                            ranges.push(r);
                            pos += t.length;
                        }
                    });
                }
            });
            ranges.length ? CSS.highlights.set('search-highlight', new Highlight(...ranges)) : CSS.highlights.delete('search-highlight');
        }
    };

    const updateSearchSelection = () => {
        const res = searchResults();
        if (!res) return;
        $$('.search-result-card', res).forEach((el, i) => {
            el.classList.toggle('selected', i === searchIdx);
            if (i === searchIdx) el.scrollIntoView({ block: 'nearest' });
        });
    };

    // 5. ACTIONS & KEY KERNEL
    const ACTIONS = {
        'theme:toggle': () => setTheme(theme === 'light' ? 'dark' : 'light'),
        'sidebar:toggle': () => setDrawer(drawerOpen ^= 1, isModal() && drawerOpen),
        'sidebar:close': () => setDrawer(0, true),
        'search:open': () => $('#searchModal')?.showModal(),
        'search:close': () => $('#searchModal')?.close(),
        'topbar:dismiss': (t) => {
            const tb = t.closest('.docs-topbar, .evin-topbar');
            if (tb) {
                localStorage.setItem(`docs:topbar:${tb.dataset.topbarScope}:${tb.dataset.topbarId}`, 'dismissed');
                tb.classList.add('is-hidden');
            }
        },
        'nav:toggle': (t) => {
            const group = t.closest('.nav-group')?.dataset.navGroup || t.closest('.nav-subgroup')?.dataset.navSubgroup;
            const list = document.getElementById(t.getAttribute('aria-controls'));
            if (!list) return;
            const next = t.getAttribute('aria-expanded') !== 'true';
            t.setAttribute('aria-expanded', String(next));
            list.hidden = !next;
            if (group) {
                navState[group] = next;
                localStorage.setItem('m3-docs-nav-groups', JSON.stringify(navState));
            }
        }
    };

    const KEY_LUT = {
        'escape': () => { if (isModal() && drawerOpen) setDrawer(0, true); return false; },
        'k': (e) => (e.ctrlKey || e.metaKey) && ACTIONS['search:open'](),
        't': (e) => e.altKey && ACTIONS['theme:toggle'](),
        'arrowdown': (e, inSearch) => {
            if (!inSearch) return false;
            e.preventDefault();
            const len = $$('.search-result-card').length;
            if (len) { searchIdx = (searchIdx + 1) % len; updateSearchSelection(); }
        },
        'arrowup': (e, inSearch) => {
            if (!inSearch) return false;
            e.preventDefault();
            const len = $$('.search-result-card').length;
            if (len) { searchIdx = (searchIdx - 1 + len) % len; updateSearchSelection(); }
        },
        'enter': (e, inSearch) => {
            if (inSearch && ~searchIdx) {
                e.preventDefault();
                $$('.search-result-card')[searchIdx]?.click();
            }
        },
        'tab': (e) => {
            if (!isModal() || !drawerOpen) return false;
            const sidebar = $('#sidebar');
            const focusable = $$('a[href]:not([hidden]), button:not([disabled]):not([hidden]), [tabindex]:not([tabindex="-1"]):not([hidden])', sidebar).filter(el => el.getClientRects().length);
            if (!focusable.length) return (e.preventDefault(), sidebar?.focus());
            const first = focusable[0], last = focusable[focusable.length - 1];
            if (!sidebar?.contains(document.activeElement)) return (e.preventDefault(), (e.shiftKey ? last : first).focus());
            if (e.shiftKey && (document.activeElement === first || document.activeElement === sidebar)) return (e.preventDefault(), last.focus());
            if (!e.shiftKey && (document.activeElement === last || document.activeElement === sidebar)) return (e.preventDefault(), first.focus());
        }
    };

    // 6. GLOBAL INITIALIZATION
    const init = () => {
        setTheme(theme);
        matchMedia('(prefers-color-scheme: dark)').addEventListener('change', e => {
            if (!localStorage.getItem('m3-docs-theme')) setTheme(e.matches ? 'dark' : 'light');
        });

        $$('.docs-topbar, .evin-topbar').forEach(tb => {
            if (localStorage.getItem(`docs:topbar:${tb.dataset.topbarScope}:${tb.dataset.topbarId}`) === 'dismissed') {
                tb.classList.add('is-hidden');
            }
        });

        const applyWindow = () => {
            const w = getComputedStyle(root).getPropertyValue('--ui-layout-window-width-class').trim() || 'compact';
            root.dataset.windowSize = w;
            const nextMode = isModal() ? 'modal' : 'persistent';
            if (nextMode !== currentMode) {
                currentMode = root.dataset.drawerMode = nextMode;
                setDrawer(isModal() ? false : (body.dataset.pageKind !== 'start' ? drawerOpen : false));
            }
        };
        window.addEventListener('resize', () => {
            applyWindow();
            updateTopbarMessageOverflow();
            cyclePrompt();
        }, { passive: true });
        applyWindow();
        requestAnimationFrame(updateTopbarMessageOverflow);
        document.fonts?.ready.then(updateTopbarMessageOverflow);

        $$('.nav-group-toggle, .nav-subgroup-toggle').forEach(t => {
            const group = t.closest('.nav-group')?.dataset.navGroup || t.closest('.nav-subgroup')?.dataset.navSubgroup;
            const list = document.getElementById(t.getAttribute('aria-controls'));
            const isActive = list?.dataset.activeGroup === 'true' || Boolean(list?.querySelector('.nav-item.active, [data-active-group="true"]'));
            const exp = isActive || (group && navState[group] === true);
            t.setAttribute('aria-expanded', String(Boolean(exp)));
            if (list) list.hidden = !exp;
            t.dataset.action = 'nav:toggle';
        });

        let lastY = 0;
        const h = $('.header-toolbar');
        $('main.content')?.addEventListener('scroll', e => {
            const y = e.target.scrollTop;
            h?.classList.toggle('is-scrolled', y > 4);
            if (y <= 4 || y - lastY <= -12) h?.classList.remove('is-hidden');
            else if (y - lastY >= 12 && y > 64) h?.classList.add('is-hidden');
            lastY = y;
        }, { passive: true });

        const tocLinks = $$('.toc-link');
        if (tocLinks.length) {
            const obs = new IntersectionObserver(entries => {
                const hit = entries.find(e => e.isIntersecting);
                if (hit) tocLinks.forEach(l => l.classList.toggle('active', l.hash === `#${hit.target.id}`));
            }, { root: $('main.content'), rootMargin: '-80px 0px -70% 0px' });
            $$('h2[id], h3[id]').forEach(hd => obs.observe(hd));
        }

        const modal = $('#searchModal');
        modal?.addEventListener('toggle', e => {
            const i = searchInput(), res = searchResults();
            if (e.target.open) {
                const depth = $('meta[name="docs-depth-prefix"], meta[name="evin-depth-prefix"]')?.content || '';
                loadBinaryIndex(depth);
                if (!searchData) searchData = JSON.parse($('#docs-search-data, #evin-search-data')?.textContent || '{}');
                if (i) i.value = '';
                if (res) res.innerHTML = '';
                searchIdx = -1;
                CSS.highlights?.delete('search-highlight');
                managePrompt(true);
                setTimeout(() => i?.focus(), 50);
            } else {
                managePrompt(false);
                CSS.highlights?.delete('search-highlight');
            }
        });

        const input = searchInput();
        input?.addEventListener('input', () => {
            performSearch();
            if (!input.value && modal?.open) managePrompt(true);
            else clearInterval(promptTimer);
        });

        document.addEventListener('click', e => {
            const t = e.target.closest('[data-action]');
            if (t && ACTIONS[t.dataset.action]) ACTIONS[t.dataset.action](t, e);
            else if (e.target.closest('.search-result-card')) {
                modal?.close();
                window.location.href = e.target.closest('.search-result-card').dataset.href;
            } else if (e.target.tagName === 'DIALOG') {
                e.target.close();
            } else if (isModal() && e.target.closest('a.nav-item')) {
                setDrawer(0);
            }
        });

        document.addEventListener('keydown', e => {
            const handler = KEY_LUT[e.key.toLowerCase()];
            if (handler && handler(e, modal?.open) !== false) e.preventDefault();
        });

        body.classList.add('no-transition');
        requestAnimationFrame(() => requestAnimationFrame(() => body.classList.remove('no-transition')));
    };

    return { init, ACTIONS };
})();

window.MatterActions = window.DocsActions = window.EvinActions = App.ACTIONS;
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', App.init);
} else {
    App.init();
}
