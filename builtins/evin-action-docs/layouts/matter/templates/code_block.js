(function() {
    'use strict';

    window.DocsActions = window.DocsActions || {};

    if (!window.DocsActions.copyText) {
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
                if (successful && onSuccess) onSuccess();
            } catch (_) {}
        }

        window.DocsActions.copyText = function(text, button, successTitle) {
            if (!text) return;
            const onSuccess = function() {
                if (!button) return;
                button.classList.add('is-copied');
                const prevTitle = button.getAttribute('title');
                button.setAttribute('title', successTitle || 'Copied!');
                setTimeout(function() {
                    button.classList.remove('is-copied');
                    if (prevTitle) button.setAttribute('title', prevTitle);
                    else button.removeAttribute('title');
                }, 2000);
            };

            if (navigator.clipboard && window.isSecureContext) {
                navigator.clipboard.writeText(text).then(onSuccess).catch(function() {
                    fallbackCopy(text, onSuccess);
                });
            } else {
                fallbackCopy(text, onSuccess);
            }
        };
    }

    window.DocsActions['code:copy'] = function(copyBtn, e) {
        const surface = copyBtn.closest('.code-surface');
        const codeEl = surface ? surface.querySelector('code') : null;
        if (!codeEl) return;

        const text = codeEl.innerText || codeEl.textContent || '';
        window.DocsActions.copyText(text, copyBtn, 'Copied!');
    };

    window.DocsActions['code:download'] = function(downloadBtn, e) {
        const surface = downloadBtn.closest('.code-surface');
        const codeEl = surface ? surface.querySelector('code') : null;
        if (!codeEl) return;

        const text = codeEl.innerText || codeEl.textContent || '';
        const ext = downloadBtn.getAttribute('data-extension') || 'txt';
        const lang = (downloadBtn.getAttribute('data-language') || 'snippet').toLowerCase().replace(/\s+/g, '_');
        const filename = `${lang}_snippet.${ext}`;

        const blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = filename;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    };
})();
