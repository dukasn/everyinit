(function() {
    'use strict';

    const copyIconSvg = '<svg class="evin-icon" aria-hidden="true"><use href="#icon-copy"></use></svg>';
    const checkIconSvg = '<svg class="evin-icon" style="color:var(--ui-color-primary)" aria-hidden="true"><use href="#icon-check"></use></svg>';

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
            document.body.appendChild(textarea);
            textarea.select();
            const successful = document.execCommand('copy');
            document.body.removeChild(textarea);
            if (successful) onSuccess();
        } catch (_) {}
    }

    function visibleSvg(figure) {
        return figure.querySelector('.evin-mermaid-svg-target svg') || figure.querySelector('.evin-diagram-svg-wrapper svg');
    }

    window.EvinActions = window.EvinActions || {};

    window.EvinActions['diagram:copy'] = function(copyBtn) {
        const figure = copyBtn.closest('.evin-diagram-figure');
        const source = figure && figure.querySelector('.evin-mermaid-source');
        const svg = figure && figure.querySelector('.evin-diagram-svg-wrapper svg');
        const text = source ? source.textContent || '' : (svg ? svg.outerHTML : '');
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
        const figure = downloadBtn.closest('.evin-diagram-figure');
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
