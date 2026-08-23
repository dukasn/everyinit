(function() {
    'use strict';

    const copyIconSvg = '<svg class="evin-icon" aria-hidden="true"><use href="#icon-copy"></use></svg>';
    const checkIconSvg = '<svg class="evin-icon" style="color:var(--ui-color-primary)" aria-hidden="true"><use href="#icon-check"></use></svg>';

    function getTheme() {
        const docTheme = document.documentElement.getAttribute('data-theme') ||
                         document.body.getAttribute('data-theme');
        if (docTheme === 'dark' || docTheme === 'light') {
            return docTheme;
        }
        return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }

    function getThemeVariables() {
        const cs = getComputedStyle(document.documentElement);
        const isDark = getTheme() === 'dark';

        const nodeBg = cs.getPropertyValue('--ui-diagram-node-bg').trim();
        const nodeText = cs.getPropertyValue('--ui-diagram-node-text').trim();
        const nodeBorder = cs.getPropertyValue('--ui-diagram-node-border').trim();
        const clusterBg = cs.getPropertyValue('--ui-diagram-cluster-bg').trim();
        const clusterBorder = cs.getPropertyValue('--ui-diagram-cluster-border').trim();
        const line = cs.getPropertyValue('--ui-diagram-line').trim();
        const primaryOn = cs.getPropertyValue('--ui-color-on-primary').trim();
        const secondary = cs.getPropertyValue('--ui-color-surface-container-highest').trim();
        const git0 = cs.getPropertyValue('--ui-diagram-git-0').trim();
        const git1 = cs.getPropertyValue('--ui-diagram-git-1').trim();
        const git2 = cs.getPropertyValue('--ui-diagram-git-2').trim();
        const git3 = cs.getPropertyValue('--ui-diagram-git-3').trim();
        const gitLabel = cs.getPropertyValue('--ui-diagram-git-label').trim();

        return {
            darkMode: isDark,
            background: 'transparent',
            primaryColor: nodeBg,
            primaryTextColor: nodeText,
            primaryBorderColor: nodeBorder,
            mainBkg: nodeBg,
            textColor: nodeText,
            nodeBorder: nodeBorder,
            clusterBkg: clusterBg,
            clusterBorder: clusterBorder,
            defaultLinkColor: line,
            titleColor: nodeText,
            edgeLabelBackground: nodeBg,
            lineColor: line,
            actorBkg: nodeBg,
            actorBorder: nodeBorder,
            actorTextColor: nodeText,
            actorLineColor: line,
            signalColor: nodeText,
            signalTextColor: nodeText,
            labelBoxBkgColor: nodeBg,
            labelBoxBorderColor: nodeBorder,
            labelTextColor: nodeText,
            loopTextColor: nodeText,
            noteBkgColor: secondary,
            noteBorderColor: nodeBorder,
            noteTextColor: nodeText,
            activationBkgColor: secondary,
            activationBorderColor: nodeBorder,
            sequenceNumberColor: primaryOn,
            git0: git0,
            git1: git1,
            git2: git2,
            git3: git3,
            gitBranchLabel0: gitLabel,
            gitBranchLabel1: gitLabel,
            gitBranchLabel2: gitLabel,
            gitBranchLabel3: gitLabel,
            gitInv0: git0,
            gitInv1: git1,
            gitInv2: git2,
            gitInv3: git3,
            tagLabelColor: nodeText,
            tagLabelBackground: secondary,
            tagLabelBorder: nodeBorder,
            tagLabelFontSize: '12px',
            commitLabelColor: nodeText,
            commitLabelBackground: nodeBg,
            commitLabelFontSize: '12px',
            fontFamily: 'inherit',
            fontSize: '14px'
        };
    }

    const themeCSS = `
        .node rect, .node circle, .node ellipse, .node polygon, .node path {
            rx: 8px !important;
            ry: 8px !important;
            stroke-width: 1.5px !important;
        }
        .edgePath path, .flowchart-link {
            stroke-width: 1.5px !important;
            stroke-linecap: round !important;
            stroke-linejoin: round !important;
        }
        .cluster rect {
            rx: 12px !important;
            ry: 12px !important;
            stroke-dasharray: 4 4 !important;
        }
        .actor {
            rx: 8px !important;
            ry: 8px !important;
            stroke-width: 1.5px !important;
        }
        .label text, text.actor {
            font-family: inherit !important;
        }
        .note {
            rx: 6px !important;
            ry: 6px !important;
        }
    `;

    function initMermaid() {
        if (typeof window.mermaid === 'undefined') return;
        try {
            window.mermaid.initialize({
                startOnLoad: false,
                theme: 'base',
                themeVariables: getThemeVariables(),
                themeCSS: themeCSS,
                securityLevel: 'loose',
                fontFamily: 'inherit',
                fontSize: 14,
                flowchart: {
                    useMaxWidth: true,
                    htmlLabels: true,
                    curve: 'basis',
                    padding: 16,
                    nodeSpacing: 45,
                    rankSpacing: 45
                },
                sequence: {
                    useMaxWidth: true,
                    showSequenceNumbers: true,
                    actorMargin: 50,
                    boxMargin: 10,
                    boxTextMargin: 5,
                    noteMargin: 10,
                    messageMargin: 35
                },
                state: {
                    useMaxWidth: true,
                    padding: 16
                },
                er: {
                    useMaxWidth: true
                },
                gitGraph: {
                    useMaxWidth: true,
                    mainBranchName: 'main'
                }
            });
        } catch (_) {}
    }

    async function loadMermaidLibrary() {
        if (typeof window.mermaid !== 'undefined') {
            return window.mermaid;
        }
        try {
            const cdnModule = await import('https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs');
            window.mermaid = cdnModule.default || cdnModule;
            return window.mermaid;
        } catch (_) {}
        return null;
    }

    function copyToClipboard(text, onSuccess) {
        if (navigator.clipboard && window.isSecureContext) {
            navigator.clipboard.writeText(text).then(onSuccess).catch(function() {
                fallbackCopy(text, onSuccess);
            });
        } else {
            fallbackCopy(text, onSuccess);
        }
    }

    function fallbackCopy(text, onSuccess) {
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
            if (successful && onSuccess) {
                onSuccess();
            }
        } catch (_) {}
    }

    window.EvinActions = window.EvinActions || {};

    window.EvinActions['diagram:copy'] = function(copyBtn) {
        const fig = copyBtn.closest('.evin-diagram-figure');
        if (!fig) return;

        const pre = fig.querySelector('pre.mermaid');
        const svgWrapper = fig.querySelector('.evin-diagram-svg-wrapper') || fig.querySelector('.evin-mermaid-svg-target');
        const text = pre ? (pre.dataset.originalSource || pre.textContent || '') : (svgWrapper ? svgWrapper.innerHTML : '');
        if (!text) return;

        copyToClipboard(text, function() {
            copyBtn.innerHTML = checkIconSvg;
            copyBtn.setAttribute('title', 'Copied!');
            setTimeout(function() {
                copyBtn.innerHTML = copyIconSvg;
                copyBtn.setAttribute('title', 'Copy diagram code');
            }, 2000);
        });
    };

    window.EvinActions['diagram:download'] = function(downloadBtn) {
        const fig = downloadBtn.closest('.evin-diagram-figure');
        if (!fig) return;

        const svgTarget = fig.querySelector('.evin-mermaid-svg-target svg') || fig.querySelector('.evin-diagram-svg-wrapper svg');
        if (svgTarget) {
            const svgText = svgTarget.outerHTML;
            let filename = downloadBtn.getAttribute('data-filename') || 'diagram.svg';
            if (!filename.endsWith('.svg')) filename += '.svg';
            const blob = new Blob([svgText], { type: 'image/svg+xml;charset=utf-8' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
        } else {
            const pre = fig.querySelector('pre.mermaid');
            const source = pre ? (pre.dataset.originalSource || pre.textContent || '') : '';
            let filename = downloadBtn.getAttribute('data-filename') || 'diagram.mmd';
            if (!filename.endsWith('.mmd')) filename += '.mmd';
            const blob = new Blob([source], { type: 'text/plain;charset=utf-8' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = filename;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
        }
    };

    async function renderDiagrams() {
        const figures = document.querySelectorAll('.evindoc-mermaid');
        if (!figures.length) return;

        const mermaid = await loadMermaidLibrary();
        if (!mermaid) return;

        initMermaid();

        figures.forEach(async function(fig, idx) {
            const pre = fig.querySelector('pre.mermaid');
            if (!pre) return;

            if (!pre.dataset.originalSource) {
                pre.dataset.originalSource = pre.textContent || '';
            }
            const rawSource = pre.dataset.originalSource;
            const container = fig.querySelector('.evin-diagram-container');
            let svgContainer = fig.querySelector('.evin-mermaid-svg-target');

            if (!svgContainer) {
                svgContainer = document.createElement('div');
                svgContainer.className = 'evin-mermaid-svg-target';
                container.insertBefore(svgContainer, pre);
            }

            const renderId = 'mermaid-render-' + idx + '-' + Date.now();
            try {
                const { svg } = await window.mermaid.render(renderId, rawSource);
                svgContainer.innerHTML = svg;
                svgContainer.style.display = 'block';
                pre.style.display = 'none';

                const svgEl = svgContainer.querySelector('svg');
                if (svgEl) {
                    svgEl.style.maxWidth = '100%';
                    svgEl.style.height = 'auto';
                    svgEl.style.display = 'block';
                    svgEl.style.margin = '0 auto';
                }
            } catch (_) {
                svgContainer.style.display = 'none';
                pre.style.display = 'block';
            }
        });
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', renderDiagrams);
    } else {
        renderDiagrams();
    }

    const themeObserver = new MutationObserver(function(mutations) {
        for (const m of mutations) {
            if (m.type === 'attributes' && m.attributeName === 'data-theme') {
                renderDiagrams();
                break;
            }
        }
    });

    themeObserver.observe(document.documentElement, { attributes: true });
    themeObserver.observe(document.body, { attributes: true });
})();
