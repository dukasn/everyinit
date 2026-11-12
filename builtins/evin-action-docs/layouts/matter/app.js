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
        $$('.docs-topbar').forEach(tb => {
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

    const utf8 = new TextDecoder();

    let binIndex = null;
    let loadingCatalog = null;
    let loadingIndex = null;

    const loadTier2Index = (depth) => {
        if (loadingIndex) return loadingIndex;
        return loadingIndex = (async () => {
            try {
                const r = await fetch(`${depth}assets/search-index.bin`, { priority: 'low' });
                if (!r.ok) return null;
                const buf = await r.arrayBuffer();
                if (buf.byteLength < 32) return null;
                const v = new DataView(buf);
                if (v.getUint32(0, 1) !== 0x58495645) return null; // "EVIX"

                const termCount = v.getUint32(12, 1);
                const mphfOff = v.getUint32(16, 1);
                const postOff = v.getUint32(20, 1);
                const strOff = v.getUint32(24, 1);

                const bucketCount = v.getUint32(mphfOff, 1);
                const disp = new Uint16Array(buf, mphfOff + 4, bucketCount);
                const fpOff = (mphfOff + 4 + bucketCount * 2 + 3) & ~3;
                const fps = new Uint8Array(buf, fpOff, termCount);
                const termTableOff = (fpOff + termCount + 3) & ~3;
                const postTableOff = termTableOff + termCount * 8;
                const postOffsets = new Uint32Array(buf, postTableOff, termCount);
                const allBytes = new Uint8Array(buf);
                const utf8Encoder = new TextEncoder();

                const eachPosting = (slot, flagMask, cb) => {
                    if (slot >= termCount) return;
                    const base = postOff + postOffsets[slot];
                    if (base + 6 > buf.byteLength) return;
                    const count = v.getUint16(base, 1);
                    if (!count) return;

                    const [lowWidth, lowLen, highLen] = [v.getUint8(base + 2), v.getUint8(base + 3), v.getUint16(base + 4, 1)];
                    if (base + 6 + lowLen + highLen + count > buf.byteLength) return;

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
                    while (read < count && (bitIdx >> 3) < highBytes.byteLength) {
                        const isSet = (highBytes[bitIdx >> 3] & (1 << (7 - (bitIdx & 7)))) !== 0;
                        bitIdx++;
                        if (isSet) {
                            const docId = (curHigh << lowWidth) | getLow(read);
                            const w = weights[read++];
                            cb(docId, ((w >> 4) & 15) | flagMask, w & 15);
                        } else curHigh++;
                    }
                };

                if (binIndex) {
                    binIndex.hasIndex = true;
                    binIndex.eachPosting = eachPosting;
                    binIndex.lookup = (t) => {
                        if (!termCount) return null;
                        const h0 = fnv1a(t, 0), b = (h0 >>> 0) % bucketCount;
                        const slot = (fnv1a(t, disp[b]) >>> 0) % termCount;
                        if (fps[slot] !== (h0 & 255)) return null;

                        const entryOff = termTableOff + slot * 8;
                        const len = v.getUint16(entryOff + 4, 1);
                        const tBytes = utf8Encoder.encode(t);
                        if (len !== tBytes.length) return null;
                        const offset = strOff + v.getUint32(entryOff, 1);
                        for (let i = 0; i < len; i++) {
                            if (allBytes[offset + i] !== tBytes[i]) return null;
                        }
                        return slot;
                    };
                    binIndex.scanPrefix = (p) => {
                        const slots = [];
                        const pBytes = utf8Encoder.encode(p);
                        const pLen = pBytes.length;
                        if (!pLen) return slots;

                        for (let s = 0; s < termCount; s++) {
                            const entryOff = termTableOff + s * 8;
                            const len = v.getUint16(entryOff + 4, 1);
                            if (len < pLen) continue;

                            const offset = strOff + v.getUint32(entryOff, 1);
                            let match = true;
                            for (let i = 0; i < pLen; i++) {
                                if (allBytes[offset + i] !== pBytes[i]) {
                                    match = false;
                                    break;
                                }
                            }
                            if (match) slots.push(s);
                        }
                        return slots;
                    };

                    const input = searchInput();
                    if (input && input.value.trim()) {
                        performSearch();
                    }
                }
            } catch (_) {}
        })();
    };

    const loadBinaryIndex = (depth) => {
        if (binIndex && binIndex.hasIndex) return Promise.resolve(binIndex);

        const script = $('#docs-search-bin');
        if (script) {
            if (binIndex) return Promise.resolve(binIndex);
            if (loadingCatalog) return loadingCatalog;
            return loadingCatalog = (async () => {
                let buf = null;
                const text = script.textContent.trim();
                if (typeof Uint8Array.fromBase64 === 'function') {
                    buf = Uint8Array.fromBase64(text).buffer;
                } else {
                    const b64 = atob(text);
                    const len = b64.length;
                    const bytes = new Uint8Array(len);
                    for (let i = 0; i < len; i++) bytes[i] = b64.charCodeAt(i);
                    buf = bytes.buffer;
                }
                if (!buf || buf.byteLength < 32) return null;
                const v = new DataView(buf);
                if (v.getUint32(0, 1) !== 0x4E495645) return null; // "EVIN"

                const [docCount, termCount, docsOff, mphfOff, postOff, strOff] = [8, 12, 16, 20, 24, 28].map(o => v.getUint32(o, 1));
                const bucketCount = v.getUint32(mphfOff, 1);
                const disp = new Uint16Array(buf, mphfOff + 4, bucketCount);
                const fpOff = (mphfOff + 4 + bucketCount * 2 + 3) & ~3;
                const fps = new Uint8Array(buf, fpOff, termCount);
                const termTableOff = (fpOff + termCount + 3) & ~3;
                const postTableOff = termTableOff + termCount * 8;
                const postOffsets = new Uint32Array(buf, postTableOff, termCount);
                const allBytes = new Uint8Array(buf);
                const utf8Encoder = new TextEncoder();

                const getStr = (o, len) => len && strOff + o + len <= buf.byteLength ? utf8.decode(allBytes.subarray(strOff + o, strOff + o + len)) : '';

                const getDoc = (id) => {
                    const o = docsOff + id * 40;
                    if (o + 40 > buf.byteLength) return ['', '', '', '', '', '', 0];
                    return [
                        getStr(v.getUint32(o, 1), v.getUint16(o + 4, 1)),
                        getStr(v.getUint32(o + 6, 1), v.getUint16(o + 10, 1)),
                        getStr(v.getUint32(o + 12, 1), v.getUint16(o + 16, 1)),
                        getStr(v.getUint32(o + 18, 1), v.getUint16(o + 22, 1)),
                        getStr(v.getUint32(o + 24, 1), v.getUint16(o + 28, 1)),
                        getStr(v.getUint32(o + 30, 1), v.getUint16(o + 34, 1)),
                        v.getUint8(o + 36)
                    ];
                };

                const eachPosting = (slot, flagMask, cb) => {
                    if (slot >= termCount) return;
                    const base = postOff + postOffsets[slot];
                    if (base + 6 > buf.byteLength) return;
                    const count = v.getUint16(base, 1);
                    if (!count) return;

                    const [lowWidth, lowLen, highLen] = [v.getUint8(base + 2), v.getUint8(base + 3), v.getUint16(base + 4, 1)];
                    if (base + 6 + lowLen + highLen + count > buf.byteLength) return;

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
                    while (read < count && (bitIdx >> 3) < highBytes.byteLength) {
                        const isSet = (highBytes[bitIdx >> 3] & (1 << (7 - (bitIdx & 7)))) !== 0;
                        bitIdx++;
                        if (isSet) {
                            const docId = (curHigh << lowWidth) | getLow(read);
                            const w = weights[read++];
                            cb(docId, ((w >> 4) & 15) | flagMask, w & 15);
                        } else curHigh++;
                    }
                };

                return binIndex = {
                    hasIndex: true,
                    docCount,
                    getDoc,
                    eachPosting,
                    lookup: (t) => {
                        if (!termCount) return null;
                        const h0 = fnv1a(t, 0), b = (h0 >>> 0) % bucketCount;
                        const slot = (fnv1a(t, disp[b]) >>> 0) % termCount;
                        if (fps[slot] !== (h0 & 255)) return null;

                        const entryOff = termTableOff + slot * 8;
                        const len = v.getUint16(entryOff + 4, 1);
                        const tBytes = utf8Encoder.encode(t);
                        if (len !== tBytes.length) return null;
                        const offset = strOff + v.getUint32(entryOff, 1);
                        for (let i = 0; i < len; i++) {
                            if (allBytes[offset + i] !== tBytes[i]) return null;
                        }
                        return slot;
                    },
                    scanPrefix: (p) => {
                        const slots = [];
                        const pBytes = utf8Encoder.encode(p);
                        const pLen = pBytes.length;
                        if (!pLen) return slots;

                        for (let s = 0; s < termCount; s++) {
                            const entryOff = termTableOff + s * 8;
                            const len = v.getUint16(entryOff + 4, 1);
                            if (len < pLen) continue;

                            const offset = strOff + v.getUint32(entryOff, 1);
                            let match = true;
                            for (let i = 0; i < pLen; i++) {
                                if (allBytes[offset + i] !== pBytes[i]) {
                                    match = false;
                                    break;
                                }
                            }
                            if (match) slots.push(s);
                        }
                        return slots;
                    }
                };
            })();
        }

        if (binIndex) return Promise.resolve(binIndex);
        if (loadingCatalog) return loadingCatalog;

        return loadingCatalog = (async () => {
            try {
                const r = await fetch(`${depth}assets/search-catalog.bin`);
                if (!r.ok) return null;
                const buf = await r.arrayBuffer();
                if (buf.byteLength < 32) return null;
                const v = new DataView(buf);
                if (v.getUint32(0, 1) !== 0x54435645) return null; // "EVCT"

                const docCount = v.getUint32(8, 1);
                const promptCount = v.getUint32(12, 1);
                const docsOff = v.getUint32(16, 1);
                const promptsOff = v.getUint32(20, 1);
                const strOff = v.getUint32(24, 1);
                const allBytes = new Uint8Array(buf);

                const getStr = (o, len) => len && strOff + o + len <= buf.byteLength ? utf8.decode(allBytes.subarray(strOff + o, strOff + o + len)) : '';

                const getDoc = (id) => {
                    const o = docsOff + id * 40;
                    if (o + 40 > buf.byteLength) return ['', '', '', '', '', '', 0];
                    return [
                        getStr(v.getUint32(o, 1), v.getUint16(o + 4, 1)),
                        getStr(v.getUint32(o + 6, 1), v.getUint16(o + 10, 1)),
                        getStr(v.getUint32(o + 12, 1), v.getUint16(o + 16, 1)),
                        getStr(v.getUint32(o + 18, 1), v.getUint16(o + 22, 1)),
                        getStr(v.getUint32(o + 24, 1), v.getUint16(o + 28, 1)),
                        getStr(v.getUint32(o + 30, 1), v.getUint16(o + 34, 1)),
                        v.getUint8(o + 36)
                    ];
                };

                const prompts = [];
                for (let p = 0; p < promptCount; p++) {
                    const o = promptsOff + p * 8;
                    prompts.push(getStr(v.getUint32(o, 1), v.getUint16(o + 4, 1)));
                }

                const scanCatalogTitles = (term, cb) => {
                    const tLower = term.toLowerCase();
                    for (let id = 0; id < docCount; id++) {
                        const d = getDoc(id);
                        const slug = d[0], pageTitle = d[1], secTitle = d[2], refId = d[4];
                        if (refId && refId.toLowerCase().includes(tLower)) {
                            cb(id, 1, 1);
                        } else if (pageTitle && pageTitle.toLowerCase().includes(tLower)) {
                            cb(id, 2, 1);
                        } else if (secTitle && secTitle.toLowerCase().includes(tLower)) {
                            cb(id, 4, 1);
                        } else if (slug && slug.toLowerCase().includes(tLower)) {
                            cb(id, 8, 1);
                        }
                    }
                };

                binIndex = {
                    hasIndex: false,
                    docCount,
                    getDoc,
                    prompts,
                    scanCatalogTitles,
                    eachPosting: () => {},
                    lookup: () => null,
                    scanPrefix: () => [],
                };

                // Trigger Tier 2 background fetch
                loadTier2Index(depth);

                return binIndex;
            } catch (_) {
                return null;
            }
        })();
    };

    // 4. SEARCH RENDERING & SCORING
    const searchInput = () => $('#docs-search-input');
    const searchResults = () => $('#docs-search-results');
    const ICONS = ['description', 'code', 'heading'];
    let promptIdx = 0;

    const cyclePrompt = () => {
        const i = searchInput();
        if (!i || i.value) return;
        const p = binIndex?.prompts?.length ? binIndex.prompts : (searchData?.prompts || ['Search documentation...']);
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
            const p = binIndex?.prompts?.length ? binIndex.prompts : (searchData?.prompts || ['Search documentation...']);
            promptTimer = setInterval(() => {
                promptIdx = (promptIdx + 1) % (p.length || 1);
                cyclePrompt();
            }, 3000);
        }
    };

    const extractSnippet = (content, terms) => {
        if (!content) return '';
        const lower = content.toLowerCase();
        let idx = -1;
        for (const t of terms) {
            const p = lower.indexOf(t);
            if (~p && (!~idx || p < idx)) idx = p;
        }
        if (!~idx) return content.slice(0, 100) + (content.length > 100 ? '...' : '');
        const s = Math.max(0, idx - 35), e = Math.min(content.length, idx + 65);
        return (s ? '...' : '') + content.slice(s, e) + (e < content.length ? '...' : '');
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

    // Pre-allocated typed arrays for zero-allocation query scoring
    let docCapacity = 0;
    let docScores = null;
    let docHits = null;
    let termVisited = null;
    let touchedDocs = null;
    let termGen = 0;

    const ensureDocCapacity = (n) => {
        if (docCapacity >= n) return;
        docCapacity = Math.max(n, 256);
        docScores = new Int32Array(docCapacity);
        docHits = new Uint8Array(docCapacity);
        termVisited = new Uint32Array(docCapacity);
        touchedDocs = new Uint32Array(docCapacity);
    };

    const performSearch = async () => {
        const input = searchInput(), results = searchResults();
        if (!input || !results) return;
        const query = input.value.trim().toLowerCase();
        if (!query) {
            results.replaceChildren();
            searchIdx = -1;
            return CSS.highlights?.delete('search-highlight');
        }

        const depth = $('meta[name="docs-depth-prefix"]')?.content || '';
        const bin = binIndex || await loadBinaryIndex(depth);
        const terms = query.split(/\s+/).filter(Boolean);
        if (!terms.length) {
            results.replaceChildren();
            searchIdx = -1;
            return CSS.highlights?.delete('search-highlight');
        }

        const totalDocs = bin?.docCount || (searchData?.docs?.length || 0);
        ensureDocCapacity(totalDocs);

        let touchedCount = 0;
        const totalTerms = terms.length;

        for (const t of terms) {
            termGen = (termGen + 1) >>> 0;
            if (termGen === 0) {
                termVisited.fill(0);
                termGen = 1;
            }
            const currentTermGen = termGen;

            const onPosting = (docId, flags, freq) => {
                if (docId >= docCapacity) return;
                if (termVisited[docId] === currentTermGen) return;
                termVisited[docId] = currentTermGen;

                if (docHits[docId] === 0) {
                    touchedDocs[touchedCount++] = docId;
                }
                docHits[docId]++;
                docScores[docId] += calcDocScore(flags, freq);
            };

            if (bin) {
                if (bin.hasIndex) {
                    const exact = bin.lookup(t);
                    if (exact !== null) bin.eachPosting(exact, 0, onPosting);
                    if (t.length >= 1) {
                        for (const s of bin.scanPrefix(t)) {
                            if (s !== exact) bin.eachPosting(s, 4, onPosting);
                        }
                    }
                } else if (bin.scanCatalogTitles) {
                    bin.scanCatalogTitles(t, onPosting);
                }
            } else if (searchData?.index) {
                if (searchData.index[t]) {
                    for (const [d, fl, fr] of searchData.index[t]) onPosting(d, fl, fr);
                }
                if (t.length >= 1) {
                    for (const k in searchData.index) {
                        if (k !== t && k.startsWith(t)) {
                            for (const [d, fl, fr] of searchData.index[k]) onPosting(d, fl | 4, fr);
                        }
                    }
                }
            }
        }

        const resolveDoc = (docId) => {
            if (bin) return bin.getDoc(docId);
            if (searchData?.docs) {
                const [slug, pTitle, sTitle, anchor, reference_id, snippetId, kind] = searchData.docs[docId];
                return [slug, pTitle, sTitle, anchor, reference_id, searchData.snippets?.[snippetId] || '', kind];
            }
            return ['', '', '', '', '', '', 0];
        };

        // Fixed-capacity top-10 selection
        const topK = [];
        for (let i = 0; i < touchedCount; i++) {
            const docId = touchedDocs[i];
            if (docHits[docId] === totalTerms) {
                const score = docScores[docId];
                let inserted = false;
                for (let j = 0; j < topK.length; j++) {
                    if (score > topK[j].score) {
                        topK.splice(j, 0, { docId, score });
                        if (topK.length > 10) topK.pop();
                        inserted = true;
                        break;
                    }
                }
                if (!inserted && topK.length < 10) {
                    topK.push({ docId, score });
                }
            }
            docScores[docId] = 0;
            docHits[docId] = 0;
        }

        if (!topK.length) {
            searchIdx = -1;
            const emptyTmpl = $('#search-empty-tmpl');
            results.replaceChildren(emptyTmpl ? emptyTmpl.content.cloneNode(true) : Object.assign(document.createElement('div'), { className: 'search-empty-state', textContent: 'No matches found' }));
            return CSS.highlights?.delete('search-highlight');
        }

        searchIdx = 0;
        const cardTmpl = $('#search-card-tmpl');
        const fragment = document.createDocumentFragment();

        topK.forEach(({ docId }, idx) => {
            const [slug, page_title, section_title, anchor, reference_id, snippet, kind] = resolveDoc(docId);
            const href = `${depth}${slug === 'index' ? 'index.html' : slug + '/index.html'}${anchor ? '#' + anchor : ''}`;
            const title = anchor && section_title !== page_title ? section_title : page_title;

            if (cardTmpl) {
                const clone = cardTmpl.content.cloneNode(true);
                const card = clone.querySelector('.search-result-card');
                if (idx === 0) card.classList.add('selected');
                card.dataset.href = href;

                const iconUse = card.querySelector('.search-result-left use');
                if (iconUse) iconUse.setAttribute('href', `#icon-${ICONS[kind] || 'description'}`);

                const titleEl = card.querySelector('.search-result-title');
                if (titleEl) titleEl.textContent = title;

                const metaEl = card.querySelector('.search-result-meta');
                if (metaEl) {
                    if (reference_id) {
                        const badge = document.createElement('span');
                        badge.className = 'm3-reference-badge';
                        badge.textContent = reference_id;
                        metaEl.appendChild(badge);
                    }
                    if (title !== page_title) {
                        const source = document.createElement('span');
                        source.className = 'search-result-source';
                        source.textContent = page_title;
                        metaEl.appendChild(source);
                    }
                    if (!reference_id && title === page_title) {
                        metaEl.remove();
                    }
                }

                const snippetEl = card.querySelector('.search-result-snippet');
                if (snippetEl) snippetEl.textContent = extractSnippet(snippet, terms);

                fragment.appendChild(clone);
            }
        });

        results.replaceChildren(fragment);

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
        } else {
            const esc = terms.map(t => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
            const regex = new RegExp(`(${esc.join('|')})`, 'gi');
            $$('.search-result-title, .search-result-snippet, .m3-reference-badge', results).forEach(el => {
                el.innerHTML = escape(el.textContent).replace(regex, '<mark class="search-highlight">$1</mark>');
            });
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
    const ripple = (button, event) => {
        if (!button || button.classList.contains('sidebar-backdrop')) return;
        button.classList.add('ripple-host');
        const circle = document.createElement('span');
        const diameter = Math.max(button.clientWidth, button.clientHeight);
        const radius = diameter / 2;
        const rect = button.getBoundingClientRect();
        const x = event?.clientX ? event.clientX - rect.left : rect.width / 2;
        const y = event?.clientY ? event.clientY - rect.top : rect.height / 2;
        circle.style.width = circle.style.height = `${diameter}px`;
        circle.style.left = `${x - radius}px`;
        circle.style.top = `${y - radius}px`;
        circle.className = 'ripple';
        button.querySelector('.ripple')?.remove();
        button.appendChild(circle);
        setTimeout(() => circle.remove(), 550);
    };

    const ACTIONS = {
        'theme:toggle': () => setTheme(theme === 'light' ? 'dark' : 'light'),
        'sidebar:toggle': () => setDrawer(drawerOpen ^= 1, isModal() && drawerOpen),
        'sidebar:close': () => setDrawer(0, true),
        'search:open': () => $('#searchModal')?.showModal(),
        'search:close': () => $('#searchModal')?.close(),
        'topbar:dismiss': (t) => {
            const tb = t.closest('.docs-topbar');
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
    ACTIONS.ripple = ripple;

    const fallbackCopy = (text, onSuccess) => {
        try {
            const textarea = document.createElement('textarea');
            textarea.value = text;
            textarea.style.position = 'fixed';
            textarea.style.left = '-999999px';
            textarea.style.top = '-999999px';
            document.body.appendChild(textarea);
            textarea.focus();
            textarea.select();
            const successful = document.execCommand('copy');
            document.body.removeChild(textarea);
            if (successful && onSuccess) onSuccess();
        } catch (_) {}
    };

    const copyText = (text, button, successTitle) => {
        if (!text) return;
        const onSuccess = () => {
            if (!button) return;
            button.classList.add('is-copied');
            const prevTitle = button.getAttribute('title');
            button.setAttribute('title', successTitle || 'Copied!');
            setTimeout(() => {
                button.classList.remove('is-copied');
                if (prevTitle) button.setAttribute('title', prevTitle);
                else button.removeAttribute('title');
            }, 2000);
        };

        if (navigator.clipboard && window.isSecureContext) {
            navigator.clipboard.writeText(text).then(onSuccess).catch(() => fallbackCopy(text, onSuccess));
        } else {
            fallbackCopy(text, onSuccess);
        }
    };

    ACTIONS.copyText = copyText;

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

        $$('.docs-topbar').forEach(tb => {
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
        modal?.addEventListener('toggle', async e => {
            const i = searchInput(), res = searchResults();
            if (e.target.open) {
                const depth = $('meta[name="docs-depth-prefix"]')?.content || '';
                if (!binIndex) await loadBinaryIndex(depth);
                if (!searchData) searchData = JSON.parse($('#docs-search-data')?.textContent || '{}');
                if (i) i.value = '';
                if (res) res.replaceChildren();
                searchIdx = -1;
                CSS.highlights?.delete('search-highlight');
                managePrompt(true);
                setTimeout(() => i?.focus(), 50);
            } else {
                managePrompt(false);
                CSS.highlights?.delete('search-highlight');
            }
        });

        // 7. KEYBOARD NAVIGATION & CLOSE TRIGGERS
        // Close search when clicking outside dialog content
        modal?.addEventListener('click', e => {
            const rect = modal.getBoundingClientRect();
            const isInDialog = (rect.top <= e.clientY && e.clientY <= rect.top + rect.height
                && rect.left <= e.clientX && e.clientX <= rect.left + rect.width);
            if (!isInDialog) modal.close();
        });

        const input = searchInput();
        input?.addEventListener('input', () => {
            performSearch();
            if (!input.value && modal?.open) managePrompt(true);
            else clearInterval(promptTimer);
        });

        document.addEventListener('click', e => {
            const t = e.target.closest('[data-action]');
            const rippleTarget = e.target.closest('button:not(.sidebar-backdrop), a, [role="button"]:not(.sidebar-backdrop)');
            if (rippleTarget) ripple(rippleTarget, e);
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

        const depth = $('meta[name="docs-depth-prefix"]')?.content || '';
        const triggerSearchPreload = () => {
            loadBinaryIndex(depth);
        };

        if ('requestIdleCallback' in window) {
            requestIdleCallback(triggerSearchPreload, { timeout: 2500 });
        } else {
            setTimeout(triggerSearchPreload, 2000);
        }

        $('#searchToggle, #docs-search-input, .header-search-btn')?.addEventListener('mouseenter', triggerSearchPreload, { once: true });
        $('#searchToggle, #docs-search-input, .header-search-btn')?.addEventListener('touchstart', triggerSearchPreload, { once: true, passive: true });

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
