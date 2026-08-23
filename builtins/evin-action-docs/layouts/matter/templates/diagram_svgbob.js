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

    if (!window.EvinActions['diagram:copy']) {
        window.EvinActions['diagram:copy'] = function(copyBtn) {
            const fig = copyBtn.closest('.evin-diagram-figure');
            if (!fig) return;

            const svgWrapper = fig.querySelector('.evin-diagram-svg-wrapper') || fig.querySelector('.evin-mermaid-svg-target');
            const pre = fig.querySelector('pre.mermaid');
            const text = pre ? (pre.dataset.originalSource || pre.textContent || '') : (svgWrapper ? (svgWrapper.querySelector('svg')?.outerHTML || svgWrapper.innerHTML) : '');
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
    }

    if (!window.EvinActions['diagram:download']) {
        window.EvinActions['diagram:download'] = function(downloadBtn) {
            const fig = downloadBtn.closest('.evin-diagram-figure');
            if (!fig) return;

            const svgTarget = fig.querySelector('.evin-diagram-svg-wrapper svg') || fig.querySelector('.evin-mermaid-svg-target svg');
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
