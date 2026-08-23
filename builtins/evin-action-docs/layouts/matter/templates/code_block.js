(function() {
    'use strict';

    window.EvinActions = window.EvinActions || {};

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

    function addRippleEffect(button, e) {
        const circle = document.createElement('span');
        const diameter = Math.max(button.clientWidth, button.clientHeight);
        const radius = diameter / 2;
        const rect = button.getBoundingClientRect();

        circle.style.width = circle.style.height = `${diameter}px`;
        circle.style.left = `${e.clientX - rect.left - radius}px`;
        circle.style.top = `${e.clientY - rect.top - radius}px`;
        circle.classList.add('ripple');

        const existing = button.querySelector('.ripple');
        if (existing) existing.remove();

        button.appendChild(circle);
    }

    window.EvinActions['code:copy'] = function(copyBtn, e) {
        if (e) addRippleEffect(copyBtn, e);
        const surface = copyBtn.closest('.code-surface');
        const codeEl = surface ? surface.querySelector('code') : null;
        if (!codeEl) return;

        const text = codeEl.innerText || codeEl.textContent || '';
        copyToClipboard(text, function() {
            copyBtn.innerHTML = checkIconSvg;
            copyBtn.setAttribute('title', 'Copied!');
            setTimeout(function() {
                copyBtn.innerHTML = copyIconSvg;
                copyBtn.setAttribute('title', 'Copy code');
            }, 2000);
        });
    };

    window.EvinActions['code:download'] = function(downloadBtn, e) {
        if (e) addRippleEffect(downloadBtn, e);
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
