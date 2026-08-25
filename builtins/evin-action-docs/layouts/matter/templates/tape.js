(function() {
    'use strict';

    window.EvinActions = window.EvinActions || {};

    const pauseIconSvg = '<svg class="evin-icon" aria-hidden="true"><use href="#icon-player-pause"></use></svg>';
    const playIconSvg = '<svg class="evin-icon" aria-hidden="true"><use href="#icon-player-play"></use></svg>';
    const copyIconSvg = '<svg class="evin-icon" aria-hidden="true"><use href="#icon-copy"></use></svg>';
    const checkIconSvg = '<svg class="evin-icon" style="color:var(--ui-color-primary)" aria-hidden="true"><use href="#icon-check"></use></svg>';
    const tapePlayers = window.EvinTapePlayers || (window.EvinTapePlayers = new WeakMap());

    function createTapePlayer(tape) {
        const svg = tape.querySelector('.evin-terminal-tape[data-tape-duration-ms]');
        const duration = Number(svg?.dataset.tapeDurationMs);
        const frames = [...(svg?.querySelectorAll('.tape-frame') || [])].map((frame) => ({
            element: frame,
            start: Number(frame.dataset.tapeFrameStartMs),
        }));
        if (!svg || !Number.isFinite(duration) || duration <= 0 || frames.length === 0) return null;

        let animationFrame = 0;
        let paused = false;
        let elapsed = 0;
        let startedAt = performance.now();
        let activeFrame = -1;

        const frameIndexAt = (time) => {
            const loopTime = time % duration;
            for (let index = frames.length - 1; index >= 0; index -= 1) {
                if (frames[index].start <= loopTime) return index;
            }
            return 0;
        };

        const showFrame = (index) => {
            if (index === activeFrame) return;
            frames.forEach((frame, frameIndex) => {
                frame.element.style.display = frameIndex === index ? 'inline' : 'none';
            });
            activeFrame = index;
        };

        const tick = (now) => {
            if (paused) return;
            elapsed = now - startedAt;
            showFrame(frameIndexAt(elapsed));
            animationFrame = requestAnimationFrame(tick);
        };

        showFrame(0);
        animationFrame = requestAnimationFrame(tick);
        return {
            setPaused(nextPaused) {
                if (paused === nextPaused) return;
                paused = nextPaused;
                if (paused) {
                    cancelAnimationFrame(animationFrame);
                } else {
                    startedAt = performance.now() - elapsed;
                    animationFrame = requestAnimationFrame(tick);
                }
            },
        };
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
            if (successful && onSuccess) onSuccess();
        } catch (_) {}
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

    function addRippleEffect(button, event) {
        if (!event) return;
        const circle = document.createElement('span');
        const diameter = Math.max(button.clientWidth, button.clientHeight);
        const radius = diameter / 2;
        const rect = button.getBoundingClientRect();

        circle.style.width = circle.style.height = `${diameter}px`;
        circle.style.left = `${event.clientX - rect.left - radius}px`;
        circle.style.top = `${event.clientY - rect.top - radius}px`;
        circle.classList.add('tape-ripple');

        button.querySelector('.tape-ripple')?.remove();
        button.appendChild(circle);
    }

    function setPlaybackState(button, paused) {
        const tape = button.closest('.terminal-tape');
        if (!tape) return;

        tapePlayers.get(tape)?.setPaused(paused);
        tape.classList.toggle('is-paused', paused);
        button.innerHTML = paused ? playIconSvg : pauseIconSvg;
        button.setAttribute('aria-pressed', String(paused));
        const label = paused ? 'Resume playback' : 'Pause playback';
        button.setAttribute('aria-label', label);
        button.setAttribute('title', label);
    }

    window.EvinActions['tape:toggle-playback'] = function(button, event) {
        if (button.disabled) return;
        addRippleEffect(button, event);
        const tape = button.closest('.terminal-tape');
        setPlaybackState(button, !tape?.classList.contains('is-paused'));
    };

    window.EvinActions['tape:copy'] = function(button, event) {
        if (button.disabled) return;
        const tape = button.closest('.terminal-tape');
        const payload = tape?.querySelector('.tape-copy-script');
        const script = payload?.content.textContent || '';
        if (!script) return;

        addRippleEffect(button, event);
        copyToClipboard(script, function() {
            button.innerHTML = checkIconSvg;
            button.setAttribute('aria-label', 'Copied!');
            button.setAttribute('title', 'Copied!');
            setTimeout(function() {
                button.innerHTML = copyIconSvg;
                button.setAttribute('aria-label', 'Copy commands as a shell script');
                button.setAttribute('title', 'Copy commands as a shell script');
            }, 2000);
        });
    };

    function initializePlayback() {
        if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
            document.querySelectorAll('.tape-playback').forEach(function(button) {
                button.disabled = true;
                button.setAttribute('aria-label', 'Playback disabled because reduced motion is enabled');
                button.setAttribute('title', 'Playback disabled because reduced motion is enabled');
            });
            return;
        }
        document.querySelectorAll('.terminal-tape').forEach(function(tape) {
            if (tapePlayers.has(tape)) return;
            const player = createTapePlayer(tape);
            if (player) tapePlayers.set(tape, player);
        });
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initializePlayback);
    } else {
        initializePlayback();
    }
})();
