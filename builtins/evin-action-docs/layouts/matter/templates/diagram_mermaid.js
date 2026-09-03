(function() {
    'use strict';

    function visibleSvg(figure) {
        return figure.querySelector('.mermaid-svg-target svg, .diagram-svg-wrapper svg, .evin-mermaid-svg-target svg, .evin-diagram-svg-wrapper svg');
    }

    window.MatterActions = window.MatterActions || window.DocsActions || window.EvinActions || {};
    window.DocsActions = window.MatterActions;
    window.EvinActions = window.MatterActions;

    window.MatterActions['diagram:copy'] = function(copyBtn) {
        const figure = copyBtn.closest('.diagram-figure, .docs-diagram-figure, .evin-diagram-figure');
        const source = figure && figure.querySelector('.mermaid-source, .evin-mermaid-source');
        const svg = figure && figure.querySelector('.diagram-svg-wrapper svg, .evin-diagram-svg-wrapper svg');
        const text = source ? source.textContent || '' : (svg ? svg.outerHTML : '');
        if (!text) return;

        if (window.MatterActions.copyText) {
            window.MatterActions.copyText(text, copyBtn, 'Copied!');
        }
    };

    window.MatterActions['diagram:download'] = function(downloadBtn) {
        const figure = downloadBtn.closest('.diagram-figure, .docs-diagram-figure, .evin-diagram-figure');
        if (!figure) return;
        const svg = visibleSvg(figure);
        if (!svg) return;

        let filename = downloadBtn.getAttribute('data-filename') || 'diagram.svg';
        if (!filename.endsWith('.svg')) filename += '.svg';
        const blob = new Blob([svg.outerHTML], { type: 'image/svg+xml;charset=utf-8' });
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
    };
})();
