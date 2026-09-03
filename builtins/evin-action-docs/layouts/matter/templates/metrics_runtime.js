(() => {
  if (window.__evinMetricsRuntime) return;
  window.__evinMetricsRuntime = true;

  let report = null;
  const roots = () => [...document.querySelectorAll('[data-metrics-component]')];
  const field = (root, name) => root.querySelector(`[data-metric-field="${name}"]`);
  const set = (root, name, value) => { const target = field(root, name); if (target) target.textContent = value; };
  const bytes = (value) => value >= 1024 * 1024 ? `${(value / 1048576).toFixed(2)} MB` : `${(value / 1024).toFixed(1)} KB`;
  const esc = (value) => String(value).replace(/[&<>"]/g, c => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;' })[c]);

  function renderHero(root) {
    set(root, 'frame0-gzip', (report.frame0.gzip_bytes / 1024).toFixed(2));
    set(root, 'asset-count', `${report.totals.file_count} assets`);
    const limit = report.frame0.budget_limit_bytes ? `${bytes(report.frame0.budget_limit_bytes)} initial paint budget` : 'unbounded budget';
    set(root, 'lead', `${bytes(report.frame0.brotli_bytes)} over Brotli. Cold 2G landing payload remains strictly locked under the ${limit}.`);
  }

  function renderBudget(root) {
    const frame = report.frame0;
    const limit = frame.budget_limit_bytes;
    const percentage = frame.budget_used_pct ?? (limit ? (frame.gzip_bytes / limit * 100) : 0);
    const statusText = report.budget_status === 'pass' ? 'STATUS: PASS' : report.budget_status.replace('_', ' ').toUpperCase();
    set(root, 'percent', `${percentage.toFixed(1)}%`);
    set(root, 'status', statusText);
    const statusEl = field(root, 'status');
    if (statusEl) {
      statusEl.className = `metrics-status-badge ${report.budget_status}`;
    }
    set(root, 'used', `${percentage.toFixed(1)}% used (${bytes(frame.gzip_bytes)} allocated)`);
    set(root, 'cap', limit ? `${bytes(limit)} ceiling (${limit.toLocaleString()} B)` : 'No ceiling configured');
    set(root, 'frame0-gzip', bytes(frame.gzip_bytes));
    set(root, 'frame0-raw', `${bytes(frame.raw_bytes)} raw · ${bytes(frame.brotli_bytes)} Brotli wire transfer`);
    const progress = field(root, 'progress');
    if (progress) {
      progress.style.width = `${Math.min(100, percentage)}%`;
      progress.classList.toggle('is-fail', report.budget_status === 'fail');
    }
  }

  function renderManifest(root) {
    set(root, 'manifest-tag', `Frame-0 Manifest (${report.frame0.files.length} Assets)`);
    set(root, 'status', report.budget_status === 'pass' ? 'STATUS: PASS' : report.budget_status.replace('_', ' ').toUpperCase());
    const statusEl = field(root, 'status');
    if (statusEl) {
      statusEl.className = `metrics-status-badge ${report.budget_status}`;
    }
    const target = field(root, 'manifest');
    if (target) {
      target.innerHTML = report.frame0.files.map(path => {
        const asset = report.assets.find(a => a.path === path);
        let sizeHtml = 'Frame-0';
        if (asset) {
          const savings = asset.brotli_savings_pct ?? (asset.raw_bytes > 0 ? (1 - asset.brotli_bytes / asset.raw_bytes) * 100 : 0);
          const savingsStr = savings > 0 ? `<span class="ratio">(${savings.toFixed(1)}% Br)</span>` : `<span class="faint">(Binary)</span>`;
          sizeHtml = `<span class="num">${bytes(asset.gzip_bytes)}</span> ${savingsStr}`;
        }
        return `<div class="metrics-manifest-row"><span class="metrics-manifest-path">${esc(path)}</span><span class="metrics-manifest-value">${sizeHtml}</span></div>`;
      }).join('') || 'No critical assets.';
    }
  }

  function renderSearch(root) {
    const search = report.search;
    if (!search?.index) {
      set(root, 'efficiency', '—');
      set(root, 'docs', 'Search index not emitted');
      return;
    }
    set(root, 'efficiency', search.index.bits_per_posting.toFixed(2));
    set(root, 'docs', `${search.catalog?.document_count ?? 0} documents indexed · ${search.index.posting_count.toLocaleString()} postings`);
    set(root, 'vocabulary', `Total ${search.index.vocabulary_count.toLocaleString()} terms across ${search.index.bucket_count.toLocaleString()} CHD bucket vectors with zero nursery allocations.`);
    const details = field(root, 'details');
    if (details) {
      const catSavings = search.catalog?.raw_bytes > 0 ? (1 - search.catalog.brotli_bytes / search.catalog.raw_bytes) * 100 : 0;
      const idxSavings = search.index.raw_bytes > 0 ? (1 - search.index.brotli_bytes / search.index.raw_bytes) * 100 : 0;
      details.innerHTML = [
        `<p class="comment">// Tier 1 Metadata Catalog (search-catalog.bin)</p>`,
        `<p><span class="key">document_count</span>: <span class="num">${search.catalog?.document_count ?? 0}</span></p>`,
        `<p><span class="key">prompt_count</span>: <span class="num">${search.catalog?.prompt_count ?? 0}</span></p>`,
        `<p><span class="key">string_pool</span>: <span class="num">${bytes(search.catalog?.string_pool_bytes ?? 0)}</span></p>`,
        `<p><span class="key">wire_transfer</span>: <span class="num">${bytes(search.catalog?.gzip_bytes ?? 0)}</span> <span class="ratio">(${catSavings.toFixed(1)}% Br)</span></p>`,
        `<br>`,
        `<p class="comment">// Tier 2 Inverted Index (search-index.bin)</p>`,
        `<p><span class="key">vocabulary_count</span>: <span class="num">${search.index.vocabulary_count.toLocaleString()}</span> <span class="val">terms</span></p>`,
        `<p><span class="key">mphf_buckets</span>: <span class="num">${search.index.bucket_count.toLocaleString()}</span></p>`,
        `<p><span class="key">elias_fano_low</span>: <span class="num">${bytes(search.index.elias_fano_low_bytes)}</span></p>`,
        `<p><span class="key">elias_fano_high</span>: <span class="num">${bytes(search.index.elias_fano_high_bytes)}</span></p>`,
        `<p><span class="key">wire_transfer</span>: <span class="num">${bytes(search.index.gzip_bytes)}</span> <span class="ratio">(${idxSavings.toFixed(1)}% Br)</span></p>`,
      ].join('');
    }
  }

  function renderTree(root) {
    const tree = report.tree_shaking;
    const density = tree.icons.total_symbols ? (tree.icons.used_symbols / tree.icons.total_symbols * 100) : 0;
    set(root, 'icons', `${tree.icons.used_symbols} / ${tree.icons.total_symbols}`);
    set(root, 'icon-status', `${tree.icons.unused_symbols.length} dead symbols eliminated · ${density.toFixed(1)}% density`);
    set(root, 'font-status', `${tree.fonts.total_fonts} font binaries emitted · ${tree.fonts.preloaded_fonts} preloaded on Frame-0.`);
    set(root, 'density', `${density.toFixed(1)}% DENSITY`);
    const details = field(root, 'details');
    if (details) {
      details.innerHTML = [
        `<p class="comment">// SVG Icon Symbols</p>`,
        `<p><span class="key">active_symbols</span>: <span class="active">${tree.icons.used_symbols} active in markup</span></p>`,
        `<p><span class="key">eliminated_dead</span>: <span class="unused">${tree.icons.unused_symbols.length} pruned</span></p>`,
        tree.icons.unused_symbols.length ? `<p class="comment" style="margin-top:6px;word-break:break-all;">pruned: ${esc(tree.icons.unused_symbols.join(', '))}</p>` : '',
        `<br>`,
        `<p class="comment">// Typography & Font Binaries</p>`,
        `<p><span class="key">emitted_fonts</span>: <span class="num">${tree.fonts.total_fonts}</span> <span class="val">files</span></p>`,
        `<p><span class="key">active_faces</span>: <span class="num">${tree.fonts.active_faces}</span> <span class="val">active @font-face rules</span></p>`,
        `<p><span class="key">preloaded_fonts</span>: <span class="num">${tree.fonts.preloaded_fonts}</span> <span class="val">preloaded</span></p>`,
      ].join('');
    }
  }

  function renderTiming(root) {
    const nav = performance.getEntriesByType('navigation')[0];
    const target = field(root, 'timeline');
    if (!nav || !target) return;
    const values = [
      ['DNS Lookup', nav.domainLookupEnd - nav.domainLookupStart],
      ['TCP Connect', nav.connectEnd - nav.connectStart],
      ['Server TTFB', nav.responseStart - nav.requestStart],
      ['Download', nav.responseEnd - nav.responseStart],
      ['DOM Interactive', nav.domInteractive],
      ['DOM Content Loaded', nav.domContentLoadedEventEnd - nav.domContentLoadedEventStart],
    ];
    const max = Math.max(1, ...values.map(([, value]) => value));
    target.innerHTML = values.map(([name, value]) => `
      <div class="metrics-timing-row">
        <span class="metrics-timing-label">${name}</span>
        <span class="metrics-timing-track"><i class="metrics-timing-fill" style="width:${Math.max(2, value / max * 100)}%"></i></span>
        <span class="metrics-timing-value">${Math.max(0, Math.round(value))}ms</span>
      </div>
    `).join('');
  }

  function category(asset) {
    if (asset.frame0) return 'frame0';
    if (asset.mime.includes('html')) return 'html';
    if (asset.mime.includes('css')) return 'css';
    if (asset.mime.includes('javascript')) return 'js';
    if (asset.mime.includes('font')) return 'fonts';
    return 'bin';
  }

  function renderInventory(root) {
    let filterName = root.dataset.metricsFilter || 'all';
    let query = root.querySelector('[data-metrics-search]')?.value.toLowerCase() || '';
    let sort = root.dataset.metricsSort || 'gzip_bytes';
    let direction = root.dataset.metricsDirection || 'desc';

    const items = report.assets.filter(asset => {
      if (filterName !== 'all' && category(asset) !== filterName) return false;
      return `${asset.path} ${asset.mime}`.toLowerCase().includes(query);
    }).sort((a,b) => {
      let valA = a[sort];
      let valB = b[sort];
      if (sort === 'savings') {
        valA = a.brotli_savings_pct ?? 0;
        valB = b.brotli_savings_pct ?? 0;
      }
      return (valA > valB ? 1 : valA < valB ? -1 : 0) * (direction === 'asc' ? 1 : -1);
    });

    set(root, 'count', `${items.length} of ${report.assets.length} assets`);
    const tbody = root.querySelector('tbody');
    if (tbody) {
      tbody.innerHTML = items.map(asset => {
        const savings = asset.brotli_savings_pct ?? (asset.raw_bytes > 0 ? (1 - asset.brotli_bytes / asset.raw_bytes) * 100 : 0);
        return `
        <tr>
          <td>
            <div style="display:flex;align-items:center;">
              ${asset.frame0 ? '<span class="pill-f0">F0</span>' : ''}
              <code>${esc(asset.path)}</code>
            </div>
          </td>
          <td style="color:var(--ui-color-on-surface-variant);font-size:0.75rem;">${esc(asset.mime)}</td>
          <td>${bytes(asset.raw_bytes)}</td>
          <td>${bytes(asset.gzip_bytes)}</td>
          <td class="cell-brotli">${bytes(asset.brotli_bytes)}</td>
          <td>
            <span class="${savings > 0 ? 'ratio-badge' : 'ratio-flat'}">
              ${savings.toFixed(1)}%
            </span>
          </td>
          <td>${asset.frame0 ? '<span class="metrics-frame0">Frame‑0</span>' : '—'}</td>
        </tr>
      `;
      }).join('');
    }
  }

  function render() {
    if (!report) return;
    roots().forEach(root => {
      switch (root.dataset.metricsComponent) {
        case 'hero': renderHero(root); break;
        case 'budget': renderBudget(root); break;
        case 'manifest': renderManifest(root); break;
        case 'search': renderSearch(root); break;
        case 'tree-shaking': renderTree(root); break;
        case 'timing': renderTiming(root); break;
        case 'inventory': renderInventory(root); break;
      }
    });
  }

  async function refresh() {
    const response = await fetch('/__evin_metrics', { cache: 'no-store' });
    if (!response.ok) throw new Error(`Metrics request failed (${response.status})`);
    report = await response.json();
    render();
  }

  document.addEventListener('click', async event => {
    const action = event.target.closest('[data-metrics-action]');
    if (action) {
      try {
        if (action.dataset.metricsAction === 'refresh') {
          await refresh();
        } else if (report) {
          await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
          roots().filter(root => root.dataset.metricsComponent === 'hero').forEach(root => set(root, 'notice', 'metrics.json copied to clipboard'));
          setTimeout(() => roots().filter(root => root.dataset.metricsComponent === 'hero').forEach(root => set(root, 'notice', '')), 3000);
        }
      } catch (error) {
        roots().filter(root => root.dataset.metricsComponent === 'hero').forEach(root => set(root, 'notice', error.message));
      }
      return;
    }

    const filterButton = event.target.closest('[data-metrics-filter]');
    if (filterButton) {
      const root = filterButton.closest('[data-metrics-component="inventory"]');
      root.dataset.metricsFilter = filterButton.dataset.metricsFilter;
      root.querySelectorAll('[data-metrics-filter]').forEach(button => button.classList.toggle('active', button === filterButton));
      renderInventory(root);
      return;
    }

    const th = event.target.closest('[data-metrics-sort]');
    if (th) {
      const root = th.closest('[data-metrics-component="inventory"]');
      const sort = th.dataset.metricsSort;
      const currentSort = root.dataset.metricsSort || 'gzip_bytes';
      const currentDirection = root.dataset.metricsDirection || 'desc';
      root.dataset.metricsSort = sort;
      root.dataset.metricsDirection = (sort === currentSort && currentDirection === 'desc') ? 'asc' : 'desc';
      renderInventory(root);
    }
  });

  document.addEventListener('input', event => {
    const searchInput = event.target.closest('[data-metrics-search]');
    if (searchInput) {
      const root = searchInput.closest('[data-metrics-component="inventory"]');
      renderInventory(root);
    }
  });

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', refresh);
  } else {
    refresh();
  }
})();
