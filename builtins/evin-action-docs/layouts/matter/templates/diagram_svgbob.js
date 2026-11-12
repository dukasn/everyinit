(function() {
    'use strict';

    window.DocsActions = window.DocsActions || {};

    if (!window.DocsActions['diagram:copy']) {
        window.DocsActions['diagram:copy'] = function(copyBtn) {
            const fig = copyBtn.closest('.diagram-figure, .docs-diagram-figure, .evin-diagram-figure');
            if (!fig) return;

            const svgWrapper = fig.querySelector('.diagram-svg-wrapper, .docs-diagram-svg-wrapper, .mermaid-svg-target, .evin-diagram-svg-wrapper, .evin-mermaid-svg-target');
            const pre = fig.querySelector('pre.mermaid, .mermaid-source');
            const text = pre ? (pre.dataset.originalSource || pre.textContent || '') : (svgWrapper ? (svgWrapper.querySelector('svg')?.outerHTML || svgWrapper.innerHTML) : '');
            if (!text) return;

            if (window.DocsActions.copyText) {
                window.DocsActions.copyText(text, copyBtn, 'Copied!');
            }
        };
    }

    if (!window.DocsActions['diagram:download']) {
        window.DocsActions['diagram:download'] = function(downloadBtn) {
            const fig = downloadBtn.closest('.diagram-figure, .docs-diagram-figure, .evin-diagram-figure');
            if (!fig) return;

            const svgTarget = fig.querySelector('.diagram-svg-wrapper svg, .docs-diagram-svg-wrapper svg, .mermaid-svg-target svg, .evin-diagram-svg-wrapper svg, .evin-mermaid-svg-target svg');
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
            }
        };
    }
})();
